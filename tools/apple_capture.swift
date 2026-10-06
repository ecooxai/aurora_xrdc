import Foundation
import ScreenCaptureKit
import VideoToolbox
import CoreMedia
import CoreVideo

struct CaptureOptions {
    var fps = 30
    var bitrateKbps = 3000
    var gopMs = 1000
    var targetHeight = 720

    static func parse() -> CaptureOptions {
        var result = CaptureOptions()
        let args = CommandLine.arguments
        var i = 1
        while i + 1 < args.count {
            let key = args[i]
            let value = args[i + 1]
            switch key {
            case "--fps": result.fps = max(1, min(60, Int(value) ?? result.fps))
            case "--bitrate-kbps": result.bitrateKbps = max(128, min(25000, Int(value) ?? result.bitrateKbps))
            case "--gop-ms": result.gopMs = max(250, min(4000, Int(value) ?? result.gopMs))
            case "--height": result.targetHeight = max(0, Int(value) ?? result.targetHeight)
            default: break
            }
            i += 2
        }
        return result
    }
}

enum CaptureError: Error, CustomStringConvertible {
    case noDisplay
    case compression(OSStatus)
    case property(String, OSStatus)
    var description: String {
        switch self {
        case .noDisplay: return "no ScreenCaptureKit display is available"
        case .compression(let status): return "VideoToolbox compression failed with status \(status)"
        case .property(let key, let status): return "VideoToolbox property \(key) failed with status \(status)"
        }
    }
}

final class AnnexBWriter: @unchecked Sendable {
    private let output = FileHandle.standardOutput
    private let lock = NSLock()
    private let startCode = Data([0, 0, 0, 1])

    func write(_ sampleBuffer: CMSampleBuffer) {
        guard CMSampleBufferDataIsReady(sampleBuffer),
              let block = CMSampleBufferGetDataBuffer(sampleBuffer) else { return }
        lock.lock()
        defer { lock.unlock() }

        var nalHeaderLength: Int32 = 4
        if let format = CMSampleBufferGetFormatDescription(sampleBuffer) {
            var count = 0
            let probe = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                format, parameterSetIndex: 0,
                parameterSetPointerOut: nil, parameterSetSizeOut: nil,
                parameterSetCountOut: &count,
                nalUnitHeaderLengthOut: &nalHeaderLength
            )
            if probe == noErr {
                for index in 0..<count {
                    var pointer: UnsafePointer<UInt8>?
                    var size = 0
                    let status = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                        format, parameterSetIndex: index,
                        parameterSetPointerOut: &pointer, parameterSetSizeOut: &size,
                        parameterSetCountOut: nil, nalUnitHeaderLengthOut: nil
                    )
                    if status == noErr, let pointer, size > 0 {
                        output.write(startCode)
                        output.write(Data(bytes: pointer, count: size))
                    }
                }
            }
        }

        let total = CMBlockBufferGetDataLength(block)
        guard total > 0 else { return }
        var bytes = [UInt8](repeating: 0, count: total)
        let copied = bytes.withUnsafeMutableBytes { raw -> OSStatus in
            guard let base = raw.baseAddress else { return -1 }
            return CMBlockBufferCopyDataBytes(block, atOffset: 0, dataLength: total, destination: base)
        }
        guard copied == kCMBlockBufferNoErr else { return }

        let header = max(1, min(4, Int(nalHeaderLength)))
        var offset = 0
        var packet = Data()
        packet.reserveCapacity(total + 64)
        while offset + header <= bytes.count {
            var size = 0
            for j in 0..<header { size = (size << 8) | Int(bytes[offset + j]) }
            offset += header
            guard size > 0, offset + size <= bytes.count else { break }
            packet.append(startCode)
            packet.append(contentsOf: bytes[offset..<(offset + size)])
            offset += size
        }
        if !packet.isEmpty { output.write(packet) }
    }
}

final class H264Encoder: @unchecked Sendable {
    private let session: VTCompressionSession
    private let writer = AnnexBWriter()

    init(width: Int, height: Int, options: CaptureOptions) throws {
        var created: VTCompressionSession?
        let spec = [kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder as String: true] as CFDictionary
        let status = VTCompressionSessionCreate(
            allocator: kCFAllocatorDefault,
            width: Int32(width), height: Int32(height),
            codecType: kCMVideoCodecType_H264,
            encoderSpecification: spec,
            imageBufferAttributes: nil,
            compressedDataAllocator: nil,
            outputCallback: nil, refcon: nil,
            compressionSessionOut: &created
        )
        guard status == noErr, let created else { throw CaptureError.compression(status) }
        session = created
        try set(kVTCompressionPropertyKey_RealTime, kCFBooleanTrue, "RealTime")
        try set(kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse, "AllowFrameReordering")
        try set(kVTCompressionPropertyKey_AverageBitRate, NSNumber(value: options.bitrateKbps * 1000), "AverageBitRate")
        try set(kVTCompressionPropertyKey_ExpectedFrameRate, NSNumber(value: options.fps), "ExpectedFrameRate")
        try set(kVTCompressionPropertyKey_MaxKeyFrameInterval, NSNumber(value: max(1, options.fps * options.gopMs / 1000)), "MaxKeyFrameInterval")
        try set(kVTCompressionPropertyKey_ProfileLevel, kVTProfileLevel_H264_Main_AutoLevel, "ProfileLevel")
        let prepared = VTCompressionSessionPrepareToEncodeFrames(session)
        if prepared != noErr { throw CaptureError.compression(prepared) }
    }

    deinit {
        VTCompressionSessionCompleteFrames(session, untilPresentationTimeStamp: .invalid)
        VTCompressionSessionInvalidate(session)
    }

    private func set(_ key: CFString, _ value: CFTypeRef, _ name: String) throws {
        let status = VTSessionSetProperty(session, key: key, value: value)
        if status != noErr { throw CaptureError.property(name, status) }
    }

    func encode(_ image: CVImageBuffer, pts: CMTime) {
        var flags = VTEncodeInfoFlags()
        let writer = self.writer
        let status = VTCompressionSessionEncodeFrame(
            session, imageBuffer: image,
            presentationTimeStamp: pts, duration: .invalid,
            frameProperties: nil, infoFlagsOut: &flags
        ) { status, _, sampleBuffer in
            guard status == noErr, let sampleBuffer else { return }
            writer.write(sampleBuffer)
        }
        if status != noErr { fputs("VideoToolbox encode failed: \(status)\n", stderr) }
    }
}

final class CaptureOutput: NSObject, SCStreamOutput, @unchecked Sendable {
    private let encoder: H264Encoder
    init(encoder: H264Encoder) { self.encoder = encoder }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen,
              sampleBuffer.isValid,
              CMSampleBufferDataIsReady(sampleBuffer),
              let image = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        encoder.encode(image, pts: CMSampleBufferGetPresentationTimeStamp(sampleBuffer))
    }
}

@main
struct AppleXRDCCapture {
    static func main() async {
        do {
            let options = CaptureOptions.parse()
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            guard let display = content.displays.first else { throw CaptureError.noDisplay }

            let sourceWidth = max(2, display.width)
            let sourceHeight = max(2, display.height)
            let wantedHeight = options.targetHeight > 0 ? min(options.targetHeight, sourceHeight) : sourceHeight
            let height = max(2, wantedHeight - wantedHeight % 2)
            let scaledWidth = Int((Double(sourceWidth) * Double(height) / Double(sourceHeight)).rounded())
            let width = max(2, scaledWidth - scaledWidth % 2)

            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            config.width = width
            config.height = height
            config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(options.fps))
            config.queueDepth = 5
            config.pixelFormat = kCVPixelFormatType_32BGRA
            config.showsCursor = true

            let encoder = try H264Encoder(width: width, height: height, options: options)
            let output = CaptureOutput(encoder: encoder)
            let queue = DispatchQueue(label: "apple_xrdc.capture.output", qos: .userInteractive)
            let stream = SCStream(filter: filter, configuration: config, delegate: nil)
            try stream.addStreamOutput(output, type: .screen, sampleHandlerQueue: queue)
            try await stream.startCapture()
            fputs("Apple ScreenCaptureKit + VideoToolbox started \(width)x\(height) @ \(options.fps) fps\n", stderr)
            await withCheckedContinuation { (_: CheckedContinuation<Void, Never>) in }
        } catch {
            fputs("apple_xrdc native capture failed: \(error)\n", stderr)
            exit(1)
        }
    }
}
