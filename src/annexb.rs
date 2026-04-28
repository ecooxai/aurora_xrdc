use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::settings::CodecKind;

#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub keyframe: bool,
    pub description_b64: Option<String>,
    pub sequence: u32,
}

pub struct AnnexBParser {
    codec: CodecKind,
    buffer: Vec<u8>,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    vps: Option<Vec<u8>>,
}

impl AnnexBParser {
    pub fn new(codec: CodecKind) -> Self {
        Self {
            codec,
            buffer: Vec::new(),
            sps: None,
            pps: None,
            vps: None,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<EncodedFrame> {
        self.buffer.extend_from_slice(chunk);
        let mut starts = start_codes(&self.buffer);
        if starts.len() < 2 {
            return Vec::new();
        }
        let mut frames = Vec::new();
        let mut frame_starts = Vec::new();
        for &idx in &starts {
            if let Some(nal_type) = nal_type(self.codec, &self.buffer[idx..]) {
                self.capture_headers(nal_type, idx);
                if is_aud(self.codec, nal_type) {
                    frame_starts.push(idx);
                }
            }
        }
        if frame_starts.len() < 2 {
            return Vec::new();
        }
        for pair in frame_starts.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start < end {
                let slice = self.buffer[start..end].to_vec();
                if !contains_vcl(self.codec, &slice) {
                    continue;
                }
                frames.push(EncodedFrame {
                    keyframe: contains_keyframe(self.codec, &slice),
                    description_b64: self.description_b64(),
                    data: sample_data(self.codec, &slice),
                    sequence: 0,
                });
            }
        }
        let drain_to = *frame_starts.last().unwrap_or(&0);
        self.buffer.drain(..drain_to);
        starts.clear();
        frames
    }

    fn capture_headers(&mut self, nal_type: u8, start: usize) {
        let end = next_start(&self.buffer, start + 3).unwrap_or(self.buffer.len());
        let unit = nal_payload(&self.buffer[start..end]).to_vec();
        match (self.codec, nal_type) {
            (CodecKind::H264, 7) => self.sps = Some(unit),
            (CodecKind::H264, 8) => self.pps = Some(unit),
            (CodecKind::H265, 32) => self.vps = Some(unit),
            (CodecKind::H265, 33) => self.sps = Some(unit),
            (CodecKind::H265, 34) => self.pps = Some(unit),
            _ => {}
        }
    }

    fn description_b64(&self) -> Option<String> {
        match self.codec {
            CodecKind::H264 => {
                let sps = self.sps.as_ref()?;
                let pps = self.pps.as_ref()?;
                Some(STANDARD.encode(avcc_from_sps_pps(sps, pps)))
            }
            // WebCodecs accepts HEVC Annex B access units when no `description`
            // is provided. That is a better fit for our live elementary stream
            // than fabricating an hvcC blob from partial parser state.
            CodecKind::H265 => None,
            CodecKind::Vp8 | CodecKind::Vp9 | CodecKind::Av1 => None,
        }
    }
}

pub struct IvfParser {
    codec: CodecKind,
    header_seen: bool,
    buffer: Vec<u8>,
}

impl IvfParser {
    pub fn new(codec: CodecKind) -> Self {
        Self {
            codec,
            header_seen: false,
            buffer: Vec::new(),
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Vec<EncodedFrame> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        if !self.header_seen {
            if self.buffer.len() < 32 {
                return frames;
            }
            self.buffer.drain(..32);
            self.header_seen = true;
        }
        loop {
            if self.buffer.len() < 12 {
                break;
            }
            let len = u32::from_le_bytes(self.buffer[..4].try_into().unwrap()) as usize;
            if self.buffer.len() < 12 + len {
                break;
            }
            let data = self.buffer[12..12 + len].to_vec();
            frames.push(EncodedFrame {
                keyframe: ivf_keyframe(self.codec, &data),
                description_b64: None,
                data,
                sequence: 0,
            });
            self.buffer.drain(..12 + len);
        }
        frames
    }
}

fn ivf_keyframe(codec: CodecKind, data: &[u8]) -> bool {
    match codec {
        CodecKind::Vp8 => data.first().map(|byte| byte & 0x01 == 0).unwrap_or(false),
        CodecKind::Vp9 => data.first().map(|byte| byte & 0x04 == 0).unwrap_or(false),
        CodecKind::Av1 => av1_temporal_unit_is_keyframe(data),
        CodecKind::H264 | CodecKind::H265 => false,
    }
}

fn av1_temporal_unit_is_keyframe(data: &[u8]) -> bool {
    let mut offset = 0;
    while offset < data.len() {
        let header = data[offset];
        offset += 1;
        let obu_type = (header >> 3) & 0x0f;
        let has_extension = header & 0x04 != 0;
        let has_size = header & 0x02 != 0;
        if has_extension {
            if offset >= data.len() {
                return false;
            }
            offset += 1;
        }
        let payload_len = if has_size {
            let Some((value, bytes_read)) = read_leb128(&data[offset..]) else {
                return false;
            };
            offset += bytes_read;
            value
        } else {
            data.len().saturating_sub(offset)
        };
        let end = offset.saturating_add(payload_len);
        if end > data.len() {
            return false;
        }
        if matches!(obu_type, 3 | 6) && av1_frame_header_is_keyframe(&data[offset..end]) {
            return true;
        }
        offset = end;
    }
    false
}

fn av1_frame_header_is_keyframe(payload: &[u8]) -> bool {
    let Some(&first) = payload.first() else {
        return false;
    };
    let show_existing_frame = first & 0x01 != 0;
    let frame_type = (first >> 1) & 0x03;
    !show_existing_frame && frame_type == 0
}

fn read_leb128(data: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0usize;
    for (idx, byte) in data.iter().copied().take(8).enumerate() {
        value |= ((byte & 0x7f) as usize) << (idx * 7);
        if byte & 0x80 == 0 {
            return Some((value, idx + 1));
        }
    }
    None
}

fn start_codes(data: &[u8]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut idx = 0;
    while idx + 3 < data.len() {
        if data[idx..].starts_with(&[0, 0, 0, 1]) {
            starts.push(idx);
            idx += 4;
            continue;
        }
        if data[idx..].starts_with(&[0, 0, 1]) {
            starts.push(idx);
            idx += 3;
            continue;
        }
        idx += 1;
    }
    starts
}

fn next_start(data: &[u8], from: usize) -> Option<usize> {
    let mut idx = from;
    while idx + 3 < data.len() {
        if data[idx..].starts_with(&[0, 0, 0, 1]) || data[idx..].starts_with(&[0, 0, 1]) {
            return Some(idx);
        }
        idx += 1;
    }
    None
}

fn nal_payload(data: &[u8]) -> &[u8] {
    if data.starts_with(&[0, 0, 0, 1]) {
        &data[4..]
    } else if data.starts_with(&[0, 0, 1]) {
        &data[3..]
    } else {
        data
    }
}

fn annexb_sample_to_length_prefixed(data: &[u8]) -> Vec<u8> {
    let starts = start_codes(data);
    if starts.is_empty() {
        return data.to_vec();
    }
    let mut out = Vec::with_capacity(data.len());
    for (idx, start) in starts.iter().enumerate() {
        let end = starts.get(idx + 1).copied().unwrap_or(data.len());
        let unit = nal_payload(&data[*start..end]);
        if unit.is_empty() {
            continue;
        }
        out.extend_from_slice(&(unit.len() as u32).to_be_bytes());
        out.extend_from_slice(unit);
    }
    out
}

fn nal_type(codec: CodecKind, data: &[u8]) -> Option<u8> {
    let payload = nal_payload(data);
    let first = *payload.first()?;
    Some(match codec {
        CodecKind::H264 => first & 0x1f,
        CodecKind::H265 => (first >> 1) & 0x3f,
        CodecKind::Vp8 | CodecKind::Vp9 | CodecKind::Av1 => return None,
    })
}

fn is_aud(codec: CodecKind, nal_type: u8) -> bool {
    matches!(
        (codec, nal_type),
        (CodecKind::H264, 9) | (CodecKind::H265, 35)
    )
}

fn contains_keyframe(codec: CodecKind, data: &[u8]) -> bool {
    start_codes(data)
        .into_iter()
        .any(|idx| match nal_type(codec, &data[idx..]) {
            Some(5) if codec == CodecKind::H264 => true,
            Some(19 | 20 | 21) if codec == CodecKind::H265 => true,
            _ => false,
        })
}

fn contains_vcl(codec: CodecKind, data: &[u8]) -> bool {
    start_codes(data)
        .into_iter()
        .any(|idx| match nal_type(codec, &data[idx..]) {
            Some(1..=5) if codec == CodecKind::H264 => true,
            Some(0..=31) if codec == CodecKind::H265 => true,
            _ => false,
        })
}

fn avcc_from_sps_pps(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut out = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
    out.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    out.extend_from_slice(sps);
    out.push(1);
    out.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    out.extend_from_slice(pps);
    out
}

fn sample_data(codec: CodecKind, sample: &[u8]) -> Vec<u8> {
    match codec {
        CodecKind::H264 => annexb_sample_to_length_prefixed(sample),
        CodecKind::H265 => sample.to_vec(),
        CodecKind::Vp8 | CodecKind::Vp9 | CodecKind::Av1 => sample.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::{AnnexBParser, IvfParser, annexb_sample_to_length_prefixed};
    use crate::settings::CodecKind;

    #[test]
    fn parses_ivf_frames() {
        let mut parser = IvfParser::new(CodecKind::Vp8);
        let mut data = b"DKIF".to_vec();
        data.resize(32, 0);
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&[0, 1, 2, 3]);
        let frames = parser.push(&data);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].keyframe);
    }

    #[test]
    fn parses_av1_ivf_keyframes_from_frame_obus() {
        let mut parser = IvfParser::new(CodecKind::Av1);
        let mut data = b"DKIF".to_vec();
        data.resize(32, 0);
        let frame = [0x32, 0x01, 0x00];
        data.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&frame);
        let frames = parser.push(&data);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].keyframe);
    }

    #[test]
    fn parses_av1_ivf_delta_frames_from_frame_obus() {
        let mut parser = IvfParser::new(CodecKind::Av1);
        let mut data = b"DKIF".to_vec();
        data.resize(32, 0);
        let frame = [0x32, 0x01, 0x02];
        data.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&frame);
        let frames = parser.push(&data);
        assert_eq!(frames.len(), 1);
        assert!(!frames[0].keyframe);
    }

    #[test]
    fn parses_h264_frames_with_aud() {
        let mut parser = AnnexBParser::new(CodecKind::H264);
        let frame1 = [
            &[0, 0, 0, 1, 0x67, 0x64, 0, 0x1f][..],
            &[0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80][..],
            &[0, 0, 0, 1, 0x09, 0x10][..],
            &[0, 0, 0, 1, 0x65, 1, 2, 3][..],
            &[0, 0, 0, 1, 0x09, 0x10][..],
            &[0, 0, 0, 1, 0x61, 4, 5, 6][..],
        ]
        .concat();
        let frames = parser.push(&frame1);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].keyframe);
        assert!(frames[0].description_b64.is_some());
    }

    #[test]
    fn keeps_h265_frames_in_annex_b_without_description() {
        let mut parser = AnnexBParser::new(CodecKind::H265);
        let frame1 = [
            &[0, 0, 0, 1, 0x40, 0x01, 0xaa][..],    // VPS (type 32)
            &[0, 0, 0, 1, 0x42, 0x01, 0xbb][..],    // SPS (type 33)
            &[0, 0, 0, 1, 0x44, 0x01, 0xcc][..],    // PPS (type 34)
            &[0, 0, 0, 1, 0x46, 0x01][..],          // AUD (type 35)
            &[0, 0, 0, 1, 0x26, 0x01, 1, 2, 3][..], // IDR_W_RADL (type 19)
            &[0, 0, 0, 1, 0x46, 0x01][..],          // AUD (type 35)
            &[0, 0, 0, 1, 0x02, 0x01, 4, 5, 6][..], // TRAIL_R (type 1)
        ]
        .concat();

        let frames = parser.push(&frame1);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].keyframe);
        assert!(frames[0].description_b64.is_none());
        assert!(frames[0].data.starts_with(&[0, 0, 0, 1, 0x46, 0x01]));
    }

    #[test]
    fn converts_annex_b_sample_to_length_prefixed_units() {
        let sample = [
            &[0, 0, 0, 1, 0x09, 0x10][..],
            &[0, 0, 0, 1, 0x67, 0x64, 0, 0x1f][..],
            &[0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80][..],
            &[0, 0, 0, 1, 0x65, 1, 2, 3][..],
        ]
        .concat();
        let converted = annexb_sample_to_length_prefixed(&sample);
        assert_eq!(&converted[..4], &(2u32.to_be_bytes()));
        assert_eq!(&converted[4..6], &[0x09, 0x10]);
        assert_eq!(&converted[6..10], &(4u32.to_be_bytes()));
        assert_eq!(&converted[10..14], &[0x67, 0x64, 0, 0x1f]);
    }

    #[test]
    fn ignores_repeated_h264_auds_without_vcl_payload() {
        let mut parser = AnnexBParser::new(CodecKind::H264);
        let sample = [
            &[0, 0, 0, 1, 0x09, 0x10][..],
            &[0, 0, 0, 1, 0x09, 0x10][..],
            &[0, 0, 0, 1, 0x67, 0x64, 0, 0x1f][..],
            &[0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80][..],
            &[0, 0, 0, 1, 0x65, 1, 2, 3][..],
            &[0, 0, 0, 1, 0x09, 0x10][..],
            &[0, 0, 0, 1, 0x09, 0x10][..],
            &[0, 0, 0, 1, 0x61, 4, 5, 6][..],
        ]
        .concat();
        let frames = parser.push(&sample);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].keyframe);
        assert!(frames[0].description_b64.is_some());
    }
}
