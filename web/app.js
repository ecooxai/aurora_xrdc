const $ = (id) => document.getElementById(id);
const SETTINGS_STORAGE_KEY = "vibe_rdesk.settings";
const API_ORIGIN_STORAGE_KEY = "vibe_rdesk.api_origin";
const PASSWD_STORAGE_KEY = "vibe_rdesk.passwd";
const TOUCH_LONG_PRESS_MS = 1000;
const TOUCH_MOVE_CANCEL_PX = 14;
const DIRECT_TOUCH_SCROLL_MULTIPLIER = 3;
const SMART_TOUCH_DRAG_HOLD_MS = 900;
const SMART_TOUCH_RIGHT_CLICK_MS = 5000;
const SMART_TOUCH_SCROLL_MULTIPLIER = 0.9;
const CLIPBOARD_HISTORY_LIMIT = 100;
const VIEW_ZOOM_STEP_PERCENT = 10;
const VIEW_ZOOM_MIN_PERCENT = 10;
const VIEW_ZOOM_MAX_PERCENT = 300;
const LATENCY_PROBE_INTERVAL_MS = 3000;
const DEFAULT_LIVE_MEDIA_MAX_AGE_MS = 2000;
const DEFAULT_MEDIA_STALL_RESET_MS = 2500;
const HIGH_LATENCY_RECONNECT_MS = 1500;
const HIGH_LATENCY_RECONNECT_GRACE_MS = 5000;
const HEALTH_WATCHDOG_INTERVAL_MS = 1000;
const ENABLE_VIDEO_RENDER_WORKER = true;
const POINTER_FLUSH_INTERVAL_MS = 8;
const TWO_FINGER_TAP_MAX_MS = 450;
const TWO_FINGER_TAP_MOVE_PX = 24;
const RECOVERY_TAP_WINDOW_MS = 1600;
const RECOVERY_TAP_SAME_AREA_PX = 70;
const KEY_STATE_SYNC_INTERVAL_MS = 500;
const MOBILE_KEYBOARD_COMPOSITION_IDLE_FLUSH_MS = 700;
const DEFAULT_MAX_VIDEO_DECODE_QUEUE = 6;
const MAX_AUDIO_DECODE_QUEUE = 24;
const AUDIO_MIN_BUFFER_SECONDS = 0.05;
const AUDIO_UNDERRUN_RETRY_MS = 2000;
const AUDIO_UNDERRUN_RESUME_SECONDS = 2;
const AUDIO_UNDERRUN_POLL_MS = 100;
const AUDIO_REBUFFER_EXTRA_SECONDS = 0.3;
const AUDIO_REBUFFER_HOLD_MS = 10000;
const AUDIO_TRIM_LATENCY_EXTRA_SECONDS = 0.3;
const AUDIO_TRIM_LATENCY_HOLD_MS = 20000;
const AUDIO_TRIM_TARGET_EXTRA_SECONDS = 0.2;
const AUDIO_GOOD_LATENCY_BUFFER_SECONDS = 0.75;
const AUDIO_UNDERRUN_WARNING = "Audio buffer too small, pausing playback";
const MESSAGE_LIMIT = 120;
const AAC_SAMPLES_PER_FRAME = 1024;
const AUDIO_BASE_PLAYBACK_RATE = 1.0000;
const DEFAULT_AUDIO_LATENCY_MS = 400;
const AUDIO_AUTO_CLOCK_STEP = 0.005;
const AUDIO_AUTO_CLOCK_SLOW_TUNE_STEP = 0.0005;
const AUDIO_AUTO_CLOCK_SLOW_TUNE_TARGET_EXTRA_SECONDS = 0.2;
const AUDIO_AUTO_CLOCK_SLOW_TUNE_INTERVAL_MS = 8000;
const AUDIO_DRIFT_SLOWDOWN_MAX = 0.05;
const AUDIO_DRIFT_SPEEDUP_MAX = 0.02;
const AUDIO_DRIFT_CORRECTION_DEADZONE_SECONDS = 0.015;
const AUDIO_DRIFT_PROPORTIONAL_GAIN = 0.02;
const AUDIO_DRIFT_INTEGRAL_GAIN = 0.015;
const AUDIO_DRIFT_INTEGRAL_MAX = 0.03;
const AUTO_DISCONNECT_DISABLED_MINUTES = 0;
const AUTO_DISCONNECT_ACTIVITY_REFRESH_MS = 1000;
const SETTINGS_RECONNECT_DELAY_MS = 3000;
const CLIPBOARD_IMAGE_LIMIT_BYTES = 8 * 1024 * 1024;
const DEFAULT_CODEC_OPTIONS = [
  { value: "h264", label: "H.264" },
  { value: "h265", label: "H.265" },
  { value: "vp8", label: "VP8" },
  { value: "vp9", label: "VP9" },
  { value: "av1", label: "AV1" },
];
const KNOWN_ENCODE_PREFERENCE_VALUES = new Set([
  "gpu",
  "cpu",
  "nvidia",
  "h264_nvenc",
  "h264_qsv",
  "h264_vaapi",
  "libx264",
  "hevc_nvenc",
  "hevc_qsv",
  "hevc_vaapi",
  "libx265",
  "libvpx",
  "vp9_qsv",
  "vp9_vaapi",
  "libvpx-vp9",
  "av1_nvenc",
  "av1_qsv",
  "av1_vaapi",
  "libsvtav1",
  "libaom-av1",
]);
const CPU_ENCODE_PREFERENCE_VALUES = new Set([
  "cpu",
  "libx264",
  "libx265",
  "libvpx",
  "libvpx-vp9",
  "libsvtav1",
  "libaom-av1",
]);
const AUTO_DISCONNECT_ACTIVITY_MESSAGE_TYPES = new Set([
  "key",
  "key_state",
  "pointer_absolute",
  "pointer_button",
  "pointer_move",
  "pointer_wheel",
  "text_input",
]);
const INPUT_SOCKET_MESSAGE_TYPES = new Set([
  "key",
  "key_state",
  "pointer_absolute",
  "pointer_button",
  "pointer_move",
  "pointer_wheel",
  "touch_tap",
  "text_input",
  "paste",
  "paste_clipboard",
  "reset_input",
]);
const state = {
  socket: null,
  inputSocket: null,
  videoSocket: null,
  audioSocket: null,
  micSocket: null,
  decoder: null,
  audioDecoder: null,
  audioContext: null,
  audioGainNode: null,
  audioSources: new Set(),
  micRecorder: null,
  micAudioContext: null,
  micStream: null,
  micSourceNode: null,
  micHighpassNode: null,
  micLowpassNode: null,
  micCompressorNode: null,
  micProcessorNode: null,
  micSilenceNode: null,
  micStreamId: 0,
  micDeviceId: "",
  micDevices: [],
  micDeviceMenuOpen: false,
  micEnabled: false,
  micStarting: false,
  cameraRecorder: null,
  cameraStream: null,
  cameraEnabled: false,
  cameraStarting: false,
  cameraUploadTail: Promise.resolve(),
  cameraSeq: 0,
  cameraMimeType: "",
  sessionId: "",
  activeCodec: "h264",
  codecString: "avc1.64001f",
  description: null,
  decoderConfigKey: "",
  audioConfigKey: "",
  audioEnabled: false,
  pendingVideoFrame: null,
  renderingVideoFrame: false,
  videoRenderRaf: 0,
  videoRenderWorker: null,
  audioNextTime: 0,
  pendingEncodedAudioFrames: [],
  pendingAudioBuffers: [],
  pendingAudioDuration: 0,
  audioDecodingDuration: 0,
  audioResumeTimer: 0,
  audioResumeBlockedUntil: 0,
  audioUnderrunActive: false,
  audioPlaybackBlocked: false,
  audioUserActivated: true,
  audioMuted: false,
  audioVolumePercent: 100,
  audioLargeBufferSinceAt: 0,
  audioHighLatencySinceAt: 0,
  audioRateIntegral: 0,
  audioRateLastUpdatedAt: 0,
  audioClockAutoLastIncreaseAt: 0,
  audioClockAutoLastIncreaseLead: 0,
  audioClockAutoLastSlowTuneAt: 0,
  audioUseRealOutput: false,
  decoderRecovering: false,
  waitingForKeyframe: true,
  frameCount: 0,
  bytesReceived: 0,
  netWindowBytes: 0,
  lastNetAt: performance.now(),
  netKbps: 0,
  manualDisconnect: false,
  reconnectTimer: null,
  settingsReconnectTimer: null,
  reconnectAttempt: 0,
  pendingPointerMotion: null,
  pointerFlushTimer: 0,
  touchPointers: new Map(),
  touchLongPressTimer: 0,
  touchLongPressPointerId: null,
  touchDragPointerId: null,
  touchScrollLastY: null,
  touchTwoFingerTap: null,
  smartTouchDragTimer: 0,
  smartTouchRightClickTimer: 0,
  smartTouchAction: null,
  smartTouchScrollLastX: null,
  smartTouchScrollLastY: null,
  lastControlTab: "status",
  recoveryTapCount: 0,
  recoveryTapLastAt: 0,
  recoveryTapCenter: null,
  inputCaptured: false,
  pressedKeys: new Set(),
  tappedPrintableKeys: new Set(),
  modifierChordKeys: new Set(),
  mobileKeyboardComposing: false,
  mobileKeyboardFlushTimer: 0,
  keyStateSyncTimer: 0,
  localClipboard: { text: null, image_png_b64: null },
  remoteClipboard: { text: null, image_png_b64: null },
  localClipboardSig: "",
  remoteClipboardSig: "",
  localClipboardUpdatedAt: 0,
  remoteClipboardUpdatedAt: 0,
  clipboardHistory: [],
  messages: [],
  activeMessageLevel: "error",
  encoderOptionsByCodec: {},
  lastEncoderStatusMessage: "",
  apiOrigin: window.location.origin,
  authenticated: false,
  sessionPasswd: "",
  connecting: false,
  remoteScreenWidth: null,
  remoteScreenHeight: null,
  videoFrameWidth: null,
  videoFrameHeight: null,
  viewZoomPercent: 100,
  viewCssWidthMode: "render",
  viewCustomCssWidth: null,
  wsLatencyMs: null,
  latencyProbeSeq: 0,
  latencyProbeSentAt: new Map(),
  serverClockOffsetMs: 0,
  lastVideoSeq: null,
  lastVideoPacketAt: 0,
  lastVideoFrameRenderedAt: 0,
  lastAudioPacketAt: 0,
  streamWarning: "",
  lastStaleDropAt: 0,
  liveMediaMaxAgeMs: DEFAULT_LIVE_MEDIA_MAX_AGE_MS,
  mediaStallResetMs: DEFAULT_MEDIA_STALL_RESET_MS,
  maxVideoDecodeQueue: DEFAULT_MAX_VIDEO_DECODE_QUEUE,
  highLatencySinceAt: 0,
  reconnectingForLatency: false,
  appliedStreamSettingsKey: "",
  pendingStreamSettingsKey: "",
  lastLocalStreamSettingsAt: 0,
  statusTimer: null,
  autoDisconnectTimer: null,
  lastAutoDisconnectActivityAt: 0,
  webclientsManagerOpen: false,
  webclients: [],
  transport: "websocket",
  wtInfo: null,
  protocolMenuOpen: false,
};

const status = $("status");
const viewportCard = $("viewport-card");
const canvas = $("screen");
let ctx = null;
const toast = $("toast");
const streamWarning = $("stream-warning");
const streamWarningText = $("stream-warning-text");
const authModal = $("auth-modal");
const authForm = $("auth-form");
const authOriginInput = $("auth-origin");
const authInput = $("auth-passwd");
const authError = $("auth-error");
const controlPanel = $("control-panel");
const controlTrigger = $("control-trigger");
const controlQuickTab = $("control-quick-tab");
const transferPanel = $("transfer-panel");
const audioPanel = $("audio-panel");
const errorPanel = $("error-panel");
const errorCount = $("error-count");
const errorList = $("error-list");
const errorClear = $("error-clear");
const messageTabButtons = Array.from(document.querySelectorAll("[data-message-level]"));
const messageErrorCount = $("message-error-count");
const messageInfoCount = $("message-info-count");
const mobileKeyboardTrigger = $("mobile-keyboard-trigger");
const micToggle = $("mic-toggle");
const micDeviceMenu = $("mic-device-menu");
const audioToggle = $("audio-toggle");
const cameraToggle = $("camera-toggle");
const fullscreenToggle = $("fullscreen-toggle");
const mobileKeyboardInput = $("mobile-keyboard-input");
const encoderStatus = $("encoder-status");
const codecSelect = $("codec");
const codecGroup = $("codec-group");
const encodePreferenceSelect = $("encode-preference");
const encodePreferenceGroup = $("encode-preference-group");
const encodePreferenceHelp = $("encode-preference-help");
const bitrateInput = $("bitrate");
const bitrateValue = $("bitrate-value");
const audioBitrateSelect = $("audio-bitrate");
const micBitrateSelect = $("mic-bitrate");
const fpsInput = $("fps");
const fpsValue = $("fps-value");
const scrollSpeedInput = $("scroll-speed");
const scrollSpeedValue = $("scroll-speed-value");
const audioLatencyInput = $("audio-latency");
const audioLatencyValue = $("audio-latency-value");
const audioVolumeInput = $("audio-volume");
const audioVolumeValue = $("audio-volume-value");
const audioMuteInput = $("audio-muted");
const audioClockRateInput = $("audio-clock-rate");
const audioClockRateValue = $("audio-clock-rate-value");
const audioClockAutoInput = $("audio-clock-auto");
const audioRealOutputInput = $("audio-real-output");
const audioOutputStatus = $("audio-output-status");
const autoDisconnectMinutesInput = $("auto-disconnect-minutes");
const encoderLatencySelect = $("encoder-latency");
const encoderQualitySelect = $("encoder-quality");
const videoScaleSelect = $("video-scale");
const gopMsInput = $("gop-ms");
const gopMsValue = $("gop-ms-value");
const bufferMsInput = $("buffer-ms");
const bufferMsValue = $("buffer-ms-value");
const staleDropMsInput = $("stale-drop-ms");
const staleDropMsValue = $("stale-drop-ms-value");
const stallResetMsInput = $("stall-reset-ms");
const stallResetMsValue = $("stall-reset-ms-value");
const decodeQueueInput = $("decode-queue");
const decodeQueueValue = $("decode-queue-value");
const performancePresetSelect = $("performance-preset");
const touchModeSelect = $("touch-mode");
const touchModeGroup = $("touch-mode-group");
const directTouchScrollInput = $("direct-touch-scroll");
const directTouchScrollLabel = $("direct-touch-scroll-label");
const uploadAction = $("upload-action");
const uploadInput = $("upload-input");
const tabButtons = Array.from(document.querySelectorAll(".tab-button"));
const tabPanels = Array.from(document.querySelectorAll(".tab-panel"));
const controlMenuTabButtons = Array.from(document.querySelectorAll("[data-control-tab]"));
const statusCpu = $("status-cpu");
const statusRam = $("status-ram");
const statusSwap = $("status-swap");
const statusLatency = $("status-latency");
const statusSpeedDownload = $("status-speed-download");
const statusSpeedUpload = $("status-speed-upload");
const statusAudioBuffer = $("status-audio-buffer");
const statusUpdatedAt = $("status-updated-at");
const webclientsToggle = $("webclients-toggle");
const webclientsCount = $("webclients-count");
const webclientsManager = $("webclients-manager");
const webclientsList = $("webclients-list");
const webclientsCloseOthers = $("webclients-close-others");
const protocolToggle = $("protocol-toggle");
const protocolValue = $("protocol-value");
const protocolMenu = $("protocol-menu");
const localClipboardSyncBtn = $("local-clipboard-sync-btn");
const remoteClipboardSyncBtn = $("remote-clipboard-sync-btn");
const clipboardHistoryList = $("clipboard-history-list");
const clipboardHistoryEmpty = $("clipboard-history-empty");
const viewCanvasRenderSize = $("view-canvas-render-size");
const viewCanvasCssSize = $("view-canvas-css-size");
const viewStreamSize = $("view-stream-size");
const viewViewportSize = $("view-viewport-size");
const viewWindowSize = $("view-window-size");
const viewRemoteScreenSize = $("view-remote-screen-size");
const viewZoomValue = $("view-zoom-value");
const viewCanvasRenderAction = $("view-canvas-render-action");
const viewCanvasCssAction = $("view-canvas-css-action");
const viewRemoteScreenAction = $("view-remote-screen-action");
const viewStreamAction = $("view-stream-action");
const viewViewportAction = $("view-viewport-action");
const viewWindowAction = $("view-window-action");
const zoomOutButton = $("zoom-out");
const zoomInButton = $("zoom-in");
const AAC_SAMPLE_RATES = [
  96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050,
  16000, 12000, 11025, 8000, 7350,
];
const AUDIO_BUFFER_PROFILES = [
  { minLatencyMs: 200, targetLeadSeconds: 0.5, maxQueueSeconds: 0.9, resetGraceSeconds: 0.14 },
  { minLatencyMs: 100, targetLeadSeconds: 0.36, maxQueueSeconds: 0.72, resetGraceSeconds: 0.12 },
  { minLatencyMs: 50, targetLeadSeconds: 0.24, maxQueueSeconds: 0.52, resetGraceSeconds: 0.1 },
  { minLatencyMs: 0, targetLeadSeconds: 0.14, maxQueueSeconds: 0.34, resetGraceSeconds: 0.08 },
];
const MIC_CHUNK_KIND = 3;
const MIC_STREAM_ID_BYTES = 4;
const MIC_HEADER_BYTES = 1 + MIC_STREAM_ID_BYTES;
const MIC_CHUNK_MS = 100;
const MIC_MIME_CANDIDATES = [
  "audio/webm;codecs=opus",
  "audio/webm",
];
const CAMERA_CHUNK_MS = 1000;
const CAMERA_MIME_CANDIDATES = [
  "video/mp4;codecs=avc1.42E01E",
  "video/mp4;codecs=avc1",
  "video/mp4",
  "video/webm;codecs=vp8",
  "video/webm;codecs=vp9",
  "video/webm",
];
const PERFORMANCE_PRESETS = {
  speed: {
    codec: "h264",
    bitrate: 3000,
    fps: 30,
    encoderLatency: "low",
    encoderQuality: "balanced",
    videoScale: "720p",
    gopMs: 1000,
    bufferMs: 1000,
    audioLatencyMs: DEFAULT_AUDIO_LATENCY_MS,
    audioClockRate: AUDIO_BASE_PLAYBACK_RATE,
    staleDropMs: 2000,
    stallResetMs: 2500,
    maxDecodeQueue: 6,
  },
  quality: {
    codec: "h264",
    bitrate: 10000,
    fps: 30,
    encoderLatency: "balanced",
    encoderQuality: "sharp_text",
    videoScale: "1080p",
    gopMs: 1000,
    bufferMs: 500,
    staleDropMs: 700,
    stallResetMs: 900,
    maxDecodeQueue: 3,
  },
};
const VIDEO_CODEC_STRINGS = {
  h264: "avc1.64001f",
  h265: "hvc1.1.6.L93.B0",
  vp8: "vp8",
  vp9: "vp09.00.10.08",
  av1: "av01.0.08M.08",
};
const MOBILE_KEYBOARD_SPECIAL_KEYS = new Set([
  "Backspace",
  "Delete",
  "Enter",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Tab",
  "Escape",
  "Home",
  "End",
  "PageUp",
  "PageDown",
]);

function clearStatusTimer() {
  if (!state.statusTimer) return;
  clearTimeout(state.statusTimer);
  state.statusTimer = null;
}

function setStatus(text, { hideAfterMs = 0 } = {}) {
  clearStatusTimer();
  status.textContent = text;
  status.classList.toggle("hidden", text === "Connected");
  if (hideAfterMs > 0) {
    state.statusTimer = setTimeout(() => {
      state.statusTimer = null;
      if (status.textContent !== text) return;
      setStatus(isConnected() ? "Connected" : "Disconnected");
    }, hideAfterMs);
  }
}

function streamMessageCode(message = "") {
  if (message.startsWith("Audio")) return "audio_buffer";
  if (message.startsWith("Video render") || message.startsWith("Video worker")) return "video_render";
  if (message.startsWith("Video decoder") || message.startsWith("Video frame")) return "video_decode";
  if (message.startsWith("Video")) return "video_stream";
  if (/latency/i.test(message)) return "stream_latency";
  return "stream_warning";
}

function setStreamWarning(message = "") {
  const previous = state.streamWarning;
  state.streamWarning = message;
  streamWarning.classList.add("hidden");
  streamWarningText.textContent = "Stream delayed";
  if (message && message !== previous) {
    pushDebug(streamMessageCode(message), message, { throttleMs: 2000 });
  }
}

function setEncoderStatus(text) {
  const statusText = text || "Not connected";
  encoderStatus.textContent = statusText;
  if (state.lastEncoderStatusMessage === statusText) return;
  state.lastEncoderStatusMessage = statusText;
  pushInfo("video_encode", statusText, { throttleMs: 3000 });
}

function formatPercent(value) {
  if (!Number.isFinite(value)) return "--";
  return `${value.toFixed(value >= 10 ? 0 : 1)}%`;
}

function formatMbPerSecond(kbps) {
  if (!Number.isFinite(kbps) || kbps < 0) return "--";
  return `${(kbps / 1000).toFixed(kbps >= 1000 ? 2 : 1)} Mb/s`;
}

function formatMemoryUsage(usedMb, totalMb) {
  if (!Number.isFinite(usedMb) || usedMb < 0) return "--";
  if (!Number.isFinite(totalMb) || totalMb <= 0) {
    return `${usedMb} MB`;
  }
  const usedGb = usedMb / 1024;
  const totalGb = totalMb / 1024;
  const percent = (usedMb / totalMb) * 100;
  return `${usedGb.toFixed(1)} / ${totalGb.toFixed(1)} GB (${percent.toFixed(0)}%)`;
}

function renderStatusMetrics({
  cpu_usage,
  memory_used_mb,
  memory_total_mb,
  swap_used_mb,
  swap_total_mb,
  net_tx_kbps,
  net_rx_kbps,
} = {}) {
  statusCpu.textContent = formatPercent(cpu_usage);
  statusRam.textContent = formatMemoryUsage(memory_used_mb, memory_total_mb);
  statusSwap.textContent = formatMemoryUsage(swap_used_mb, swap_total_mb);
  renderLatencyMetric();
  renderAudioBufferMetric();
  statusSpeedDownload.textContent = `↓ ${formatMbPerSecond(net_rx_kbps)}`;
  statusSpeedUpload.textContent = `↑ ${formatMbPerSecond(net_tx_kbps)}`;
  statusUpdatedAt.textContent = `Updated ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`;
}

function renderLatencyMetric() {
  statusLatency.textContent = Number.isFinite(state.wsLatencyMs)
    ? `${Math.round(state.wsLatencyMs)} ms`
    : "--";
}

function pendingEncodedAudioSeconds() {
  return state.pendingEncodedAudioFrames.reduce(
    (total, frame) => total + (Number(frame.durationSeconds) || 0),
    0,
  );
}

function currentDecodedAudioSeconds() {
  const audioContext = state.audioContext;
  const now = Number.isFinite(audioContext?.currentTime) ? audioContext.currentTime : 0;
  const queuedFor = state.audioNextTime > now ? state.audioNextTime - now : 0;
  return Math.max(0, queuedFor + state.pendingAudioDuration + state.audioDecodingDuration);
}

function currentBufferedAudioSeconds() {
  return currentDecodedAudioSeconds() + pendingEncodedAudioSeconds();
}

function renderAudioBufferMetric() {
  if (!audioPanel.open) {
    return;
  }
  if (state.audioMuted) {
    statusAudioBuffer.textContent = "Muted";
    return;
  }
  if (!state.audioEnabled) {
    statusAudioBuffer.textContent = "--";
    return;
  }
  statusAudioBuffer.textContent = `${Math.round(currentBufferedAudioSeconds() * 1000)} ms`;
}

function resetStatusMetrics() {
  state.wsLatencyMs = null;
  state.latencyProbeSentAt.clear();
  renderStatusMetrics({});
  statusUpdatedAt.textContent = "Stats and latency every 3s";
  state.highLatencySinceAt = 0;
  state.lastStaleDropAt = 0;
  setStreamWarning("");
}

function updateServerClockOffset(serverTimeMs, roundTripMs = 0) {
  if (!Number.isFinite(serverTimeMs) || serverTimeMs <= 0) return;
  const sample = Date.now() - serverTimeMs - Math.max(0, roundTripMs) / 2;
  if (!Number.isFinite(state.serverClockOffsetMs) || state.serverClockOffsetMs === 0) {
    state.serverClockOffsetMs = sample;
    return;
  }
  state.serverClockOffsetMs = (state.serverClockOffsetMs * 0.8) + (sample * 0.2);
}

function estimateMediaAgeMs(sentAtMs) {
  if (!Number.isFinite(sentAtMs) || sentAtMs <= 0) return 0;
  if (!Number.isFinite(state.serverClockOffsetMs) || state.serverClockOffsetMs === 0) {
    return 0;
  }
  return Math.max(0, Date.now() - state.serverClockOffsetMs - sentAtMs);
}

function formatDimensions(width, height) {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return "--";
  }
  return `${Math.round(width)} x ${Math.round(height)}`;
}

function clampZoomPercent(value) {
  if (!Number.isFinite(value)) return 100;
  return Math.min(VIEW_ZOOM_MAX_PERCENT, Math.max(VIEW_ZOOM_MIN_PERCENT, value));
}

function syncZoomButtons() {
  zoomOutButton.disabled = state.viewZoomPercent <= VIEW_ZOOM_MIN_PERCENT;
  zoomInButton.disabled = state.viewZoomPercent >= VIEW_ZOOM_MAX_PERCENT;
}

function currentVideoRenderWidth() {
  return state.videoFrameWidth ?? canvas.width;
}

function currentVideoRenderHeight() {
  return state.videoFrameHeight ?? canvas.height;
}

function renderViewMetrics() {
  const rect = canvas.getBoundingClientRect();
  const renderWidth = currentVideoRenderWidth();
  const renderHeight = currentVideoRenderHeight();
  viewCanvasRenderSize.textContent = formatDimensions(renderWidth, renderHeight);
  viewCanvasCssSize.textContent = formatDimensions(rect.width, rect.height);
  viewStreamSize.textContent = formatDimensions(state.videoFrameWidth, state.videoFrameHeight);
  viewViewportSize.textContent = formatDimensions(viewportCard.clientWidth, viewportCard.clientHeight);
  viewWindowSize.textContent = formatDimensions(window.innerWidth, window.innerHeight);
  viewRemoteScreenSize.textContent = formatDimensions(state.remoteScreenWidth, state.remoteScreenHeight);
  viewZoomValue.textContent = state.viewCssWidthMode === "custom"
    ? "Custom"
    : state.viewCssWidthMode === "window"
    ? "Window"
    : state.viewCssWidthMode === "viewport"
    ? "Viewport"
    : `${Math.round(state.viewZoomPercent)}%`;
  syncZoomButtons();
}

function applyCanvasZoom() {
  const surfaceWidth = currentVideoRenderWidth();
  const surfaceHeight = currentVideoRenderHeight();
  if (!Number.isFinite(surfaceWidth) || !Number.isFinite(surfaceHeight) || surfaceWidth <= 0 || surfaceHeight <= 0) {
    renderViewMetrics();
    return;
  }

  const setCanvasCssWidth = (width) => {
    const displayWidth = Math.max(1, Math.round(width));
    const displayHeight = Math.max(1, Math.round(displayWidth * (surfaceHeight / surfaceWidth)));
    canvas.style.width = `${displayWidth}px`;
    canvas.style.height = `${displayHeight}px`;
  };

  if (state.viewCssWidthMode === "viewport") {
    setCanvasCssWidth(viewportCard.clientWidth);
    renderViewMetrics();
    return;
  }

  if (state.viewCssWidthMode === "window") {
    setCanvasCssWidth(window.innerWidth);
    renderViewMetrics();
    return;
  }

  if (state.viewCssWidthMode === "custom") {
    const customWidth = Number(state.viewCustomCssWidth);
    if (Number.isFinite(customWidth) && customWidth > 0) {
      setCanvasCssWidth(customWidth);
      renderViewMetrics();
      return;
    }
  }

  setCanvasCssWidth(surfaceWidth * (state.viewZoomPercent / 100));
  renderViewMetrics();
}

function videoSurfaceWidth() {
  return state.remoteScreenWidth ?? currentVideoRenderWidth();
}

function videoSurfaceHeight() {
  return state.remoteScreenHeight ?? currentVideoRenderHeight();
}

function adjustZoom(deltaPercent) {
  const nextZoom = clampZoomPercent(state.viewZoomPercent + deltaPercent);
  if (nextZoom === state.viewZoomPercent) {
    syncZoomButtons();
    return;
  }
  state.viewZoomPercent = nextZoom;
  state.viewCssWidthMode = "zoom";
  state.viewCustomCssWidth = null;
  applyCanvasZoom();
  saveSettings();
}

function setCanvasDisplayRenderWidth() {
  state.viewCssWidthMode = "render";
  state.viewZoomPercent = 100;
  state.viewCustomCssWidth = null;
  applyCanvasZoom();
  saveSettings();
}

function setCanvasDisplayViewportWidth() {
  setCanvasDisplayWidth(viewportCard.clientWidth, "viewport");
}

function setCanvasDisplayWidth(width, mode = "custom") {
  const nextWidth = Number(width);
  if (!Number.isFinite(nextWidth) || nextWidth <= 0) {
    return;
  }
  const renderWidth = currentVideoRenderWidth();
  if (Number.isFinite(renderWidth) && renderWidth > 0) {
    state.viewZoomPercent = clampZoomPercent((nextWidth / renderWidth) * 100);
  }
  state.viewCssWidthMode = mode;
  state.viewCustomCssWidth = Math.round(nextWidth);
  applyCanvasZoom();
  saveSettings();
}

function setCanvasDisplayCurrentCssSize() {
  const rect = canvas.getBoundingClientRect();
  setCanvasDisplayWidth(rect.width);
}

function setCanvasDisplayRemoteScreenSize() {
  setCanvasDisplayWidth(state.remoteScreenWidth);
}

function setCanvasDisplayStreamSize() {
  setCanvasDisplayWidth(state.videoFrameWidth);
}

function setCanvasDisplayWindowSize() {
  setCanvasDisplayWidth(window.innerWidth, "window");
}

function controlTabLabel(tabName) {
  const button = tabButtons.find((item) => item.dataset.tabTarget === tabName);
  return button?.getAttribute("aria-label") || button?.title || "Controls";
}

function syncControlQuickTab(tabName = state.lastControlTab) {
  if (!controlQuickTab) return;
  const tabButton = tabButtons.find((button) => button.dataset.tabTarget === tabName) || tabButtons[0];
  const menuButton = controlMenuTabButtons.find((button) => button.dataset.controlTab === tabName);
  const iconSource = tabButton?.querySelector("svg") || menuButton?.querySelector("svg");
  if (!tabButton || !iconSource) return;
  controlQuickTab.replaceChildren(iconSource.cloneNode(true));
  const label = controlTabLabel(tabButton.dataset.tabTarget || tabName);
  controlQuickTab.title = label;
  controlQuickTab.setAttribute("aria-label", `Open ${label}`);
}

function setActiveTab(tabName) {
  state.lastControlTab = tabName;
  for (const button of tabButtons) {
    const active = button.dataset.tabTarget === tabName;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  }
  for (const button of controlMenuTabButtons) {
    button.classList.toggle("is-active", button.dataset.controlTab === tabName);
  }
  for (const panel of tabPanels) {
    const active = panel.id === `tab-panel-${tabName}`;
    panel.classList.toggle("is-active", active);
    panel.hidden = !active;
  }
  syncControlQuickTab(tabName);
  renderViewMetrics();
}

function openControlCard(tabName = state.lastControlTab) {
  setActiveTab(tabName);
  controlPanel.classList.add("is-card-open");
  controlPanel.open = true;
  transferPanel.open = false;
  audioPanel.open = false;
  errorPanel.open = false;
  closeMicDeviceMenu();
  releaseInput();
}

function isFullscreen() {
  return Boolean(document.fullscreenElement);
}

function syncFullscreenToggle() {
  if (!fullscreenToggle) return;
  const active = isFullscreen();
  fullscreenToggle.classList.toggle("is-active", active);
  fullscreenToggle.setAttribute("aria-pressed", active ? "true" : "false");
  fullscreenToggle.setAttribute("aria-label", active ? "Exit fullscreen" : "Enter fullscreen");
  const label = fullscreenToggle.querySelector("span");
  if (label) {
    label.textContent = active ? "Exit Fullscreen" : "Fullscreen";
  }
}

async function toggleFullscreen() {
  controlPanel.open = false;
  try {
    if (isFullscreen()) {
      await document.exitFullscreen();
      return;
    }
    if (!document.documentElement.requestFullscreen) {
      showToast("fullscreen_unavailable", "Fullscreen is not available in this browser");
      return;
    }
    await document.documentElement.requestFullscreen({ navigationUI: "hide" });
  } catch (error) {
    showToast("fullscreen_failed", error.message || String(error));
  } finally {
    syncFullscreenToggle();
  }
}

function showToast(code, message) {
  if (isErrorNotice(code)) {
    pushError(code, message);
    return;
  }
  pushInfo(code, message);
  toast.textContent = `${code}: ${message}`;
  toast.dataset.copy = `${code}: ${message}`;
  toast.classList.remove("hidden");
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(() => toast.classList.add("hidden"), 10000);
}

function isErrorNotice(code = "") {
  return /(?:error|fail|failed|unsupported|unavailable|missing|insecure|denied)/i.test(code);
}

function pushError(code, message) {
  pushMessage(code || "error", message || "Unknown error", { level: "error" });
}

function pushInfo(code, message, options = {}) {
  pushMessage(code || "info", message || "", { level: "info", ...options });
}

function pushDebug(code, message, options = {}) {
  pushMessage(code || "debug", message || "", { level: "debug", ...options });
}

function pushMessage(code, message, { level = "info", throttleMs = 0 } = {}) {
  const now = Date.now();
  const entryCode = String(code || level);
  const entryMessage = String(message || "");
  if (throttleMs > 0) {
    const existing = state.messages.find((entry) => (
      entry.code === entryCode
      && entry.level === level
      && now - entry.at < throttleMs
    ));
    if (existing) {
      existing.message = entryMessage;
      existing.at = now;
      renderErrors();
      return;
    }
  }
  state.messages.unshift({
    code: entryCode,
    message: entryMessage,
    level,
    at: now,
  });
  if (state.messages.length > MESSAGE_LIMIT) {
    state.messages.length = MESSAGE_LIMIT;
  }
  renderErrors();
}

function clearErrors() {
  const activeLevel = state.activeMessageLevel || "error";
  state.messages = state.messages.filter((entry) => messageDisplayLevel(entry) !== activeLevel);
  errorPanel.open = state.messages.length > 0;
  renderErrors();
}

function messageDisplayLevel(entry) {
  return entry?.level === "error" ? "error" : "info";
}

function setActiveMessageLevel(level = "error") {
  state.activeMessageLevel = level === "info" ? "info" : "error";
  renderErrors();
}

function renderErrors() {
  const errorMessages = state.messages.filter((entry) => messageDisplayLevel(entry) === "error");
  const infoMessages = state.messages.filter((entry) => messageDisplayLevel(entry) === "info");
  const activeMessages = state.activeMessageLevel === "info" ? infoMessages : errorMessages;
  const hasErrors = errorMessages.length > 0;
  errorPanel.classList.toggle("hidden", state.messages.length === 0);
  errorCount.textContent = String(Math.min(state.messages.length, 99));
  errorCount.classList.toggle("has-error", hasErrors);
  if (messageErrorCount) {
    messageErrorCount.textContent = String(Math.min(errorMessages.length, 99));
  }
  if (messageInfoCount) {
    messageInfoCount.textContent = String(Math.min(infoMessages.length, 99));
  }
  for (const button of messageTabButtons) {
    const active = button.dataset.messageLevel === state.activeMessageLevel;
    button.classList.toggle("is-active", active);
    button.classList.toggle("secondary", !active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  }
  errorList.replaceChildren();
  const fragment = document.createDocumentFragment();
  if (activeMessages.length === 0) {
    const empty = document.createElement("div");
    empty.className = "error-item";
    empty.textContent = state.activeMessageLevel === "info" ? "No info messages" : "No error messages";
    fragment.append(empty);
  }
  for (const error of activeMessages) {
    const item = document.createElement("article");
    item.className = `error-item is-${error.level || "info"}`;

    const code = document.createElement("span");
    code.className = "error-item-code";
    code.textContent = error.code;

    const message = document.createElement("div");
    message.className = "error-item-message";
    message.textContent = error.message;

    const time = document.createElement("span");
    time.className = "error-item-time";
    time.textContent = new Date(error.at).toLocaleTimeString();

    item.append(code, message, time);
    fragment.append(item);
  }
  errorList.append(fragment);
}

function markStaleDrop(message) {
  state.lastStaleDropAt = performance.now();
  pushDebug(message.startsWith("Audio") ? "audio_buffer" : "video_decode", message, { throttleMs: 2000 });
}

function clearReconnectTimer() {
  if (!state.reconnectTimer) return;
  clearTimeout(state.reconnectTimer);
  state.reconnectTimer = null;
}

function forceReconnect(reason) {
  if (state.reconnectingForLatency || state.connecting) return;
  state.reconnectingForLatency = true;
  state.highLatencySinceAt = 0;
  clearReconnectTimer();
  setStreamWarning(reason);
  closeConnection({ manual: false, preserveStatus: true });
  setStatus("Reconnecting...");
  setTimeout(() => {
    if (!state.reconnectingForLatency || state.connecting) return;
    void connect();
  }, 150);
}

function clearSettingsReconnectTimer() {
  if (!state.settingsReconnectTimer) return;
  clearTimeout(state.settingsReconnectTimer);
  state.settingsReconnectTimer = null;
}

function streamReconnectSettingsKey(settings = readSettingsFromControls()) {
  return JSON.stringify({
    codec: settings.codec,
    encodePreference: settings.encodePreference,
    bitrate: settings.bitrate,
    audioBitrateKbps: settings.audioBitrateKbps,
    fps: settings.fps,
    encoderLatency: settings.encoderLatency,
    encoderQuality: settings.encoderQuality,
    videoScale: settings.videoScale,
    gopMs: settings.gopMs,
    bufferMs: settings.bufferMs,
  });
}

function markAppliedStreamSettings(settings) {
  state.appliedStreamSettingsKey = streamReconnectSettingsKey(settings);
  if (state.pendingStreamSettingsKey === state.appliedStreamSettingsKey) {
    state.pendingStreamSettingsKey = "";
  }
}

function syncPendingStreamSettings(settings = readSettingsFromControls()) {
  const nextKey = streamReconnectSettingsKey(settings);
  if (state.appliedStreamSettingsKey && nextKey === state.appliedStreamSettingsKey) {
    state.pendingStreamSettingsKey = "";
    return;
  }
  state.pendingStreamSettingsKey = nextKey;
  state.lastLocalStreamSettingsAt = performance.now();
}

function syncServerStreamSettings(streamConfig, audioConfig, { force = false } = {}) {
  const incoming = normalizeSettings({
    codec: streamConfig?.codec,
    encodePreference: streamConfig?.encode_preference,
    bitrate: streamConfig?.bitrate_kbps,
    audioBitrateKbps: audioConfig?.bitrate_kbps,
    fps: streamConfig?.fps,
    encoderLatency: streamConfig?.performance?.encoder_latency,
    encoderQuality: streamConfig?.performance?.encoder_quality,
    videoScale: streamConfig?.performance?.scale,
    gopMs: streamConfig?.performance?.gop_ms,
    bufferMs: streamConfig?.performance?.buffer_ms,
  });
  const current = readSettingsFromControls();
  const currentKey = streamReconnectSettingsKey(current);
  const incomingKey = streamReconnectSettingsKey(incoming);
  if (incomingKey === currentKey) {
    markAppliedStreamSettings(incoming);
    if (force) {
      clearSettingsReconnectTimer();
      state.pendingStreamSettingsKey = "";
      state.lastLocalStreamSettingsAt = 0;
    }
    return;
  }
  if (!force) {
    const recentlyChangedLocally = state.lastLocalStreamSettingsAt > 0
      && performance.now() - state.lastLocalStreamSettingsAt < SETTINGS_RECONNECT_DELAY_MS;
    if (recentlyChangedLocally) {
      return;
    }
    if (!state.appliedStreamSettingsKey) {
      markAppliedStreamSettings(incoming);
      return;
    }
    if (state.pendingStreamSettingsKey && incomingKey !== state.pendingStreamSettingsKey) {
      return;
    }
  } else {
    clearSettingsReconnectTimer();
    state.pendingStreamSettingsKey = "";
    state.lastLocalStreamSettingsAt = 0;
  }
  const next = {
    ...current,
    codec: incoming.codec,
    encodePreference: incoming.encodePreference,
    bitrate: incoming.bitrate,
    audioBitrateKbps: incoming.audioBitrateKbps,
    fps: incoming.fps,
    encoderLatency: incoming.encoderLatency,
    encoderQuality: incoming.encoderQuality,
    videoScale: incoming.videoScale,
    gopMs: incoming.gopMs,
    bufferMs: incoming.bufferMs,
  };
  applySettings(next);
  const applied = readSettingsFromControls();
  saveSettings(applied);
  markAppliedStreamSettings(applied);
}

function reconnectWithCurrentStreamSettings(reason) {
  if (state.connecting) return;
  if (!isConnected()) return;
  clearSettingsReconnectTimer();
  state.pendingStreamSettingsKey = streamReconnectSettingsKey();
  closeConnection({ manual: false, preserveStatus: true, keepCameraEnabled: true });
  setStatus(reason);
  setTimeout(() => {
    if (state.connecting || isConnected()) return;
    void connect();
  }, 150);
}

function maybeScheduleSettingsReconnect(settings = readSettingsFromControls()) {
  const socketOpen = isConnected();
  if (!socketOpen || state.connecting || state.reconnectingForLatency) {
    clearSettingsReconnectTimer();
    return;
  }
  const nextKey = streamReconnectSettingsKey(settings);
  if (!state.appliedStreamSettingsKey || nextKey === state.appliedStreamSettingsKey) {
    if (nextKey === state.appliedStreamSettingsKey) {
      state.pendingStreamSettingsKey = "";
    }
    clearSettingsReconnectTimer();
    return;
  }
  clearSettingsReconnectTimer();
  state.settingsReconnectTimer = setTimeout(() => {
    state.settingsReconnectTimer = null;
    if (!isConnected() || state.connecting) return;
    const latestSettings = readSettingsFromControls();
    if (streamReconnectSettingsKey(latestSettings) === state.appliedStreamSettingsKey) return;
    reconnectWithCurrentStreamSettings("Reconnecting with stream settings...");
  }, SETTINGS_RECONNECT_DELAY_MS);
}

function monitorConnectionHealth() {
  const now = performance.now();
  const socketOpen = isConnected();

  let warning = "";
  if (socketOpen && state.audioUnderrunActive && !state.audioMuted) {
    warning = state.streamWarning || AUDIO_UNDERRUN_WARNING;
  } else if (socketOpen && state.lastVideoPacketAt && now - state.lastVideoPacketAt > state.mediaStallResetMs) {
    warning = "Video stalled";
  } else if (socketOpen && state.lastVideoFrameRenderedAt && now - state.lastVideoFrameRenderedAt > state.mediaStallResetMs) {
    warning = "Video render stalled";
  }

  if (warning) {
    setStreamWarning(warning);
  } else if (state.streamWarning) {
    setStreamWarning("");
  }

  if (Number.isFinite(state.wsLatencyMs) && state.wsLatencyMs > HIGH_LATENCY_RECONNECT_MS) {
    if (!state.highLatencySinceAt) {
      state.highLatencySinceAt = now;
    } else if (now - state.highLatencySinceAt >= HIGH_LATENCY_RECONNECT_GRACE_MS) {
      forceReconnect("Latency stayed above 1500 ms, reconnecting");
    }
  } else {
    state.highLatencySinceAt = 0;
    if (!warning && state.reconnectingForLatency) {
      state.reconnectingForLatency = false;
    }
  }
}

toast.addEventListener("click", async () => {
  if (!canUseBrowserClipboard()) {
    setStatus("Focus window before copying");
    return;
  }
  try {
    await navigator.clipboard.writeText(toast.dataset.copy || toast.textContent || "");
    setStatus("Error copied");
  } catch {
    setStatus("Copy failed");
  }
});

function apiUrl(path) {
  return new URL(path, state.apiOrigin || window.location.origin);
}

function webSocketUrl(path) {
  const url = apiUrl(path);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url;
}

function audioOutputModeLabel(useRealOutput) {
  return useRealOutput ? "Real audio devices" : "Virtual output";
}

function renderAudioOutputStatus(text = audioOutputModeLabel(audioRealOutputInput.checked), warning = false) {
  if (!audioOutputStatus) return;
  audioOutputStatus.textContent = text;
  audioOutputStatus.classList.toggle("is-warning", warning);
}

async function refreshAudioOutputStatus({ silent = false } = {}) {
  if (!state.authenticated && !state.sessionPasswd) {
    renderAudioOutputStatus();
    return null;
  }
  try {
    const response = await fetch(appendAuthQuery(apiUrl("/api/audio/output")), {
      cache: "no-store",
      credentials: "include",
    });
    if (!response.ok) throw new Error(await response.text());
    const body = await response.json();
    const useRealOutput = body?.mode === "real";
    state.audioUseRealOutput = useRealOutput;
    audioRealOutputInput.checked = useRealOutput;
    renderAudioOutputStatus(audioOutputModeLabel(useRealOutput));
    return body;
  } catch (error) {
    if (!silent) {
      showToast("audio_output_status_failed", error.message || String(error));
    }
    renderAudioOutputStatus("Audio output status unavailable", true);
    return null;
  }
}

async function setAudioOutputMode(useRealOutput, { silent = false } = {}) {
  if (!state.authenticated && !state.sessionPasswd) {
    renderAudioOutputStatus(audioOutputModeLabel(useRealOutput));
    return;
  }
  renderAudioOutputStatus("Switching audio output...");
  try {
    const response = await fetch(appendAuthQuery(apiUrl("/api/audio/output")), {
      method: "POST",
      credentials: "include",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ use_real_device: useRealOutput }),
    });
    if (!response.ok) throw new Error(await response.text());
    const body = await response.json();
    const appliedUseRealOutput = body?.mode === "real";
    state.audioUseRealOutput = appliedUseRealOutput;
    audioRealOutputInput.checked = appliedUseRealOutput;
    renderAudioOutputStatus(audioOutputModeLabel(appliedUseRealOutput));
  } catch (error) {
    renderAudioOutputStatus("Audio output switch failed", true);
    await refreshAudioOutputStatus({ silent: true });
    if (!silent) {
      showToast("audio_output_switch_failed", error.message || String(error));
    }
  }
}

async function probeAuth() {
  try {
    const response = await fetch(apiUrl("/api/auth"), {
      cache: "no-store",
      credentials: "include",
    });
    return response.ok;
  } catch {
    return false;
  }
}

function clampControlValue(control, value, fallback) {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return fallback;
  const min = Number(control.min);
  const max = Number(control.max);
  return Math.min(max, Math.max(min, numeric));
}

function normalizeApiOrigin(value) {
  const fallback = window.location.origin;
  if (typeof value !== "string") return fallback;
  const trimmed = value.trim();
  if (!trimmed) return fallback;
  try {
    return new URL(trimmed, fallback).origin;
  } catch {
    return fallback;
  }
}

function loadStoredApiOrigin() {
  try {
    return localStorage.getItem(API_ORIGIN_STORAGE_KEY) || window.location.origin;
  } catch {
    return window.location.origin;
  }
}

function saveApiOrigin(origin) {
  try {
    localStorage.setItem(API_ORIGIN_STORAGE_KEY, normalizeApiOrigin(origin));
  } catch {
    // Ignore storage failures; the session still works for this visit.
  }
}

function loadStoredPasswd() {
  try {
    return localStorage.getItem(PASSWD_STORAGE_KEY) || "";
  } catch {
    return "";
  }
}

function savePasswd(passwd) {
  try {
    localStorage.setItem(PASSWD_STORAGE_KEY, passwd);
  } catch {
    // Ignore storage failures; the session still works for this visit.
  }
}

function normalizeSettings(settings = {}) {
  const allowedCodecs = new Set(Array.from(codecSelect.options, (option) => option.value));
  const allowedAudioBitrates = new Set(Array.from(audioBitrateSelect.options, (option) => Number(option.value)));
  const allowedMicBitrates = new Set(Array.from(micBitrateSelect.options, (option) => Number(option.value)));
  const allowedTouchModes = new Set(Array.from(touchModeSelect.options, (option) => option.value));
  const allowedEncoderLatency = new Set(Array.from(encoderLatencySelect.options, (option) => option.value));
  const allowedEncoderQuality = new Set(Array.from(encoderQualitySelect.options, (option) => option.value));
  const allowedVideoScales = new Set(Array.from(videoScaleSelect.options, (option) => option.value));
  const defaultCodec = codecSelect.options[0]?.value || "h264";
  const defaultBitrate = Number(bitrateInput.value);
  const defaultAudioBitrate = Number(audioBitrateSelect.value);
  const defaultMicBitrate = Number(micBitrateSelect.value);
  const defaultFps = Number(fpsInput.value);
  const defaultScrollSpeed = Number(scrollSpeedInput.value);
  const defaultAudioVolumePercent = Number(audioVolumeInput.value);
  const defaultAudioClockRate = Number(audioClockRateInput.value);
  const defaultAutoDisconnectMinutes = Number(autoDisconnectMinutesInput.value);
  const codec = allowedCodecs.has(settings.codec) ? settings.codec : defaultCodec;
  return {
    codec,
    encodePreference: normalizeEncodePreferenceForCodec(settings.encodePreference, codec),
    bitrate: clampControlValue(bitrateInput, settings.bitrate, defaultBitrate),
    audioBitrateKbps: allowedAudioBitrates.has(Number(settings.audioBitrateKbps))
      ? Number(settings.audioBitrateKbps)
      : defaultAudioBitrate,
    micBitrateKbps: allowedMicBitrates.has(Number(settings.micBitrateKbps))
      ? Number(settings.micBitrateKbps)
      : defaultMicBitrate,
    fps: clampControlValue(fpsInput, settings.fps, defaultFps),
    scrollSpeed: clampControlValue(scrollSpeedInput, settings.scrollSpeed, defaultScrollSpeed),
    touchMode: allowedTouchModes.has(settings.touchMode) ? settings.touchMode : touchModeSelect.value,
    directTouchScroll: settings.directTouchScroll === true,
    micEnabled: settings.micEnabled === true,
    micDeviceId: typeof settings.micDeviceId === "string" ? settings.micDeviceId : "",
    audioMuted: settings.audioMuted === true,
    audioUseRealOutput: settings.audioUseRealOutput === true,
    audioVolumePercent: clampControlValue(audioVolumeInput, settings.audioVolumePercent, defaultAudioVolumePercent),
    audioLatencyMs: clampControlValue(audioLatencyInput, settings.audioLatencyMs, Number(audioLatencyInput.value)),
    audioClockRate: clampControlValue(audioClockRateInput, settings.audioClockRate, defaultAudioClockRate),
    audioClockAuto: settings.audioClockAuto !== false,
    encoderLatency: allowedEncoderLatency.has(settings.encoderLatency)
      ? settings.encoderLatency
      : encoderLatencySelect.value,
    encoderQuality: allowedEncoderQuality.has(settings.encoderQuality)
      ? settings.encoderQuality
      : encoderQualitySelect.value,
    videoScale: allowedVideoScales.has(settings.videoScale)
      ? settings.videoScale
      : videoScaleSelect.value,
    gopMs: clampControlValue(gopMsInput, settings.gopMs, Number(gopMsInput.value)),
    bufferMs: clampControlValue(bufferMsInput, settings.bufferMs, Number(bufferMsInput.value)),
    staleDropMs: clampControlValue(
      staleDropMsInput,
      settings.staleDropMs,
      DEFAULT_LIVE_MEDIA_MAX_AGE_MS,
    ),
    stallResetMs: clampControlValue(
      stallResetMsInput,
      settings.stallResetMs,
      DEFAULT_MEDIA_STALL_RESET_MS,
    ),
    maxDecodeQueue: clampControlValue(
      decodeQueueInput,
      settings.maxDecodeQueue,
      DEFAULT_MAX_VIDEO_DECODE_QUEUE,
    ),
    autoDisconnectMinutes: clampControlValue(
      autoDisconnectMinutesInput,
      settings.autoDisconnectMinutes,
      defaultAutoDisconnectMinutes,
    ),
    viewZoomPercent: clampZoomPercent(settings.viewZoomPercent),
  };
}

function normalizeCodecOptions(options = DEFAULT_CODEC_OPTIONS) {
  const normalized = [];
  const seen = new Set();
  for (const option of options) {
    if (typeof option?.value !== "string" || option.value.length === 0 || seen.has(option.value)) continue;
    seen.add(option.value);
    normalized.push({
      value: option.value,
      label: typeof option?.label === "string" && option.label.length > 0 ? option.label : option.value.toUpperCase(),
    });
  }
  return normalized.length > 0 ? normalized : DEFAULT_CODEC_OPTIONS;
}

function setCodecOptions(options = DEFAULT_CODEC_OPTIONS, preferredValue = codecSelect.value) {
  const nextOptions = normalizeCodecOptions(options);
  codecSelect.replaceChildren();
  for (const option of nextOptions) {
    const element = document.createElement("option");
    element.value = option.value;
    element.textContent = option.label;
    codecSelect.appendChild(element);
  }
  const allowedValues = new Set(Array.from(codecSelect.options, (option) => option.value));
  codecSelect.value = allowedValues.has(preferredValue)
    ? preferredValue
    : (codecSelect.options[0]?.value || "h264");
  renderCodecOptions();
}

function renderRadioGroupFromSelect(group, selectEl, name) {
  if (!group || !selectEl) return;
  const fragment = document.createDocumentFragment();
  Array.from(selectEl.options).forEach((option, index) => {
    if (typeof option.value !== "string" || option.value.length === 0) return;
    const chip = document.createElement("label");
    chip.className = "radio-chip";

    const input = document.createElement("input");
    input.type = "radio";
    input.name = name;
    input.id = `${selectEl.id}-radio-${index}`;
    input.value = option.value;
    input.checked = option.value === selectEl.value;
    input.addEventListener("change", () => {
      if (!input.checked || selectEl.value === input.value) return;
      selectEl.value = input.value;
      selectEl.dispatchEvent(new Event("change", { bubbles: true }));
    });

    const text = document.createElement("span");
    text.className = "radio-chip-label";
    text.textContent = option.textContent?.trim() || option.value;

    chip.append(input, text);
    fragment.appendChild(chip);
  });
  group.replaceChildren(fragment);
}

function renderCodecOptions() {
  renderRadioGroupFromSelect(codecGroup, codecSelect, "codec-choice");
}

function renderEncodePreferenceRadioGroup() {
  renderRadioGroupFromSelect(
    encodePreferenceGroup,
    encodePreferenceSelect,
    "encode-preference-choice",
  );
}

function renderTouchModeCheckboxGroup() {
  if (!touchModeGroup || !touchModeSelect) return;
  const fragment = document.createDocumentFragment();
  Array.from(touchModeSelect.options).forEach((option, index) => {
    if (typeof option.value !== "string" || option.value.length === 0) return;
    const chip = document.createElement("label");
    chip.className = "checkbox-chip";

    const input = document.createElement("input");
    input.type = "checkbox";
    input.name = "touch-mode-choice";
    input.id = `${touchModeSelect.id}-checkbox-${index}`;
    input.value = option.value;
    input.checked = option.value === touchModeSelect.value;
    input.addEventListener("change", () => {
      if (!input.checked) {
        input.checked = true;
        return;
      }
      if (touchModeSelect.value === input.value) return;
      touchModeSelect.value = input.value;
      touchModeSelect.dispatchEvent(new Event("change", { bubbles: true }));
    });

    const text = document.createElement("span");
    text.className = "checkbox-chip-label";
    text.textContent = option.textContent?.trim() || option.value;

    chip.append(input, text);
    fragment.appendChild(chip);
  });
  touchModeGroup.replaceChildren(fragment);
}

function readSettingsFromControls() {
  return normalizeSettings({
    codec: codecSelect.value,
    encodePreference: encodePreferenceSelect.value,
    bitrate: bitrateInput.value,
    audioBitrateKbps: audioBitrateSelect.value,
    micBitrateKbps: micBitrateSelect.value,
    fps: fpsInput.value,
    scrollSpeed: scrollSpeedInput.value,
    audioLatencyMs: audioLatencyInput.value,
    audioVolumePercent: audioVolumeInput.value,
    audioClockRate: audioClockRateInput.value,
    audioClockAuto: audioClockAutoInput.checked,
    audioUseRealOutput: audioRealOutputInput.checked,
    encoderLatency: encoderLatencySelect.value,
    encoderQuality: encoderQualitySelect.value,
    videoScale: videoScaleSelect.value,
    gopMs: gopMsInput.value,
    bufferMs: bufferMsInput.value,
    staleDropMs: staleDropMsInput.value,
    stallResetMs: stallResetMsInput.value,
    maxDecodeQueue: decodeQueueInput.value,
    autoDisconnectMinutes: autoDisconnectMinutesInput.value,
    touchMode: touchModeSelect.value,
    directTouchScroll: directTouchScrollInput.checked,
    micEnabled: state.micEnabled,
    micDeviceId: state.micDeviceId,
    audioMuted: audioMuteInput?.checked ?? state.audioMuted,
    viewZoomPercent: state.viewZoomPercent,
  });
}

function saveSettings(settings = readSettingsFromControls()) {
  try {
    const stored = { ...settings };
    delete stored.audioLatencyMs;
    delete stored.audioClockRate;
    localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(stored));
  } catch {
    // Ignore storage failures; the session still works for this visit.
  }
}

function loadStoredSettings() {
  try {
    const raw = localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return readSettingsFromControls();
    const stored = JSON.parse(raw);
    delete stored.audioLatencyMs;
    delete stored.audioClockRate;
    return {
      ...normalizeSettings(stored),
      audioMuted: false,
    };
  } catch {
    return readSettingsFromControls();
  }
}

function renderSettingsValues(settings = readSettingsFromControls()) {
  bitrateValue.textContent = `${settings.bitrate} kbps`;
  fpsValue.textContent = `${settings.fps} fps`;
  scrollSpeedValue.textContent = `${settings.scrollSpeed} / 10`;
  audioLatencyValue.textContent = `${settings.audioLatencyMs} ms`;
  audioVolumeValue.textContent = `${settings.audioVolumePercent}%`;
  audioClockRateValue.textContent = `${Number(settings.audioClockRate).toFixed(4)}x`;
  gopMsValue.textContent = `${settings.gopMs} ms`;
  bufferMsValue.textContent = `${settings.bufferMs} ms`;
  staleDropMsValue.textContent = `${settings.staleDropMs} ms`;
  stallResetMsValue.textContent = `${settings.stallResetMs} ms`;
  decodeQueueValue.textContent = `${settings.maxDecodeQueue} frame${settings.maxDecodeQueue === 1 ? "" : "s"}`;
  syncPerformancePresetSelect(settings);
}

function syncPerformancePresetSelect(settings = readSettingsFromControls()) {
  const activePreset = Object.entries(PERFORMANCE_PRESETS)
    .find(([, preset]) => Object.entries(preset)
      .every(([key, value]) => settings[key] === value))?.[0];
  if (performancePresetSelect) {
    performancePresetSelect.value = activePreset || "custom";
  }
}

function syncTouchModeControls(settings = readSettingsFromControls()) {
  const isDirectTouch = settings.touchMode === "direct_touch";
  renderTouchModeCheckboxGroup();
  directTouchScrollInput.disabled = !isDirectTouch;
  directTouchScrollLabel.classList.toggle("is-disabled", !isDirectTouch);
}

function renderMicToggle() {
  micToggle.classList.toggle("is-active", state.micEnabled);
  micToggle.classList.toggle("is-pending", state.micStarting);
  micToggle.setAttribute("aria-pressed", state.micEnabled ? "true" : "false");
  micToggle.setAttribute("aria-expanded", state.micDeviceMenuOpen ? "true" : "false");
  micToggle.setAttribute(
    "aria-label",
    state.micEnabled ? `Microphone: ${currentMicDeviceLabel()}` : "Choose microphone",
  );
}

function renderAudioToggle() {
  const active = !state.audioMuted && state.audioUserActivated && !state.audioPlaybackBlocked;
  audioToggle.classList.toggle("is-active", active);
  audioToggle.setAttribute("aria-pressed", active ? "true" : "false");
  audioToggle.setAttribute("aria-expanded", audioPanel.open ? "true" : "false");
  audioToggle.setAttribute("aria-label", active ? "Audio settings" : "Audio settings, muted");
  if (audioMuteInput) {
    audioMuteInput.checked = state.audioMuted;
  }
}

function micDeviceLabel(device, index = 0) {
  const label = device?.label?.trim();
  if (label) return label;
  if (device?.deviceId === "default") return "Default microphone";
  return `Microphone ${index + 1}`;
}

function currentMicDeviceLabel() {
  const selectedIndex = state.micDevices.findIndex((device) => (
    (device.deviceId || "") === (state.micDeviceId || "")
  ));
  if (selectedIndex >= 0) {
    return micDeviceLabel(state.micDevices[selectedIndex], selectedIndex);
  }
  return state.micDeviceId ? "Selected microphone" : "Default microphone";
}

function createMicMenuItem({
  name,
  meta = "",
  active = false,
  danger = false,
  disabled = false,
  onClick = null,
}) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "device-menu-item";
  button.setAttribute("role", danger || !onClick ? "menuitem" : "menuitemradio");
  if (!danger && onClick) {
    button.setAttribute("aria-checked", active ? "true" : "false");
  }
  if (active) {
    button.classList.add("is-active");
  }
  if (danger) {
    button.classList.add("is-danger");
  }
  button.disabled = disabled;

  const nameEl = document.createElement("span");
  nameEl.className = "device-menu-name";
  nameEl.textContent = name;
  button.append(nameEl);

  const metaEl = document.createElement("span");
  metaEl.className = "device-menu-meta";
  metaEl.textContent = meta;
  button.append(metaEl);

  if (onClick) {
    button.addEventListener("click", onClick);
  }
  return button;
}

function renderMicDeviceMenu({ loading = false } = {}) {
  const fragment = document.createDocumentFragment();
  if (loading) {
    fragment.append(createMicMenuItem({
      name: "Loading microphones...",
      disabled: true,
    }));
  } else if (!navigator.mediaDevices?.getUserMedia) {
    fragment.append(createMicMenuItem({
      name: "Microphone unavailable",
      disabled: true,
    }));
  } else if (state.micDevices.length > 0) {
    state.micDevices.forEach((device, index) => {
      const deviceId = device.deviceId || "";
      const active = state.micEnabled && (state.micDeviceId || "") === deviceId;
      fragment.append(createMicMenuItem({
        name: micDeviceLabel(device, index),
        meta: active ? "Selected" : "",
        active,
        onClick: () => {
          void selectMicrophoneDevice(deviceId);
        },
      }));
    });
  } else {
    fragment.append(createMicMenuItem({
      name: "No microphones found",
      disabled: true,
    }));
  }

  fragment.append(createMicMenuItem({
    name: "Disable microphone",
    meta: state.micEnabled ? "On" : "Off",
    danger: true,
    onClick: disableMicrophoneFromMenu,
  }));
  micDeviceMenu.replaceChildren(fragment);
}

async function refreshMicDevices({ silent = false } = {}) {
  if (!navigator.mediaDevices?.enumerateDevices) {
    state.micDevices = [];
    renderMicDeviceMenu();
    renderMicToggle();
    return [];
  }
  try {
    const devices = await navigator.mediaDevices.enumerateDevices();
    state.micDevices = devices.filter((device) => device.kind === "audioinput");
  } catch (error) {
    state.micDevices = [];
    if (!silent) {
      showToast("mic_devices_failed", error.message || String(error));
    }
  }
  renderMicDeviceMenu();
  renderMicToggle();
  return state.micDevices;
}

async function openMicDeviceMenu() {
  controlPanel.open = false;
  transferPanel.open = false;
  audioPanel.open = false;
  errorPanel.open = false;
  releaseInput();
  state.micDeviceMenuOpen = true;
  micDeviceMenu.classList.remove("hidden");
  renderMicToggle();
  renderMicDeviceMenu({ loading: true });
  await refreshMicDevices({ silent: true });
}

function closeMicDeviceMenu() {
  if (!state.micDeviceMenuOpen) return;
  state.micDeviceMenuOpen = false;
  micDeviceMenu.classList.add("hidden");
  renderMicToggle();
}

function toggleMicDeviceMenu(event) {
  event.preventDefault();
  event.stopPropagation();
  if (state.micDeviceMenuOpen) {
    closeMicDeviceMenu();
    return;
  }
  void openMicDeviceMenu();
}

async function selectMicrophoneDevice(deviceId = "") {
  closeMicDeviceMenu();
  state.micDeviceId = deviceId || "";
  state.micEnabled = true;
  persistCurrentSettings();
  renderMicToggle();
  if (isConnected()) {
    stopMicrophoneCapture();
    await startMicrophoneCapture();
  }
}

function disableMicrophoneFromMenu() {
  closeMicDeviceMenu();
  state.micEnabled = false;
  persistCurrentSettings();
  stopMicrophoneCapture();
}

function renderCameraToggle() {
  cameraToggle.classList.toggle("is-active", state.cameraEnabled);
  cameraToggle.classList.toggle("is-pending", state.cameraStarting);
  cameraToggle.setAttribute("aria-pressed", state.cameraEnabled ? "true" : "false");
  cameraToggle.setAttribute("aria-label", state.cameraEnabled ? "Disable camera uplink" : "Enable camera uplink");
}

function applySettings(settings) {
  const normalized = normalizeSettings(settings);
  codecSelect.value = normalized.codec;
  renderCodecOptions();
  renderEncodePreferenceOptions(encodeOptionsForCodec(normalized.codec), normalized.encodePreference);
  bitrateInput.value = String(normalized.bitrate);
  audioBitrateSelect.value = String(normalized.audioBitrateKbps);
  micBitrateSelect.value = String(normalized.micBitrateKbps);
  fpsInput.value = String(normalized.fps);
  scrollSpeedInput.value = String(normalized.scrollSpeed);
  audioLatencyInput.value = String(normalized.audioLatencyMs);
  audioVolumeInput.value = String(normalized.audioVolumePercent);
  audioClockRateInput.value = String(normalized.audioClockRate);
  audioClockAutoInput.checked = normalized.audioClockAuto;
  audioRealOutputInput.checked = normalized.audioUseRealOutput;
  if (audioMuteInput) {
    audioMuteInput.checked = normalized.audioMuted;
  }
  encoderLatencySelect.value = normalized.encoderLatency;
  encoderQualitySelect.value = normalized.encoderQuality;
  videoScaleSelect.value = normalized.videoScale;
  gopMsInput.value = String(normalized.gopMs);
  bufferMsInput.value = String(normalized.bufferMs);
  staleDropMsInput.value = String(normalized.staleDropMs);
  stallResetMsInput.value = String(normalized.stallResetMs);
  decodeQueueInput.value = String(normalized.maxDecodeQueue);
  autoDisconnectMinutesInput.value = String(normalized.autoDisconnectMinutes);
  touchModeSelect.value = normalized.touchMode;
  directTouchScrollInput.checked = normalized.directTouchScroll;
  state.micEnabled = normalized.micEnabled;
  state.micDeviceId = normalized.micDeviceId;
  state.audioMuted = normalized.audioMuted;
  state.audioVolumePercent = normalized.audioVolumePercent;
  state.audioUseRealOutput = normalized.audioUseRealOutput;
  state.viewZoomPercent = normalized.viewZoomPercent;
  state.liveMediaMaxAgeMs = normalized.staleDropMs;
  state.mediaStallResetMs = normalized.stallResetMs;
  state.maxVideoDecodeQueue = normalized.maxDecodeQueue;
  renderSettingsValues(normalized);
  syncTouchModeControls(normalized);
  renderMicToggle();
  renderAudioToggle();
  renderAudioOutputStatus(audioOutputModeLabel(normalized.audioUseRealOutput));
  syncAudioGain();
  applyCanvasZoom();
}

function persistResolvedSettings(settings, { scheduleReconnect = true } = {}) {
  renderSettingsValues(settings);
  syncTouchModeControls(settings);
  state.liveMediaMaxAgeMs = settings.staleDropMs;
  state.mediaStallResetMs = settings.stallResetMs;
  state.maxVideoDecodeQueue = settings.maxDecodeQueue;
  state.audioVolumePercent = settings.audioVolumePercent;
  syncAudioGain();
  syncPendingStreamSettings(settings);
  saveSettings(settings);
  syncAutoDisconnectTimer(settings);
  if (scheduleReconnect) {
    maybeScheduleSettingsReconnect(settings);
  }
}

function persistCurrentSettings() {
  persistResolvedSettings(readSettingsFromControls());
}

function applyPerformancePreset(name) {
  const preset = PERFORMANCE_PRESETS[name];
  if (!preset) return;
  const current = readSettingsFromControls();
  const next = normalizeSettings({ ...current, ...preset });
  applySettings(next);
  persistResolvedSettings(readSettingsFromControls());
}

function clearAutoDisconnectTimer() {
  if (!state.autoDisconnectTimer) return;
  clearTimeout(state.autoDisconnectTimer);
  state.autoDisconnectTimer = null;
}

function syncAutoDisconnectTimer(settings = readSettingsFromControls()) {
  clearAutoDisconnectTimer();
  if (!isConnected()) return;
  if (settings.autoDisconnectMinutes <= AUTO_DISCONNECT_DISABLED_MINUTES) return;
  state.autoDisconnectTimer = setTimeout(() => {
    state.autoDisconnectTimer = null;
    showToast("auto_disconnect", `Disconnected after ${settings.autoDisconnectMinutes} minutes`);
    disconnect();
  }, settings.autoDisconnectMinutes * 60 * 1000);
}

function noteAutoDisconnectActivity(message) {
  if (!AUTO_DISCONNECT_ACTIVITY_MESSAGE_TYPES.has(message?.type)) return;
  const now = performance.now();
  if (now - state.lastAutoDisconnectActivityAt < AUTO_DISCONNECT_ACTIVITY_REFRESH_MS) return;
  state.lastAutoDisconnectActivityAt = now;
  syncAutoDisconnectTimer();
}

function defaultEncodePreferenceForCodec(codec) {
  if (codec === "h265") return "libx265";
  if (codec === "vp8") return "libvpx";
  if (codec === "vp9") return "libvpx-vp9";
  if (codec === "av1") return "libsvtav1";
  return "libx264";
}

function defaultEncodeOptionsForCodec(codec) {
  if (codec === "h265") {
    return [
      { value: "hevc_nvenc", label: "hevc_nvenc" },
      { value: "hevc_qsv", label: "hevc_qsv" },
      { value: "hevc_vaapi", label: "hevc_vaapi" },
      { value: "libx265", label: "libx265" },
    ];
  }
  if (codec === "vp8") {
    return [{ value: "libvpx", label: "libvpx" }];
  }
  if (codec === "vp9") {
    return [
      { value: "vp9_qsv", label: "vp9_qsv" },
      { value: "vp9_vaapi", label: "vp9_vaapi" },
      { value: "libvpx-vp9", label: "libvpx-vp9" },
    ];
  }
  if (codec === "av1") {
    return [
      { value: "av1_nvenc", label: "av1_nvenc" },
      { value: "av1_qsv", label: "av1_qsv" },
      { value: "av1_vaapi", label: "av1_vaapi" },
      { value: "libsvtav1", label: "libsvtav1" },
      { value: "libaom-av1", label: "libaom-av1" },
    ];
  }
  return [
    { value: "h264_nvenc", label: "h264_nvenc" },
    { value: "h264_qsv", label: "h264_qsv" },
    { value: "h264_vaapi", label: "h264_vaapi" },
    { value: "libx264", label: "libx264" },
  ];
}

function fallbackEncodePreferenceForCodec(codec, options = defaultEncodeOptionsForCodec(codec)) {
  const defaultPreference = defaultEncodePreferenceForCodec(codec);
  const allowedValues = new Set(
    Array.isArray(options)
      ? options
          .map((option) => option?.value)
          .filter((value) => typeof value === "string" && value.length > 0)
      : [],
  );
  if (allowedValues.has(defaultPreference)) {
    return defaultPreference;
  }
  return options?.[0]?.value || defaultPreference;
}

function encodeOptionsForCodec(codec) {
  const cached = state.encoderOptionsByCodec?.[codec];
  return Array.isArray(cached) && cached.length > 0
    ? cached
    : defaultEncodeOptionsForCodec(codec);
}

function normalizeEncodePreferenceForCodec(value, codec) {
  const preferred = KNOWN_ENCODE_PREFERENCE_VALUES.has(value)
    ? value
    : defaultEncodePreferenceForCodec(codec);
  const allowedValues = new Set(
    codec === "h264"
      ? ["h264_nvenc", "h264_qsv", "h264_vaapi", "libx264"]
      : codec === "h265"
        ? ["hevc_nvenc", "hevc_qsv", "hevc_vaapi", "libx265"]
        : codec === "vp8"
          ? ["libvpx"]
          : codec === "vp9"
            ? ["vp9_qsv", "vp9_vaapi", "libvpx-vp9"]
            : ["av1_nvenc", "av1_qsv", "av1_vaapi", "libsvtav1", "libaom-av1"],
  );
  if (allowedValues.has(preferred)) return preferred;
  return defaultEncodePreferenceForCodec(codec);
}

function renderEncodePreferenceOptions(
  options = defaultEncodeOptionsForCodec(codecSelect.value),
  preferredValue = encodePreferenceSelect.value,
) {
  const fallbackOptions = Array.isArray(options) && options.length > 0
    ? options
    : defaultEncodeOptionsForCodec(codecSelect.value);
  encodePreferenceSelect.replaceChildren();
  for (const option of fallbackOptions) {
    if (typeof option?.value !== "string" || typeof option?.label !== "string") continue;
    const element = document.createElement("option");
    element.value = option.value;
    element.textContent = option.label;
    encodePreferenceSelect.appendChild(element);
  }
  const allowedValues = new Set(Array.from(encodePreferenceSelect.options, (option) => option.value));
  const fallbackValue = fallbackEncodePreferenceForCodec(codecSelect.value, fallbackOptions);
  const nextValue = normalizeEncodePreferenceForCodec(preferredValue, codecSelect.value);
  encodePreferenceSelect.value = allowedValues.has(nextValue) ? nextValue : fallbackValue;
  renderEncodePreferenceRadioGroup();
  renderEncodePreferenceHelp(fallbackOptions, codecSelect.value);
}

function renderEncodePreferenceHelp(options = defaultEncodeOptionsForCodec(codecSelect.value), codec = codecSelect.value) {
  if (!encodePreferenceHelp) return;
  const exactEncoders = Array.isArray(options)
    ? options
        .map((option) => option?.ffmpeg_encoder)
        .filter((encoder) => typeof encoder === "string" && encoder.length > 0)
    : [];
  const nvidiaEncoders = exactEncoders.filter((encoder) => encoder.includes("nvenc"));
  if (nvidiaEncoders.length > 0) {
    encodePreferenceHelp.textContent = `Detected NVIDIA encoder${nvidiaEncoders.length > 1 ? "s" : ""}: ${nvidiaEncoders.join(", ")}.`;
    encodePreferenceHelp.classList.remove("is-warning");
    return;
  }
  if (exactEncoders.length > 0) {
    encodePreferenceHelp.textContent = `Detected ffmpeg encoders: ${exactEncoders.join(", ")}.`;
    encodePreferenceHelp.classList.remove("is-warning");
    return;
  }
  if (codec === "vp8") {
    encodePreferenceHelp.textContent = "VP8 uses libvpx on this host.";
    encodePreferenceHelp.classList.remove("is-warning");
    return;
  }
  if (!state.authenticated) {
    encodePreferenceHelp.textContent = "Enter the server password to load ffmpeg encoder availability.";
    encodePreferenceHelp.classList.remove("is-warning");
    return;
  }
  encodePreferenceHelp.textContent = "No exact ffmpeg encoder detected for this codec on the server.";
  encodePreferenceHelp.classList.add("is-warning");
}

async function refreshCodecOptions(
  preferredValue = codecSelect.value,
  { silent = false } = {},
) {
  let options = DEFAULT_CODEC_OPTIONS;
  if (state.authenticated) {
    try {
      const response = await fetch(apiUrl("/api/codecs"), {
        cache: "no-store",
        credentials: "include",
      });
      if (!response.ok) {
        throw new Error(await response.text().catch(() => "Failed to load codecs"));
      }
      const body = await response.json();
      if (Array.isArray(body?.options) && body.options.length > 0) {
        options = body.options;
      }
    } catch (error) {
      if (!silent) {
        showToast("codec_options_failed", error.message || String(error));
      }
    }
  }
  const previousValue = codecSelect.value;
  setCodecOptions(options, preferredValue);
  if (previousValue !== codecSelect.value && preferredValue === previousValue && !silent) {
    showToast("codec_unavailable", `${preferredValue.toUpperCase()} is not available on this server`);
  }
}

async function refreshEncodePreferenceOptions(
  codec = codecSelect.value,
  preferredValue = encodePreferenceSelect.value,
  { silent = false } = {},
) {
  let options = defaultEncodeOptionsForCodec(codec);
  if (state.authenticated) {
    try {
      const url = apiUrl("/api/encoders");
      url.searchParams.set("codec", codec);
      const response = await fetch(url, { cache: "no-store", credentials: "include" });
      if (!response.ok) {
        throw new Error(await response.text().catch(() => "Failed to load encoders"));
      }
      const body = await response.json();
      if (Array.isArray(body?.options) && body.options.length > 0) {
        options = body.options;
      }
    } catch (error) {
      if (!silent) {
        showToast("encoder_options_failed", error.message || String(error));
      }
    }
  }
  state.encoderOptionsByCodec[codec] = options;
  renderEncodePreferenceOptions(options, preferredValue);
}

async function disconnectIfSelectedCodecIsUnsupported(settings = readSettingsFromControls()) {
  if (!isConnected()) return true;
  const videoCodecSupport = await getVideoCodecSupport(settings.codec);
  if (videoCodecSupport.supported) {
    return true;
  }
  clearSettingsReconnectTimer();
  state.pendingStreamSettingsKey = "";
  persistResolvedSettings(settings, { scheduleReconnect: false });
  showToast("codec_unsupported", `${videoCodecSupport.message}. Disconnected.`);
  disconnect();
  return false;
}

async function handleCodecSettingChange() {
  const provisionalSettings = readSettingsFromControls();
  syncPendingStreamSettings(provisionalSettings);
  const preferredValue = encodePreferenceSelect.value;
  await refreshEncodePreferenceOptions(codecSelect.value, preferredValue, { silent: true });
  const settings = readSettingsFromControls();
  if (!(await disconnectIfSelectedCodecIsUnsupported(settings))) {
    return;
  }
  persistResolvedSettings(settings);
}

function handleEncoderSettingChange() {
  persistCurrentSettings();
}

async function getVideoCodecSupport(codec) {
  const codecStrings = videoCodecCandidates(codec);
  if (codecStrings.length === 0) {
    return { supported: false, message: `Unknown video codec: ${codec}` };
  }
  const resolved = await resolveSupportedVideoCodecString(codecStrings);
  return resolved.supported
    ? { supported: true, codecString: resolved.codecString }
    : { supported: false, message: `This browser does not support ${codec.toUpperCase()} video decode` };
}

function videoCodecCandidates(codecOrCodecString) {
  if (!codecOrCodecString) return [];
  if (codecOrCodecString === "h265") {
    return ["hvc1.1.6.L93.B0", "hev1.1.6.L93.B0"];
  }
  if (codecOrCodecString === "vp9") {
    return ["vp09.00.10.08", "vp09.00.10.08.01.01.01.01.00"];
  }
  if (codecOrCodecString === "av1") {
    return ["av01.0.08M.08", "av01.0.05M.08"];
  }
  if (VIDEO_CODEC_STRINGS[codecOrCodecString]) {
    return [VIDEO_CODEC_STRINGS[codecOrCodecString]];
  }
  if (codecOrCodecString.startsWith("hvc1.")) {
    return [codecOrCodecString, codecOrCodecString.replace(/^hvc1\./, "hev1.")];
  }
  if (codecOrCodecString.startsWith("hev1.")) {
    return [codecOrCodecString, codecOrCodecString.replace(/^hev1\./, "hvc1.")];
  }
  return [codecOrCodecString];
}

async function resolveSupportedVideoCodecString(codecStrings, description = null) {
  let decoder = null;
  for (const codecString of codecStrings) {
    const config = { codec: codecString, optimizeForLatency: true };
    if (description) {
      config.description = description;
    }
    if (!("VideoDecoder" in window)) {
      return { supported: false };
    }
    if (typeof VideoDecoder.isConfigSupported === "function") {
      try {
        const result = await VideoDecoder.isConfigSupported(config);
        if (result.supported) {
          return { supported: true, codecString };
        }
      } catch {
        // Fall back to direct configure probing below.
      }
    }
    try {
      decoder = new VideoDecoder({
        output: (frame) => frame.close(),
        error: () => {},
      });
      decoder.configure(config);
      return { supported: true, codecString };
    } catch {
      decoder?.close();
      decoder = null;
    }
  }
  return { supported: false };
}

function normalizeClipboardHistory(entries) {
  if (!Array.isArray(entries)) return [];
  return entries
    .map((entry) => ({
      side: entry?.side === "remote" ? "remote" : "local",
      payload: normalizeClipboardPayload(entry?.payload),
    }))
    .filter((entry) => hasClipboardContent(entry.payload))
    .slice(0, CLIPBOARD_HISTORY_LIMIT);
}

function setAuthPrompt(message = "") {
  authModal.classList.remove("hidden");
  authError.textContent = message;
  authError.classList.toggle("hidden", !message);
  authInput.focus({ preventScroll: true });
  authInput.select();
}

function clearAuthPrompt() {
  authError.textContent = "";
  authError.classList.add("hidden");
  authModal.classList.add("hidden");
}

async function verifyPassword(passwd) {
  const response = await fetch(apiUrl("/api/auth"), {
    cache: "no-store",
    credentials: "include",
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ passwd }),
  });
  if (!response.ok) {
    const message = await response.text().catch(() => "");
    throw new Error(message || "Authentication failed");
  }
}

async function loadClipboardHistory() {
  const response = await fetch(apiUrl("/api/clipboard/history"), {
    cache: "no-store",
    credentials: "include",
  });
  if (!response.ok) {
    throw new Error(await response.text().catch(() => "Failed to load clipboard history"));
  }
  state.clipboardHistory = normalizeClipboardHistory(await response.json());
  renderClipboardHistory();
}

async function saveClipboardHistory() {
  const response = await fetch(apiUrl("/api/clipboard/history"), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(state.clipboardHistory),
  });
  if (!response.ok) {
    throw new Error(await response.text().catch(() => "Failed to save clipboard history"));
  }
}

function isAuthFailureMessage(message) {
  if (typeof message !== "string") return false;
  const lower = message.toLowerCase();
  return lower.includes("authentication required")
    || lower.includes("invalid or missing passwd")
    || lower.includes("authentication failed");
}

function appendStreamQuery(url, settings) {
  if (state.sessionId) {
    url.searchParams.set("client_id", state.sessionId);
  }
  url.searchParams.set("codec", settings.codec);
  url.searchParams.set("encode_preference", settings.encodePreference);
  url.searchParams.set("bitrate_kbps", settings.bitrate);
  url.searchParams.set("audio_bitrate_kbps", settings.audioBitrateKbps);
  url.searchParams.set("fps", settings.fps);
  url.searchParams.set("encoder_latency", settings.encoderLatency);
  url.searchParams.set("encoder_quality", settings.encoderQuality);
  url.searchParams.set("scale", settings.videoScale);
  url.searchParams.set("gop_ms", settings.gopMs);
  url.searchParams.set("buffer_ms", settings.bufferMs);
  if (state.sessionPasswd) {
    url.searchParams.set("passwd", state.sessionPasswd);
  }
}

function roleWebSocketUrl(role, settings) {
  const url = webSocketUrl("/ws");
  appendStreamQuery(url, settings);
  url.searchParams.set("role", role);
  return url;
}

function createClientSessionId() {
  if (globalThis.crypto?.randomUUID) {
    return globalThis.crypto.randomUUID();
  }
  return `${Date.now().toString(16)}-${Math.random().toString(16).slice(2)}`;
}

const TRANSPORT_STORAGE_KEY = "vibe_rdesk_transport";

function isWebTransportSupported() {
  return typeof WebTransport !== "undefined" && window.isSecureContext;
}

function loadStoredTransport() {
  // An explicit ?wt=1 / ?wt=0 query parameter wins, then a stored preference.
  try {
    const params = new URLSearchParams(window.location.search);
    if (params.has("wt")) {
      const value = params.get("wt");
      return value === "0" || value === "false" ? "websocket" : "webtransport";
    }
  } catch {
    // Ignore malformed query strings.
  }
  try {
    const stored = window.localStorage?.getItem(TRANSPORT_STORAGE_KEY);
    if (stored === "webtransport" || stored === "websocket") return stored;
  } catch {
    // Ignore storage access errors (private mode, etc.).
  }
  return "websocket";
}

function saveTransport(transport) {
  try {
    window.localStorage?.setItem(TRANSPORT_STORAGE_KEY, transport);
  } catch {
    // Ignore storage write failures.
  }
}

// Resolves the transport to actually use, downgrading to WebSocket when
// WebTransport is unavailable in this browser/context.
function currentTransport() {
  if (state.transport === "webtransport" && isWebTransportSupported()) {
    return "webtransport";
  }
  return "websocket";
}

// Fetches (fresh each connect, since the self-signed cert rotates on restart)
// the WebTransport endpoint metadata the browser needs to connect.
async function ensureWtInfo() {
  const url = apiUrl("/api/wt-info");
  const response = await fetch(url, { cache: "no-store", credentials: "include" });
  if (!response.ok) throw new Error(`wt-info request failed (${response.status})`);
  const data = await response.json();
  if (!data.enabled || !data.port) {
    throw new Error("WebTransport is not available on the server");
  }
  state.wtInfo = data;
  return data;
}

function base64ToBytes(value) {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

// Reads the first 5 bytes (frame header) spread across a queue of chunks and
// returns the big-endian u32 payload length at offset 1. Assumes >= 5 bytes are
// buffered across `chunks`.
function peekFrameLength(chunks) {
  const header = new Uint8Array(5);
  let filled = 0;
  for (let c = 0; c < chunks.length && filled < 5; c += 1) {
    const chunk = chunks[c];
    const take = Math.min(5 - filled, chunk.length);
    header.set(chunk.subarray(0, take), filled);
    filled += take;
  }
  return ((header[1] << 24) | (header[2] << 16) | (header[3] << 8) | header[4]) >>> 0;
}

// Removes and returns the first `count` bytes from a queue of chunks, splitting
// the boundary chunk in place. Each byte is copied at most once, so a frame that
// spans many QUIC packets costs O(frame) rather than O(frame²) to assemble.
function takeBytes(chunks, count) {
  const out = new Uint8Array(count);
  let filled = 0;
  while (filled < count) {
    const chunk = chunks[0];
    const remaining = count - filled;
    if (chunk.length <= remaining) {
      out.set(chunk, filled);
      filled += chunk.length;
      chunks.shift();
    } else {
      out.set(chunk.subarray(0, remaining), filled);
      chunks[0] = chunk.subarray(remaining);
      filled += remaining;
    }
  }
  return out;
}

// A drop-in replacement for the parts of the WebSocket API the client uses,
// backed by a WebTransport session: a bidirectional stream carries control and
// input traffic while a server-initiated unidirectional stream carries video.
// Messages are framed as [u8 kind][u32be length][payload]; kind 0 = binary,
// 1 = text, 2 = close.
class WtSocket {
  constructor(url, { certHashes } = {}) {
    this.url = url;
    this.binaryType = "arraybuffer";
    this.readyState = 0; // CONNECTING
    this.onopen = null;
    this.onerror = null;
    this.onclose = null;
    this.onmessage = null;
    this._transport = null;
    this._writer = null;
    this._writeChain = Promise.resolve();
    this._closed = false;
    this._textDecoder = new TextDecoder();
    this._textEncoder = new TextEncoder();
    this._buffer = new Uint8Array(0);
    this._start(certHashes);
  }

  async _start(certHashes) {
    try {
      const options = {};
      if (Array.isArray(certHashes) && certHashes.length) {
        options.serverCertificateHashes = certHashes.map((hash) => ({
          algorithm: "sha-256",
          value: base64ToBytes(hash),
        }));
      }
      const transport = new WebTransport(this.url, options);
      this._transport = transport;
      transport.closed
        .then(() => this._handleClosed(1000, "transport closed"))
        .catch((err) => this._handleClosed(1006, String(err?.message || err)));
      await transport.ready;
      if (this._closed) return;
      const stream = await transport.createBidirectionalStream();
      this._writer = stream.writable.getWriter();
      this._readLoop(stream.readable.getReader());
      // Video is delivered on a dedicated server-initiated unidirectional
      // stream (see webtransport.rs): start accepting those in parallel.
      this._acceptUniStreams();
      this.readyState = 1; // OPEN
      // Browsers don't expose a freshly opened bidirectional stream to the
      // server until the client writes to it. Send a no-op frame immediately so
      // read-only streams (video/audio) start flowing right away.
      this._writeChain = this._writer.write(this._buildFrame(3, new Uint8Array(0))).catch(() => {});
      if (this.onopen) this.onopen({ target: this });
    } catch (err) {
      if (this.onerror) this.onerror({ target: this, message: String(err?.message || err) });
      this._handleClosed(1006, String(err?.message || err));
    }
  }

  async _readLoop(reader) {
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value && value.length) {
          this._append(value);
          if (this._drainFrames()) return; // close frame consumed
        }
      }
      this._handleClosed(1000, "stream finished");
    } catch (err) {
      this._handleClosed(1006, String(err?.message || err));
    }
  }

  // Accepts server-initiated unidirectional streams. The server carries video on
  // its own stream so a congested video backlog never head-of-line-blocks the
  // control/input traffic on the bidirectional stream. Each stream is a flow of
  // length-prefixed binary frames identical to the bidirectional framing.
  async _acceptUniStreams() {
    const incoming = this._transport?.incomingUnidirectionalStreams;
    if (!incoming) return;
    let reader;
    try {
      reader = incoming.getReader();
    } catch {
      return;
    }
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value) this._readUniStream(value);
      }
    } catch {
      // Transport closed; the bidirectional read loop reports the closure.
    }
  }

  // Reads one unidirectional (video) stream, dispatching each complete
  // [u8 kind][u32be length][payload] frame's payload as a binary message. The
  // kind byte is always binary on this stream. Chunks are held in a queue and
  // each byte is copied at most once even when a frame spans many QUIC packets.
  async _readUniStream(readable) {
    let reader;
    try {
      reader = readable.getReader();
    } catch {
      return;
    }
    const chunks = [];
    let buffered = 0;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        if (!value || !value.length) continue;
        chunks.push(value);
        buffered += value.length;
        for (;;) {
          if (buffered < 5) break;
          const length = peekFrameLength(chunks);
          const frameLength = 5 + length;
          if (buffered < frameLength) break;
          const frame = takeBytes(chunks, frameLength);
          buffered -= frameLength;
          const payload = frame.subarray(5);
          if (this.onmessage) this.onmessage({ data: payload.slice().buffer, target: this });
        }
      }
    } catch {
      // Stream reset (the server abandoned a superseded frame) or transport
      // closed: drop any partial frame and move on.
    }
  }

  _append(chunk) {
    if (this._buffer.length === 0) {
      this._buffer = chunk.slice();
      return;
    }
    const merged = new Uint8Array(this._buffer.length + chunk.length);
    merged.set(this._buffer, 0);
    merged.set(chunk, this._buffer.length);
    this._buffer = merged;
  }

  // Parses and dispatches every complete frame currently buffered. Returns true
  // when a close frame was seen (the caller should stop reading).
  _drainFrames() {
    let offset = 0;
    const buffer = this._buffer;
    const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength);
    while (buffer.length - offset >= 5) {
      const kind = buffer[offset];
      const length = view.getUint32(offset + 1, false);
      if (buffer.length - offset - 5 < length) break;
      const start = offset + 5;
      const payload = buffer.subarray(start, start + length);
      offset = start + length;
      if (kind === 2) {
        const code = length >= 2 ? new DataView(payload.buffer, payload.byteOffset, 2).getUint16(0, false) : 1000;
        this._buffer = buffer.subarray(offset).slice();
        this._handleClosed(code, "");
        return true;
      }
      if (kind === 1) {
        const text = this._textDecoder.decode(payload);
        if (this.onmessage) this.onmessage({ data: text, target: this });
      } else {
        const copy = payload.slice();
        if (this.onmessage) this.onmessage({ data: copy.buffer, target: this });
      }
    }
    this._buffer = offset === 0 ? buffer : buffer.subarray(offset).slice();
    return false;
  }

  _buildFrame(kind, payload) {
    const frame = new Uint8Array(5 + payload.length);
    frame[0] = kind;
    new DataView(frame.buffer).setUint32(1, payload.length, false);
    frame.set(payload, 5);
    return frame;
  }

  send(data) {
    if (this.readyState !== 1 || !this._writer) return;
    let frame;
    if (typeof data === "string") {
      frame = this._buildFrame(1, this._textEncoder.encode(data));
    } else if (data instanceof ArrayBuffer) {
      frame = this._buildFrame(0, new Uint8Array(data));
    } else if (ArrayBuffer.isView(data)) {
      frame = this._buildFrame(0, new Uint8Array(data.buffer, data.byteOffset, data.byteLength));
    } else {
      return;
    }
    const writer = this._writer;
    this._writeChain = this._writeChain
      .then(() => writer.write(frame))
      .catch((err) => {
        if (this.onerror) this.onerror({ target: this, message: String(err?.message || err) });
      });
  }

  close() {
    if (this._closed && this.readyState === 3) return;
    this.readyState = 2; // CLOSING
    const writer = this._writer;
    const transport = this._transport;
    if (writer) {
      const closeFrame = this._buildFrame(2, new Uint8Array([0x03, 0xe8])); // 1000
      this._writeChain = this._writeChain
        .then(() => writer.write(closeFrame))
        .then(() => writer.close())
        .catch(() => {})
        .finally(() => {
          try {
            transport?.close();
          } catch {
            // Already closing.
          }
        });
    } else {
      try {
        transport?.close();
      } catch {
        // Already closing.
      }
    }
    this._handleClosed(1000, "");
  }

  _handleClosed(code, reason) {
    if (this._closed) return;
    this._closed = true;
    this.readyState = 3; // CLOSED
    if (this.onclose) this.onclose({ code, reason, target: this });
  }
}

function isSocketOpen(socket) {
  return socket?.readyState === WebSocket.OPEN;
}

function isConnected() {
  return isSocketOpen(state.socket);
}

function isInputConnected() {
  return isSocketOpen(state.inputSocket);
}

function isFullyConnected() {
  return isSocketOpen(state.socket)
    && isSocketOpen(state.inputSocket)
    && isSocketOpen(state.videoSocket)
    && isSocketOpen(state.audioSocket)
    && isSocketOpen(state.micSocket);
}

function roleWebTransportUrl(role, settings, port) {
  const url = apiUrl("/ws");
  url.protocol = "https:";
  url.port = String(port);
  appendStreamQuery(url, settings);
  url.searchParams.set("role", role);
  return url;
}

function openRoleSocket(role, settings, onMessage) {
  const transport = currentTransport();
  const label = transport === "webtransport" ? "WebTransport" : "WebSocket";
  let socket;
  if (transport === "webtransport") {
    const info = state.wtInfo;
    socket = new WtSocket(roleWebTransportUrl(role, settings, info.port).toString(), {
      certHashes: info.cert_hashes,
    });
  } else {
    socket = new WebSocket(roleWebSocketUrl(role, settings));
  }
  socket.binaryType = "arraybuffer";
  let opened = false;
  const openPromise = new Promise((resolve, reject) => {
    socket.onopen = () => {
      opened = true;
      resolve(socket);
    };
    socket.onerror = () => {
      if (!opened) {
        reject(new Error(`${role} ${label} error`));
      } else {
        showToast("ws_error", `${role} ${label} error`);
      }
    };
    socket.onclose = (event) => {
      if (!opened) {
        reject(new Error(`${role} ${label} closed (${event.code || "no code"})`));
        return;
      }
      handleRoleSocketClose(role, event);
    };
  });
  socket.onmessage = onMessage;
  return { socket, openPromise };
}

function handleRoleSocketClose(role, event) {
  if (state.manualDisconnect) return;
  if (event.code === 4000) {
    showToast("client_closed", "This webclient was closed from another browser");
    closeConnection({ manual: true });
    void refreshWebClients();
    return;
  }
  setStatus("Disconnected");
  if (event.code && event.code !== 1000 && !state.reconnectTimer) {
    showToast("ws_closed", `${role} WebSocket closed (${event.code})`);
  }
  closeConnection({ manual: false, preserveStatus: true });
  scheduleReconnect();
}

function appendAuthQuery(url) {
  if (state.sessionId) {
    url.searchParams.set("client_id", state.sessionId);
  }
  if (state.sessionPasswd) {
    url.searchParams.set("passwd", state.sessionPasswd);
  }
  return url;
}

async function refreshWebClients() {
  if (!state.authenticated && !state.sessionPasswd) {
    renderWebClients([]);
    return;
  }
  try {
    const url = appendAuthQuery(apiUrl("/api/webclients"));
    const response = await fetch(url, { cache: "no-store", credentials: "include" });
    if (!response.ok) throw new Error(await response.text());
    const data = await response.json();
    renderWebClients(Array.isArray(data.clients) ? data.clients : []);
  } catch (error) {
    webclientsCount.textContent = "--";
    if (state.webclientsManagerOpen) {
      webclientsList.textContent = error.message || String(error);
    }
  }
}

function renderWebClients(clients) {
  state.webclients = clients;
  webclientsCount.textContent = `${clients.length}`;
  webclientsList.textContent = "";
  if (!clients.length) {
    const empty = document.createElement("div");
    empty.className = "metric-meta";
    empty.textContent = "No connected webclients";
    webclientsList.append(empty);
    return;
  }
  for (const client of clients) {
    const row = document.createElement("div");
    row.className = "webclient-row";
    const main = document.createElement("div");
    main.className = "webclient-main";
    const ip = document.createElement("div");
    ip.className = "webclient-ip";
    ip.textContent = client.id === state.sessionId ? `${client.ip} (this client)` : client.ip;
    const meta = document.createElement("div");
    meta.className = "webclient-meta";
    meta.textContent = Array.isArray(client.roles) && client.roles.length
      ? client.roles.join(", ")
      : "connected";
    main.append(ip, meta);
    const close = document.createElement("button");
    close.type = "button";
    close.className = "danger";
    close.textContent = "Close";
    close.disabled = client.id === state.sessionId;
    close.addEventListener("click", () => {
      void closeWebClient(client.id);
    });
    row.append(main, close);
    webclientsList.append(row);
  }
}

function setWebclientsManagerOpen(open) {
  state.webclientsManagerOpen = open;
  webclientsManager.classList.toggle("hidden", !open);
  webclientsToggle.setAttribute("aria-expanded", open ? "true" : "false");
  if (open) void refreshWebClients();
}

function renderProtocolStatus() {
  if (!protocolValue) return;
  const active = currentTransport();
  protocolValue.textContent = active === "webtransport" ? "WebTransport" : "WebSocket";
  if (protocolMenu) {
    const supported = isWebTransportSupported();
    for (const option of protocolMenu.querySelectorAll(".protocol-option")) {
      const value = option.dataset.protocol;
      const disabled = value === "webtransport" && !supported;
      option.disabled = disabled;
      option.title = disabled ? "WebTransport requires a secure (HTTPS) context" : "";
      option.classList.toggle("is-active", value === active);
      option.setAttribute("aria-selected", value === active ? "true" : "false");
    }
  }
}

function setProtocolMenuOpen(open) {
  state.protocolMenuOpen = open;
  if (protocolMenu) protocolMenu.classList.toggle("hidden", !open);
  if (protocolToggle) protocolToggle.setAttribute("aria-expanded", open ? "true" : "false");
}

async function selectTransport(transport) {
  setProtocolMenuOpen(false);
  if (transport !== "websocket" && transport !== "webtransport") return;
  if (transport === "webtransport" && !isWebTransportSupported()) {
    showToast("wt_unsupported", "WebTransport is not supported in this browser or context");
    return;
  }
  if (transport === state.transport) return;
  const reconnect = isConnected() || state.connecting || !isConnectionDisconnected();
  state.transport = transport;
  state.wtInfo = null;
  saveTransport(transport);
  renderProtocolStatus();
  if (reconnect) {
    closeConnection({ manual: true });
    state.manualDisconnect = false;
    await connect();
  }
}

async function closeWebClient(clientId) {
  if (!clientId || clientId === state.sessionId) return;
  try {
    const url = appendAuthQuery(apiUrl(`/api/webclients/${encodeURIComponent(clientId)}/close`));
    const response = await fetch(url, { method: "POST", credentials: "include" });
    if (!response.ok) throw new Error(await response.text());
    await refreshWebClients();
  } catch (error) {
    showToast("webclient_close_failed", error.message || String(error));
  }
}

async function closeOtherWebClients() {
  if (!state.sessionId) return;
  try {
    const url = appendAuthQuery(apiUrl("/api/webclients/close-others"));
    const response = await fetch(url, {
      method: "POST",
      credentials: "include",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ client_id: state.sessionId }),
    });
    if (!response.ok) throw new Error(await response.text());
    await refreshWebClients();
  } catch (error) {
    showToast("webclient_close_failed", error.message || String(error));
  }
}

function handleControlSocketMessage(event) {
  if (typeof event.data === "string") {
    handleServerMessage(JSON.parse(event.data));
  }
}

function handleMediaSocketMessage(event) {
  if (typeof event.data === "string") {
    handleServerMessage(JSON.parse(event.data));
  } else {
    handleFrame(event.data);
  }
}

function handleMicSocketMessage(event) {
  if (typeof event.data === "string") {
    handleServerMessage(JSON.parse(event.data));
  }
}

async function connect() {
  if (state.connecting) return;
  state.apiOrigin = normalizeApiOrigin(authOriginInput.value || state.apiOrigin);
  saveApiOrigin(state.apiOrigin);
  const needsLogin = !state.authenticated;
  const passwd = authInput.value.trim() || state.sessionPasswd;
  if (needsLogin && !passwd) {
    setAuthPrompt("Enter the server password.");
    return;
  }
  state.connecting = true;
  try {
    if (needsLogin) {
      setStatus("Authenticating...");
      await verifyPassword(passwd);
      state.authenticated = true;
      state.sessionPasswd = passwd;
      authInput.value = passwd;
      savePasswd(passwd);
      clearAuthPrompt();
    }
    await refreshCodecOptions(readSettingsFromControls().codec, { silent: true });
    const settingsBeforeConnect = readSettingsFromControls();
    await refreshEncodePreferenceOptions(
      settingsBeforeConnect.codec,
      settingsBeforeConnect.encodePreference,
      { silent: true },
    );
    await setAudioOutputMode(settingsBeforeConnect.audioUseRealOutput, { silent: true });
    closeConnection({ manual: false, preserveStatus: true });
    state.manualDisconnect = false;
    clearReconnectTimer();
    void primeAudioPlayback();
    const settings = readSettingsFromControls();
    const videoCodecSupport = await getVideoCodecSupport(settings.codec);
    if (!videoCodecSupport.supported) {
      setStatus("Disconnected");
      setEncoderStatus("Not connected");
      showToast("codec_unsupported", videoCodecSupport.message);
      return;
    }
    state.activeCodec = settings.codec;
    if (videoCodecSupport.codecString) {
      state.codecString = videoCodecSupport.codecString;
    }
    state.frameCount = 0;
    state.bytesReceived = 0;
    state.netWindowBytes = 0;
    state.netKbps = 0;
    state.lastNetAt = performance.now();
    setStatus("Connecting...");
    setEncoderStatus("Connecting...");
    state.sessionId = createClientSessionId();
    if (currentTransport() === "webtransport") {
      try {
        await ensureWtInfo();
      } catch (error) {
        showToast("wt_unavailable", `WebTransport unavailable, using WebSocket: ${error.message || error}`);
        state.transport = "websocket";
        saveTransport("websocket");
        renderProtocolStatus();
      }
    }
    const control = openRoleSocket("control", settings, handleControlSocketMessage);
    const input = openRoleSocket("input", settings, handleControlSocketMessage);
    const video = openRoleSocket("video", settings, handleMediaSocketMessage);
    const audio = openRoleSocket("audio", settings, handleMediaSocketMessage);
    const mic = openRoleSocket("mic", settings, handleMicSocketMessage);
    state.socket = control.socket;
    state.inputSocket = input.socket;
    state.videoSocket = video.socket;
    state.audioSocket = audio.socket;
    state.micSocket = mic.socket;
    await Promise.all([
      control.openPromise,
      input.openPromise,
      video.openPromise,
      audio.openPromise,
      mic.openPromise,
    ]);
    markAppliedStreamSettings(settings);
    state.reconnectAttempt = 0;
    state.reconnectingForLatency = false;
    state.highLatencySinceAt = 0;
    setStreamWarning("");
    setStatus("Connected");
    void refreshCodecOptions(codecSelect.value, { silent: true });
    void refreshEncodePreferenceOptions(
      codecSelect.value,
      loadStoredSettings().encodePreference,
      { silent: true },
    );
    void loadClipboardHistory().catch((error) => {
      showToast("history_load_failed", error.message || String(error));
    });
    startKeyStateSync();
    startPing();
    startRemoteClipboardPolling();
    syncAutoDisconnectTimer();
    void refreshLocalClipboard();
    if (state.micEnabled) {
      void startMicrophoneCapture();
    }
    maybeScheduleSettingsReconnect();
    void refreshWebClients();
  } catch (error) {
    const message = error.message || String(error);
    closeConnection({ manual: false, preserveStatus: true });
    if (needsLogin || isAuthFailureMessage(message)) {
      state.authenticated = false;
      showToast("auth_failed", message);
      setAuthPrompt(message || "Authentication failed");
    } else {
      showToast("connect_failed", message);
      if (!state.manualDisconnect) scheduleReconnect();
    }
    setStatus(isConnected() ? "Connected" : "Disconnected");
  } finally {
    state.connecting = false;
  }
}

function closeConnection({ manual = true, preserveStatus = false, keepCameraEnabled = false } = {}) {
  state.manualDisconnect = manual;
  clearReconnectTimer();
  clearSettingsReconnectTimer();
  clearAutoDisconnectTimer();
  stopKeyStateSync();
  clearInterval(startPing.timer);
  state.latencyProbeSentAt.clear();
  state.lastVideoPacketAt = 0;
  state.lastVideoSeq = null;
  state.lastAudioPacketAt = 0;
  if (!preserveStatus) {
    state.wsLatencyMs = null;
    state.serverClockOffsetMs = 0;
    renderLatencyMetric();
  }
  if (manual) {
    state.reconnectingForLatency = false;
  }
  stopRemoteClipboardPolling();
  stopMicrophoneCapture();
  void stopCameraCapture({ notifyServer: true, keepEnabled: keepCameraEnabled });
  clearPendingPointerMotion();
  cancelVideoFrameRender();
  clearPendingVideoFrame();
  clearWorkerVideoFrame();
  state.renderingVideoFrame = false;
  state.decoder?.close();
  state.decoder = null;
  state.audioDecoder?.close();
  state.audioDecoder = null;
  resetAudioPlayback();
  state.decoderConfigKey = "";
  state.audioConfigKey = "";
  state.decoderRecovering = false;
  state.audioEnabled = false;
  state.sessionId = "";
  state.waitingForKeyframe = true;
  state.lastVideoFrameRenderedAt = 0;
  state.netWindowBytes = 0;
  state.netKbps = 0;
  resetKeys();
  const sockets = [state.socket, state.inputSocket, state.videoSocket, state.audioSocket, state.micSocket];
  state.socket = null;
  state.inputSocket = null;
  state.videoSocket = null;
  state.audioSocket = null;
  state.micSocket = null;
  for (const socket of sockets) {
    if (!socket) continue;
    socket.onclose = null;
    socket.onerror = null;
    socket.onmessage = null;
    try {
      socket.close();
    } catch {
      // Ignore close errors while replacing a connection.
    }
  }
  if (!preserveStatus) setEncoderStatus("Not connected");
  if (!preserveStatus) setStatus("Disconnected");
  if (!preserveStatus) resetStatusMetrics();
}

function disconnect() {
  closeConnection({ manual: true });
}

function isConnectionOpen() {
  return isFullyConnected();
}

function isConnectionDisconnected() {
  return [state.socket, state.inputSocket, state.videoSocket, state.audioSocket, state.micSocket]
    .every((socket) => !socket || socket.readyState === WebSocket.CLOSED);
}

function reconnectFromViewport() {
  if (state.connecting || isConnectionOpen() || !isConnectionDisconnected()) return false;
  void connect();
  return true;
}

async function requestKeyboardLock() {
  if (!navigator.keyboard?.lock) return;
  try {
    await navigator.keyboard.lock();
  } catch {
    // Some browsers or contexts reject keyboard lock; key handling still works without it.
  }
}

function releaseKeyboardLock() {
  navigator.keyboard?.unlock?.();
}

function captureInput() {
  captureInputTarget(canvas);
}

function captureInputTarget(target) {
  state.inputCaptured = true;
  target.focus({ preventScroll: true });
  void requestKeyboardLock();
  void primeAudioPlayback();
}

function releaseInput() {
  resetTouchInteraction();
  state.inputCaptured = false;
  if (document.activeElement === mobileKeyboardInput) {
    mobileKeyboardInput.blur();
  }
  window.navigator.virtualKeyboard?.hide?.();
  state.mobileKeyboardComposing = false;
  clearMobileKeyboardFlushTimer();
  mobileKeyboardInput.value = "";
  syncMobileKeyboardButton();
  releaseKeyboardLock();
  resetKeys();
  send({ type: "reset_input" });
}

function scheduleReconnect() {
  if (state.reconnectTimer) return;
  state.reconnectAttempt += 1;
  const delay = Math.min(1000 * (2 ** (state.reconnectAttempt - 1)), 5000);
  setStatus(`Reconnecting in ${Math.round(delay / 1000)}s`);
  state.reconnectTimer = setTimeout(() => {
    state.reconnectTimer = null;
    connect();
  }, delay);
}

function handleServerMessage(message) {
  if (message.type === "hello") {
    updateServerClockOffset(message.server_time_ms);
    if (message.session_id && !state.sessionId) {
      state.sessionId = message.session_id;
    }
    updateRemoteScreenSize(message.screen_width, message.screen_height);
    if (message.config) {
      syncServerStreamSettings(message.config, message.audio_config, {
        force: message.config_fallback === true,
      });
    }
    const codecStringChanged = typeof message.codec_string === "string"
      && message.codec_string !== state.codecString;
    if (message.codec_string) {
      state.codecString = message.codec_string;
    }
    if (message.config?.codec) {
      state.activeCodec = message.config.codec;
    }
    if (message.description_b64) {
      const description = Uint8Array.from(atob(message.description_b64), (c) => c.charCodeAt(0));
      const descriptionChanged = !byteArraysEqual(state.description, description);
      state.description = description;
      if (descriptionChanged) {
        closeVideoDecoderForReconfigure();
      }
    } else if (codecStringChanged) {
      state.description = null;
      closeVideoDecoderForReconfigure();
    }
    state.audioEnabled = !!message.audio_enabled;
    void setupDecoder();
    if (!state.audioEnabled) {
      state.audioDecoder?.close();
      state.audioDecoder = null;
      resetAudioPlayback();
      state.audioConfigKey = "";
    }
    setEncoderStatus(`${message.active_encoder || "ready"} ${message.encoder_mode || ""}`.trim());
  } else if (message.type === "stats") {
    setEncoderStatus(`${message.active_encoder || "ready"} ${message.encoder_mode || ""}`.trim());
    renderStatusMetrics(message);
  } else if (message.type === "pong") {
    const sentAt = state.latencyProbeSentAt.get(message.seq);
    if (sentAt !== undefined) {
      state.latencyProbeSentAt.delete(message.seq);
      state.wsLatencyMs = performance.now() - sentAt;
      updateServerClockOffset(message.server_time_ms, state.wsLatencyMs);
      renderLatencyMetric();
    }
  } else if (message.type === "error") {
    pushError(message.code, message.message);
  } else if (message.type === "clipboard" && message.side === "remote") {
    updateClipboardState("remote", message.payload, { announce: true });
  }
}

function byteArraysEqual(left, right) {
  if (left === right) return true;
  if (!left || !right || left.length !== right.length) return false;
  for (let idx = 0; idx < left.length; idx += 1) {
    if (left[idx] !== right[idx]) return false;
  }
  return true;
}

async function setupDecoder() {
  if (!("VideoDecoder" in window)) {
    showToast("webcodecs_missing", "This browser does not support WebCodecs");
    return;
  }
  const codecSupport = await resolveSupportedVideoCodecString(
    videoCodecCandidates(state.codecString),
    state.description,
  );
  if (!codecSupport.supported) {
    state.decoder?.close();
    state.decoder = null;
    state.decoderConfigKey = "";
    showToast("decoder_config_failed", `Decoder does not support ${state.codecString}. Disconnected.`);
    disconnect();
    return;
  }
  const selectedCodecString = codecSupport.codecString;
  const configKey = `${selectedCodecString}:${state.description ? btoa(String.fromCharCode(...state.description)) : ""}`;
  if (state.decoder && state.decoder.state !== "closed" && state.decoderConfigKey === configKey) {
    state.codecString = selectedCodecString;
    return;
  }
  state.decoder?.close();
  state.decoder = new VideoDecoder({
    output: (frame) => {
      queueVideoFrameForRender(frame);
    },
    error: (err) => {
      recoverVideoDecoder(err?.message || String(err), { toastCode: "decoder_error" });
    },
  });
  const config = { codec: selectedCodecString, optimizeForLatency: true };
  if (state.description) config.description = state.description;
  try {
    state.decoder.configure(config);
    state.codecString = selectedCodecString;
    state.decoderConfigKey = configKey;
    state.waitingForKeyframe = true;
    state.lastVideoSeq = null;
  } catch (error) {
    state.decoder?.close();
    state.decoder = null;
    state.decoderConfigKey = "";
    showToast("decoder_config_failed", error.message || String(error));
  }
}

function closeVideoDecoderForReconfigure() {
  const decoder = state.decoder;
  state.decoder = null;
  state.decoderConfigKey = "";
  state.waitingForKeyframe = true;
  state.lastVideoSeq = null;
  try {
    decoder?.close();
  } catch {
    // The decoder may already be closed by an internal WebCodecs error.
  }
}

function recoverVideoDecoder(reason = "", { toastCode = "decoder_error" } = {}) {
  if (state.decoderRecovering) return;
  state.decoderRecovering = true;
  cancelVideoFrameRender();
  clearPendingVideoFrame();
  clearWorkerVideoFrame();
  state.renderingVideoFrame = false;
  const decoder = state.decoder;
  state.decoder = null;
  state.decoderConfigKey = "";
  state.waitingForKeyframe = true;
  state.lastVideoSeq = null;
  try {
    decoder?.close();
  } catch {
    // Some decoder failures already close the instance internally.
  }
  if (reason) {
    showToast(toastCode, reason);
  }
  queueMicrotask(async () => {
    try {
      await setupDecoder();
    } finally {
      state.decoderRecovering = false;
    }
  });
}

function clearPendingVideoFrame() {
  if (state.pendingVideoFrame) {
    state.pendingVideoFrame.close();
    state.pendingVideoFrame = null;
  }
}

function clearWorkerVideoFrame() {
  try {
    state.videoRenderWorker?.postMessage({ type: "clear" });
  } catch {
    // The worker may already be gone during a reconnect or page shutdown.
  }
}

function disableVideoRenderWorker() {
  const worker = state.videoRenderWorker;
  if (!worker) return;
  worker.removeEventListener("message", handleVideoRenderWorkerMessage);
  worker.terminate();
  state.videoRenderWorker = null;
}

function cancelVideoFrameRender() {
  if (!state.videoRenderRaf) return;
  cancelAnimationFrame(state.videoRenderRaf);
  state.videoRenderRaf = 0;
}

function ensureCanvasContext() {
  if (ctx) return ctx;
  ctx = canvas.getContext("2d", { alpha: false, desynchronized: true });
  if (!ctx) {
    throw new Error("Canvas 2D context is unavailable");
  }
  return ctx;
}

function handleVideoRenderWorkerMessage(event) {
  const message = event.data;
  if (!message || typeof message !== "object") return;
  if (message.type === "error") {
    showToast(message.code || "video_worker_error", message.message || "Video worker failed");
  } else if (message.type === "rendered") {
    state.frameCount += 1;
    state.lastVideoFrameRenderedAt = performance.now();
  }
}

function initVideoRenderer() {
  if (!ENABLE_VIDEO_RENDER_WORKER) return;
  if (state.videoRenderWorker) return;
  if (
    typeof Worker === "undefined"
    || typeof OffscreenCanvas === "undefined"
    || typeof canvas.transferControlToOffscreen !== "function"
  ) {
    return;
  }
  try {
    const worker = new Worker(new URL("./video_renderer_worker.js", window.location.href));
    worker.addEventListener("message", handleVideoRenderWorkerMessage);
    worker.addEventListener("error", (event) => {
      showToast("video_worker_error", event.message || "Video worker failed");
      setStreamWarning("Video worker failed");
    });
    const offscreenCanvas = canvas.transferControlToOffscreen();
    worker.postMessage({ type: "init", canvas: offscreenCanvas }, [offscreenCanvas]);
    state.videoRenderWorker = worker;
  } catch (error) {
    disableVideoRenderWorker();
    showToast("video_worker_unavailable", error.message || String(error));
  }
}

function updateVideoSurfaceSize(width, height) {
  const frameSizeChanged = state.videoFrameWidth !== width || state.videoFrameHeight !== height;
  if (state.videoRenderWorker) {
    if (frameSizeChanged) {
      try {
        state.videoRenderWorker.postMessage({ type: "resize", width, height });
      } catch {
        setStreamWarning("Video worker failed");
      }
    }
  } else if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  if (frameSizeChanged) {
    state.videoFrameWidth = width;
    state.videoFrameHeight = height;
    if (!state.remoteScreenWidth || !state.remoteScreenHeight) {
      state.remoteScreenWidth = width;
      state.remoteScreenHeight = height;
    }
    applyCanvasZoom();
  }
}

function updateRemoteScreenSize(width, height) {
  const screenWidth = Number(width);
  const screenHeight = Number(height);
  if (!Number.isFinite(screenWidth) || !Number.isFinite(screenHeight) || screenWidth <= 0 || screenHeight <= 0) {
    return;
  }
  if (state.remoteScreenWidth === screenWidth && state.remoteScreenHeight === screenHeight) {
    return;
  }
  state.remoteScreenWidth = screenWidth;
  state.remoteScreenHeight = screenHeight;
  applyCanvasZoom();
}

function resetVideoDecoderForLiveCatchup() {
  recoverVideoDecoder();
}

function queueVideoFrameForRender(frame) {
  updateVideoSurfaceSize(frame.displayWidth, frame.displayHeight);
  if (state.videoRenderWorker) {
    try {
      state.videoRenderWorker.postMessage({ type: "frame", frame }, [frame]);
    } catch (error) {
      frame.close();
      showToast("video_worker_send_failed", error?.message || String(error));
      setStreamWarning("Video worker failed");
    }
    return;
  }
  if (state.pendingVideoFrame) {
    // Keep only the latest decoded frame for the next paint. This is normal
    // queue coalescing and should not be surfaced as delayed playback.
    state.pendingVideoFrame.close();
  }
  state.pendingVideoFrame = frame;
  scheduleVideoFrameRender();
}

function scheduleVideoFrameRender() {
  if (state.videoRenderRaf || state.renderingVideoFrame) return;
  state.videoRenderRaf = requestAnimationFrame(() => {
    state.videoRenderRaf = 0;
    void renderLatestVideoFrame();
  });
}

async function renderLatestVideoFrame() {
  if (state.renderingVideoFrame) return;
  const frame = state.pendingVideoFrame;
  if (!frame) return;
  state.pendingVideoFrame = null;
  state.renderingVideoFrame = true;
  try {
    const sentAtMs = Number(frame.timestamp ?? 0) / 1000;
    if (estimateMediaAgeMs(sentAtMs) > state.liveMediaMaxAgeMs) {
      markStaleDrop("Dropping delayed video");
      return;
    }
    await drawFrame(frame);
  } finally {
    frame.close();
    state.renderingVideoFrame = false;
    if (state.pendingVideoFrame) {
      scheduleVideoFrameRender();
    }
  }
}

async function drawFrame(frame) {
  const renderContext = ensureCanvasContext();
  updateVideoSurfaceSize(frame.displayWidth, frame.displayHeight);
  try {
    renderContext.drawImage(frame, 0, 0, canvas.width, canvas.height);
  } catch {
    const bitmap = await createImageBitmap(frame);
    renderContext.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    bitmap.close();
  }
  state.frameCount += 1;
  state.lastVideoFrameRenderedAt = performance.now();
}

function handleFrame(buffer) {
  const view = new DataView(buffer);
  const kind = view.getUint8(0);
  if (kind === 1) {
    handleVideoFrame(buffer, view);
  } else if (kind === 2) {
    handleAudioFrame(buffer, view);
  }
}

function handleVideoFrame(buffer, view) {
  if (!state.decoder || state.decoder.state === "closed") return;
  const key = !!view.getUint8(1);
  const sentAt = Number(view.getBigUint64(2, true));
  const receivedAt = performance.now();
  const seq = view.getUint32(10, true);
  const length = view.getUint32(14, true);
  const bytes = new Uint8Array(buffer, 18, length);
  state.bytesReceived += length;
  state.netWindowBytes += length;
  const deltaSec = (receivedAt - state.lastNetAt) / 1000;
  if (deltaSec >= 0.25) {
    state.netKbps = (state.netWindowBytes * 8) / Math.max(deltaSec, 0.001) / 1000;
    state.netWindowBytes = 0;
    state.lastNetAt = receivedAt;
  }
  const stalledForMs = state.lastVideoPacketAt ? receivedAt - state.lastVideoPacketAt : 0;
  state.lastVideoPacketAt = receivedAt;
  const mediaAgeMs = estimateMediaAgeMs(sentAt);
  if (stalledForMs >= state.mediaStallResetMs || mediaAgeMs > state.liveMediaMaxAgeMs) {
    markStaleDrop("Dropping delayed video");
    resetVideoDecoderForLiveCatchup();
    return;
  }
  if ((state.decoder.decodeQueueSize ?? 0) > state.maxVideoDecodeQueue) {
    markStaleDrop("Video decoder catching up");
    resetVideoDecoderForLiveCatchup();
    return;
  }
  const timestamp = sentAt * 1000;
  if (state.lastVideoSeq !== null && ((state.lastVideoSeq + 1) >>> 0) !== seq) {
    state.lastVideoSeq = seq;
    if (!key) {
      markStaleDrop("Video frame gap");
      resetVideoDecoderForLiveCatchup();
      return;
    }
  } else {
    state.lastVideoSeq = seq;
  }
  if (state.waitingForKeyframe) {
    if (!key) return;
    state.waitingForKeyframe = false;
  }
  try {
    state.decoder.decode(new EncodedVideoChunk({
      type: key ? "key" : "delta",
      timestamp,
      data: bytes,
    }));
  } catch (error) {
    recoverVideoDecoder(error?.message || String(error), { toastCode: "decode_submit_failed" });
  }
}

function handleAudioFrame(buffer, view) {
  if (!state.audioEnabled || !state.audioUserActivated || state.audioMuted) return;
  const sentAt = Number(view.getBigUint64(1, true));
  const receivedAt = performance.now();
  const stalledForMs = state.lastAudioPacketAt ? receivedAt - state.lastAudioPacketAt : 0;
  state.lastAudioPacketAt = receivedAt;
  const mediaAgeMs = estimateMediaAgeMs(sentAt);
  if (mediaAgeMs > state.liveMediaMaxAgeMs) {
    markStaleDrop(`Dropping delayed audio packet ${Math.round(mediaAgeMs)} ms old`);
    return;
  }
  if (stalledForMs >= state.mediaStallResetMs) {
    markStaleDrop("Audio stream resumed after stall");
  }
  const length = view.getUint32(9, true);
  const bytes = new Uint8Array(buffer, 13, length);
  const frame = parseAdtsFrame(bytes);
  if (!frame) return;
  setupAudioDecoder(frame);
  if (!state.audioDecoder || state.audioDecoder.state === "closed") return;
  state.pendingEncodedAudioFrames.push({
    timestamp: sentAt * 1000,
    data: frame.payload,
    durationSeconds: AAC_SAMPLES_PER_FRAME / frame.sampleRate,
  });
  pumpPendingAudioDecode();
}

function parseAdtsFrame(bytes) {
  if (bytes.byteLength < 7) return null;
  if (bytes[0] !== 0xff || (bytes[1] & 0xf0) !== 0xf0) return null;
  const protectionAbsent = bytes[1] & 0x01;
  const profile = ((bytes[2] & 0xc0) >> 6) + 1;
  const sampleRateIndex = (bytes[2] & 0x3c) >> 2;
  const sampleRate = AAC_SAMPLE_RATES[sampleRateIndex];
  const channelConfig = ((bytes[2] & 0x01) << 2) | ((bytes[3] & 0xc0) >> 6);
  const frameLength = ((bytes[3] & 0x03) << 11) | (bytes[4] << 3) | ((bytes[5] & 0xe0) >> 5);
  const headerLength = protectionAbsent ? 7 : 9;
  if (!sampleRate || !channelConfig || frameLength > bytes.byteLength || frameLength <= headerLength) {
    return null;
  }
  const description = new Uint8Array([
    (profile << 3) | (sampleRateIndex >> 1),
    ((sampleRateIndex & 0x01) << 7) | (channelConfig << 3),
  ]);
  return {
    codec: `mp4a.40.${profile}`,
    sampleRate,
    numberOfChannels: channelConfig,
    description,
    payload: bytes.subarray(headerLength, frameLength),
  };
}

function setupAudioDecoder(frame) {
  if (!("AudioDecoder" in window)) {
    showToast("audio_webcodecs_missing", "This browser does not support WebCodecs audio decode");
    return;
  }
  const configKey = `${frame.codec}:${frame.sampleRate}:${frame.numberOfChannels}:${btoa(String.fromCharCode(...frame.description))}`;
  if (state.audioDecoder && state.audioDecoder.state !== "closed" && state.audioConfigKey === configKey) {
    return;
  }
  state.audioDecoder?.close();
  resetAudioPlayback();
  state.audioDecoder = new AudioDecoder({
    output: (audioData) => {
      void playAudioData(audioData);
    },
    error: (err) => showToast("audio_decoder_error", err.message || String(err)),
  });
  state.audioDecoder.configure({
    codec: frame.codec,
    description: frame.description,
    sampleRate: frame.sampleRate,
    numberOfChannels: frame.numberOfChannels,
  });
  state.audioConfigKey = configKey;
}

function resetAudioDecoderForLiveCatchup() {
  state.audioDecoder?.close();
  state.audioDecoder = null;
  state.audioConfigKey = "";
  resetAudioPlayback();
}

function resetAudioPlayback() {
  if (state.audioResumeTimer) {
    clearTimeout(state.audioResumeTimer);
    state.audioResumeTimer = 0;
  }
  state.audioResumeBlockedUntil = 0;
  state.audioUnderrunActive = false;
  state.audioPlaybackBlocked = false;
  state.audioLargeBufferSinceAt = 0;
  state.audioHighLatencySinceAt = 0;
  for (const source of state.audioSources) {
    try {
      source.stop();
    } catch {
      // Source may already be ended.
    }
    source.disconnect();
  }
  state.audioSources.clear();
  state.audioNextTime = 0;
  state.pendingEncodedAudioFrames = [];
  state.pendingAudioBuffers = [];
  state.pendingAudioDuration = 0;
  state.audioDecodingDuration = 0;
  state.audioRateIntegral = 0;
  state.audioRateLastUpdatedAt = 0;
  state.audioClockAutoLastIncreaseAt = 0;
  state.audioClockAutoLastIncreaseLead = 0;
  state.audioClockAutoLastSlowTuneAt = 0;
}

function stopActiveAudioPlayback() {
  for (const source of state.audioSources) {
    try {
      source.stop();
    } catch {
      // Source may already be ended.
    }
    source.disconnect();
  }
  state.audioSources.clear();
  state.audioNextTime = 0;
}

function scheduleAudioResumeCheck(audioContext, delayMs) {
  if (state.audioResumeTimer) return;
  state.audioResumeTimer = window.setTimeout(() => {
    state.audioResumeTimer = 0;
    if (!state.audioContext || state.audioContext.state === "closed") {
      return;
    }
    driveAudioPlayback(audioContext);
  }, Math.max(0, delayMs));
}

function enterAudioUnderrun(audioContext, bufferedSeconds) {
  const wallNow = performance.now();
  const wasUnderrunActive = state.audioUnderrunActive;
  state.audioLargeBufferSinceAt = 0;
  state.audioHighLatencySinceAt = 0;
  state.audioUnderrunActive = true;
  state.audioResumeBlockedUntil = Math.max(
    state.audioResumeBlockedUntil,
    wallNow + AUDIO_UNDERRUN_RETRY_MS,
  );
  if (!wasUnderrunActive) {
    maybeAutoDecreaseAudioClockRate();
  }
  stopActiveAudioPlayback();
  const bufferedMs = Math.max(0, Math.round(bufferedSeconds * 1000));
  setStreamWarning(`${AUDIO_UNDERRUN_WARNING} (${bufferedMs} ms buffered)`);
  scheduleAudioResumeCheck(
    audioContext,
    state.audioResumeBlockedUntil - wallNow,
  );
}

function clearAudioUnderrun() {
  const wasUnderrunActive = state.audioUnderrunActive;
  state.audioUnderrunActive = false;
  state.audioResumeBlockedUntil = 0;
  if (
    state.streamWarning.startsWith(AUDIO_UNDERRUN_WARNING)
    || state.streamWarning.startsWith("Audio rebuffering")
  ) {
    setStreamWarning("");
  }
  if (wasUnderrunActive) {
    pushDebug("audio_buffer", "Audio buffer refilled");
  }
}

function currentConfiguredAudioLatencyMs() {
  return clampControlValue(
    audioLatencyInput,
    audioLatencyInput.value,
    Number(audioLatencyInput.value),
  );
}

function currentConfiguredAudioClockRate() {
  return clampControlValue(
    audioClockRateInput,
    audioClockRateInput.value,
    AUDIO_BASE_PLAYBACK_RATE,
  );
}

function currentConfiguredAudioClockAutoEnabled() {
  return audioClockAutoInput.checked;
}

function currentConfiguredAudioVolumeGain() {
  return clampControlValue(audioVolumeInput, audioVolumeInput.value, 100) / 100;
}

function syncAudioGain() {
  state.audioVolumePercent = clampControlValue(audioVolumeInput, audioVolumeInput.value, 100);
  if (state.audioGainNode) {
    state.audioGainNode.gain.value = currentConfiguredAudioVolumeGain();
  }
}

function currentConfiguredAudioLatencySeconds() {
  return Math.max(AUDIO_MIN_BUFFER_SECONDS, currentConfiguredAudioLatencyMs() / 1000);
}

function oversizedAudioBufferSeconds(profile) {
  return (profile.targetBufferSeconds * 2) + AUDIO_REBUFFER_EXTRA_SECONDS;
}

function trimAudioLatencyLowSeconds(profile) {
  return profile.targetBufferSeconds + AUDIO_TRIM_LATENCY_EXTRA_SECONDS;
}

function audioBufferNeedsCatchup(bufferedSeconds) {
  return bufferedSeconds >= AUDIO_GOOD_LATENCY_BUFFER_SECONDS;
}

function msFromSeconds(seconds) {
  return Math.round(Math.max(0, seconds) * 1000);
}

function audioTrimTargetSeconds(profile) {
  return Math.max(
    profile.minBufferSeconds,
    profile.targetBufferSeconds + AUDIO_TRIM_TARGET_EXTRA_SECONDS,
  );
}

function rebufferOversizedAudioBuffer(profile, bufferedSeconds) {
  state.audioLargeBufferSinceAt = 0;
  state.audioHighLatencySinceAt = 0;
  setConfiguredAudioClockRate(currentConfiguredAudioClockRate() + AUDIO_AUTO_CLOCK_STEP);
  const limitMs = msFromSeconds(oversizedAudioBufferSeconds(profile));
  const goodLatencyMs = msFromSeconds(AUDIO_GOOD_LATENCY_BUFFER_SECONDS);
  const targetMs = msFromSeconds(audioTrimTargetSeconds(profile));
  trimAudioBufferToTarget(
    profile,
    `Audio buffer ${msFromSeconds(bufferedSeconds)} ms > latency*2+300 ms (${limitMs} ms) and >= ${goodLatencyMs} ms, dropping to latency+200 ms (${targetMs} ms)`,
  );
}

function trimPendingEncodedAudioFrames(keepSeconds) {
  const keptFrames = [];
  let keptDuration = 0;
  for (let index = state.pendingEncodedAudioFrames.length - 1; index >= 0; index -= 1) {
    const frame = state.pendingEncodedAudioFrames[index];
    const duration = Number(frame.durationSeconds) || 0;
    if (keptFrames.length > 0 && keptDuration + duration > keepSeconds) {
      continue;
    }
    keptFrames.unshift(frame);
    keptDuration += duration;
  }
  state.pendingEncodedAudioFrames = keptFrames;
  return keptDuration;
}

function trimAudioBufferToTarget(profile, message = "") {
  const keepSeconds = audioTrimTargetSeconds(profile);
  stopActiveAudioPlayback();
  const encodedBudgetSeconds = Math.max(0, keepSeconds - state.audioDecodingDuration);
  const keptEncodedDuration = trimPendingEncodedAudioFrames(encodedBudgetSeconds);
  const decodedBudgetSeconds = Math.max(0, keepSeconds - state.audioDecodingDuration - keptEncodedDuration);
  const keptBuffers = [];
  let keptDuration = 0;
  if (decodedBudgetSeconds > 0) {
    for (let index = state.pendingAudioBuffers.length - 1; index >= 0; index -= 1) {
      const buffer = state.pendingAudioBuffers[index];
      if (keptBuffers.length > 0 && keptDuration + buffer.duration > decodedBudgetSeconds) {
        continue;
      }
      keptBuffers.unshift(buffer);
      keptDuration += buffer.duration;
      if (keptDuration >= decodedBudgetSeconds) {
        break;
      }
    }
  }
  state.pendingAudioBuffers = keptBuffers;
  state.pendingAudioDuration = keptDuration;
  state.audioHighLatencySinceAt = 0;
  state.audioLargeBufferSinceAt = 0;
  state.audioClockAutoLastIncreaseAt = 0;
  state.audioClockAutoLastIncreaseLead = 0;
  state.audioClockAutoLastSlowTuneAt = 0;
  markStaleDrop(message || `Dropping delayed audio buffer to ${msFromSeconds(keepSeconds)} ms`);
}

function setConfiguredAudioClockRate(nextRate) {
  const clamped = clampControlValue(
    audioClockRateInput,
    nextRate,
    currentConfiguredAudioClockRate(),
  );
  if (Math.abs(clamped - currentConfiguredAudioClockRate()) < 0.00005) {
    return false;
  }
  audioClockRateInput.value = clamped.toFixed(4);
  audioClockRateValue.textContent = `${clamped.toFixed(4)}x`;
  persistCurrentSettings();
  return true;
}

function maybeAutoDecreaseAudioClockRate() {
  if (!currentConfiguredAudioClockAutoEnabled()) return;
  state.audioClockAutoLastIncreaseAt = 0;
  state.audioClockAutoLastIncreaseLead = 0;
  state.audioClockAutoLastSlowTuneAt = 0;
  setConfiguredAudioClockRate(currentConfiguredAudioClockRate() - AUDIO_AUTO_CLOCK_STEP);
}

function maybeAutoSlowTuneAudioClockRate(totalAvailableLead, wallNow, profile) {
  if (!currentConfiguredAudioClockAutoEnabled() || state.audioUnderrunActive) {
    state.audioClockAutoLastSlowTuneAt = 0;
    return;
  }
  const slowTuneThreshold = (
    profile.targetBufferSeconds + AUDIO_AUTO_CLOCK_SLOW_TUNE_TARGET_EXTRA_SECONDS
  );
  if (!state.audioClockAutoLastSlowTuneAt) {
    state.audioClockAutoLastSlowTuneAt = wallNow;
    return;
  }
  if (wallNow - state.audioClockAutoLastSlowTuneAt < AUDIO_AUTO_CLOCK_SLOW_TUNE_INTERVAL_MS) {
    return;
  }
  const nextRate = totalAvailableLead > slowTuneThreshold
    ? currentConfiguredAudioClockRate() + AUDIO_AUTO_CLOCK_SLOW_TUNE_STEP
    : currentConfiguredAudioClockRate() - AUDIO_AUTO_CLOCK_SLOW_TUNE_STEP;
  setConfiguredAudioClockRate(nextRate);
  state.audioClockAutoLastSlowTuneAt = wallNow;
}

function currentAudioBufferProfile() {
  const targetBufferSeconds = Math.max(
    AUDIO_MIN_BUFFER_SECONDS,
    currentConfiguredAudioLatencyMs() / 1000,
  );
  return {
    minBufferSeconds: AUDIO_MIN_BUFFER_SECONDS,
    targetBufferSeconds,
    resetGraceSeconds: 0.05,
  };
}

function currentAudioResumeBufferSeconds(profile = currentAudioBufferProfile()) {
  return profile.targetBufferSeconds;
}

function currentAudioStartSlack(audioContext) {
  const baseLatency = Number.isFinite(audioContext?.baseLatency) ? audioContext.baseLatency : 0;
  return Math.max(0.03, Math.min(0.08, Math.max(baseLatency * 3, 0.05)));
}

function currentAudioPlaybackRate(profile, totalAvailableLead) {
  return currentConfiguredAudioClockRate();
}

function scheduleAudioBuffer(
  audioContext,
  audioBuffer,
  playbackRate = 1,
  profile = currentAudioBufferProfile(),
) {
  const source = audioContext.createBufferSource();
  source.buffer = audioBuffer;
  source.playbackRate.value = playbackRate;
  source.connect(state.audioGainNode || audioContext.destination);
  const now = audioContext.currentTime;
  if (!state.audioNextTime || state.audioNextTime < now - profile.resetGraceSeconds) {
    state.audioNextTime = now + currentAudioStartSlack(audioContext);
  }
  state.audioSources.add(source);
  source.start(state.audioNextTime);
  state.audioNextTime += audioBuffer.duration / playbackRate;
  source.onended = () => {
    state.audioSources.delete(source);
    source.disconnect();
    driveAudioPlayback(audioContext);
  };
}

function pumpPendingAudioDecode() {
  const audioDecoder = state.audioDecoder;
  if (!audioDecoder || audioDecoder.state === "closed") return;
  if (state.audioMuted) return;
  if (state.audioPlaybackBlocked) return;
  const profile = currentAudioBufferProfile();
  const decodeTargetSeconds = state.audioUnderrunActive
    ? currentAudioResumeBufferSeconds(profile)
    : oversizedAudioBufferSeconds(profile);
  while (state.pendingEncodedAudioFrames.length > 0) {
    const bufferedSeconds = currentDecodedAudioSeconds();
    if (bufferedSeconds >= decodeTargetSeconds) {
      break;
    }
    if ((audioDecoder.decodeQueueSize ?? 0) >= MAX_AUDIO_DECODE_QUEUE) {
      markStaleDrop("Audio decoder backlog growing");
      break;
    }
    const nextFrame = state.pendingEncodedAudioFrames.shift();
    if (!nextFrame) {
      break;
    }
    try {
      audioDecoder.decode(new EncodedAudioChunk({
        type: "key",
        timestamp: nextFrame.timestamp,
        data: nextFrame.data,
      }));
      state.audioDecodingDuration += nextFrame.durationSeconds;
    } catch (error) {
      state.pendingEncodedAudioFrames.unshift(nextFrame);
      showToast("audio_decode_submit_failed", error.message || String(error));
      break;
    }
  }
}

function driveAudioPlayback(audioContext = state.audioContext) {
  if (!audioContext || audioContext.state === "closed") return;
  if (state.audioMuted) return;
  pumpPendingAudioDecode();
  flushPendingAudioPlayback(audioContext);
  pumpPendingAudioDecode();
  flushPendingAudioPlayback(audioContext);
}

function flushPendingAudioPlayback(audioContext) {
  const profile = currentAudioBufferProfile();
  while (true) {
    const now = audioContext.currentTime;
    const wallNow = performance.now();
    const queuedFor = state.audioNextTime > now ? state.audioNextTime - now : 0;
    const decodedAvailableLead = queuedFor + state.pendingAudioDuration + state.audioDecodingDuration;
    const totalAvailableLead = decodedAvailableLead + pendingEncodedAudioSeconds();
    const playbackStale = !state.audioNextTime || state.audioNextTime < now - profile.resetGraceSeconds;
    const needsCatchup = audioBufferNeedsCatchup(totalAvailableLead);
    const oversizedBufferSeconds = oversizedAudioBufferSeconds(profile);
    if (needsCatchup && totalAvailableLead > oversizedBufferSeconds) {
      if (!state.audioLargeBufferSinceAt) {
        state.audioLargeBufferSinceAt = wallNow;
      } else if (wallNow - state.audioLargeBufferSinceAt >= AUDIO_REBUFFER_HOLD_MS) {
        rebufferOversizedAudioBuffer(profile, totalAvailableLead);
        break;
      }
    } else {
      state.audioLargeBufferSinceAt = 0;
    }
    const trimInBand = needsCatchup
      && totalAvailableLead > trimAudioLatencyLowSeconds(profile)
      && totalAvailableLead <= oversizedBufferSeconds;
    if (trimInBand) {
      if (!state.audioHighLatencySinceAt) {
        state.audioHighLatencySinceAt = wallNow;
      } else if (wallNow - state.audioHighLatencySinceAt >= AUDIO_TRIM_LATENCY_HOLD_MS) {
        const limitMs = msFromSeconds(trimAudioLatencyLowSeconds(profile));
        const goodLatencyMs = msFromSeconds(AUDIO_GOOD_LATENCY_BUFFER_SECONDS);
        const targetMs = msFromSeconds(audioTrimTargetSeconds(profile));
        trimAudioBufferToTarget(
          profile,
          `Audio buffer ${msFromSeconds(totalAvailableLead)} ms > latency+300 ms (${limitMs} ms) and >= ${goodLatencyMs} ms, dropping to latency+200 ms (${targetMs} ms)`,
        );
        break;
      }
    } else {
      state.audioHighLatencySinceAt = 0;
    }
    if (decodedAvailableLead < profile.minBufferSeconds) {
      enterAudioUnderrun(audioContext, decodedAvailableLead);
      break;
    }
    if (state.audioUnderrunActive) {
      const resumeBufferSeconds = currentAudioResumeBufferSeconds(profile);
      if (wallNow < state.audioResumeBlockedUntil || decodedAvailableLead < resumeBufferSeconds) {
        const bufferedMs = Math.max(0, Math.round(decodedAvailableLead * 1000));
        const resumeMs = Math.max(0, Math.round(resumeBufferSeconds * 1000));
        setStreamWarning(`Audio rebuffering (${bufferedMs} / ${resumeMs} ms)`);
        const nextDelayMs = wallNow < state.audioResumeBlockedUntil
          ? state.audioResumeBlockedUntil - wallNow
          : AUDIO_UNDERRUN_POLL_MS;
        scheduleAudioResumeCheck(audioContext, nextDelayMs);
        break;
      }
      clearAudioUnderrun();
    }
    if (
      needsCatchup
      && !trimInBand
      && totalAvailableLead <= trimAudioLatencyLowSeconds(profile)
    ) {
      maybeAutoSlowTuneAudioClockRate(totalAvailableLead, wallNow, profile);
    }
    if (playbackStale && decodedAvailableLead < profile.targetBufferSeconds) {
      break;
    }
    if (!playbackStale && queuedFor >= profile.targetBufferSeconds) {
      break;
    }
    const nextBuffer = state.pendingAudioBuffers.shift();
    if (!nextBuffer) {
      break;
    }
    const playbackRate = currentAudioPlaybackRate(profile, totalAvailableLead);
    state.pendingAudioDuration = Math.max(0, state.pendingAudioDuration - nextBuffer.duration);
    scheduleAudioBuffer(audioContext, nextBuffer, playbackRate, profile);
  }
}

async function ensureAudioContext() {
  if (!window.AudioContext) return null;
  if (!state.audioContext || state.audioContext.state === "closed") {
    state.audioContext = new AudioContext({ latencyHint: "balanced", sampleRate: 48000 });
    state.audioGainNode = state.audioContext.createGain();
    state.audioGainNode.connect(state.audioContext.destination);
    syncAudioGain();
  } else if (!state.audioGainNode) {
    state.audioGainNode = state.audioContext.createGain();
    state.audioGainNode.connect(state.audioContext.destination);
    syncAudioGain();
  }
  if (state.audioContext.state === "suspended") {
    await state.audioContext.resume();
  }
  return state.audioContext;
}

async function primeAudioPlayback() {
  if (state.audioMuted) {
    renderAudioToggle();
    return;
  }
  const wasBlocked = state.audioPlaybackBlocked;
  try {
    await ensureAudioContext();
    if (!state.audioUserActivated) {
      state.audioUserActivated = true;
      resetAudioDecoderForLiveCatchup();
    }
    if (wasBlocked) {
      resetAudioDecoderForLiveCatchup();
    }
    state.audioPlaybackBlocked = false;
  } catch {
    // Autoplay policy may require a user gesture; playback will retry later.
    state.audioPlaybackBlocked = true;
  } finally {
    renderAudioToggle();
  }
}

function currentConfiguredMicBitrateBps() {
  return readSettingsFromControls().micBitrateKbps * 1000;
}

async function startMicrophoneCapture() {
  if (!navigator.mediaDevices?.getUserMedia || !window.MediaRecorder) return;
  if (state.micRecorder || state.micStarting || !state.micEnabled) return;
  state.micStarting = true;
  renderMicToggle();
  try {
    const mimeType = pickMicMimeType();
    if (!mimeType) {
      throw new Error("This browser cannot record microphone uplink as Opus");
    }
    const audioConstraints = {
      channelCount: 1,
      latency: 0.02,
      echoCancellation: true,
      noiseSuppression: true,
      autoGainControl: true,
    };
    if (state.micDeviceId) {
      audioConstraints.deviceId = { exact: state.micDeviceId };
    }
    const stream = await navigator.mediaDevices.getUserMedia({ audio: audioConstraints });
    if (!isConnected()) {
      for (const track of stream.getTracks()) {
        track.stop();
      }
      return;
    }
    stopMicrophoneCapture();
    state.micStreamId += 1;
    const streamId = state.micStreamId >>> 0;
    const recorder = new MediaRecorder(stream, {
      mimeType,
      audioBitsPerSecond: currentConfiguredMicBitrateBps(),
    });
    recorder.ondataavailable = async (event) => {
      if (!isSocketOpen(state.micSocket) || !event.data || event.data.size === 0) return;
      const payload = new Uint8Array(await event.data.arrayBuffer());
      const message = new Uint8Array(MIC_HEADER_BYTES + payload.byteLength);
      const header = new DataView(message.buffer);
      header.setUint8(0, MIC_CHUNK_KIND);
      header.setUint32(1, streamId, true);
      message.set(payload, MIC_HEADER_BYTES);
      sendBinary(message);
    };
    recorder.onerror = () => {
      showToast("mic_record_failed", "Microphone recorder stopped unexpectedly");
      state.micEnabled = false;
      persistCurrentSettings();
      stopMicrophoneCapture();
    };
    recorder.start(MIC_CHUNK_MS);
    state.micRecorder = recorder;
    state.micStream = stream;
    void refreshMicDevices({ silent: true });
  } catch (error) {
    showToast("mic_access_failed", error.message || String(error));
    state.micEnabled = false;
    persistCurrentSettings();
  } finally {
    state.micStarting = false;
    renderMicToggle();
  }
}

function stopMicrophoneCapture() {
  state.micStarting = false;
  if (state.micRecorder) {
    try {
      if (state.micRecorder.state !== "inactive") {
        state.micRecorder.stop();
      }
    } catch {
      // Ignore recorder shutdown errors during reconnect/close.
    }
    state.micRecorder.ondataavailable = null;
    state.micRecorder.onerror = null;
    state.micRecorder = null;
  }
  if (state.micStream) {
    for (const track of state.micStream.getTracks()) {
      track.stop();
    }
    state.micStream = null;
  }
  state.micAudioContext = null;
  state.micSourceNode = null;
  state.micHighpassNode = null;
  state.micLowpassNode = null;
  state.micCompressorNode = null;
  state.micProcessorNode = null;
  state.micSilenceNode = null;
  renderMicToggle();
}

function pickMicMimeType() {
  if (!window.MediaRecorder) return "";
  if (typeof MediaRecorder.isTypeSupported !== "function") {
    return MIC_MIME_CANDIDATES[0];
  }
  return MIC_MIME_CANDIDATES.find((mimeType) => MediaRecorder.isTypeSupported(mimeType)) || "";
}

function pickCameraMimeType() {
  if (!window.MediaRecorder) return "";
  if (typeof MediaRecorder.isTypeSupported !== "function") {
    return CAMERA_MIME_CANDIDATES[0];
  }
  return CAMERA_MIME_CANDIDATES.find((mimeType) => MediaRecorder.isTypeSupported(mimeType)) || "";
}

function cameraUploadExtension(mimeType) {
  const normalized = (mimeType || "").toLowerCase();
  if (normalized.includes("webm")) return "webm";
  if (normalized.includes("mp4")) return "mp4";
  return "bin";
}

function isLocalBrowserOrigin() {
  return ["localhost", "127.0.0.1", "::1"].includes(window.location.hostname);
}

async function uploadCameraChunk(blob, seq) {
  const mimeType = blob.type || state.cameraMimeType;
  const formData = new FormData();
  formData.append("session_id", state.sessionId);
  formData.append("seq", String(seq));
  formData.append("mime_type", mimeType);
  formData.append("file", blob, `camera_${seq}.${cameraUploadExtension(mimeType)}`);
  const response = await fetch(apiUrl("/api/camera/chunk"), {
    method: "POST",
    credentials: "include",
    body: formData,
  });
  if (!response.ok) {
    throw new Error(await response.text().catch(() => "Camera upload failed"));
  }
  return response.json().catch(() => null);
}

function queueCameraChunkUpload(blob) {
  if (!(blob instanceof Blob) || blob.size === 0) {
    return;
  }
  state.cameraSeq += 1;
  const seq = state.cameraSeq;
  state.cameraUploadTail = state.cameraUploadTail
    .catch(() => {})
    .then(() => uploadCameraChunk(blob, seq))
    .catch((error) => {
      showToast("camera_upload_failed", error.message || String(error));
      state.cameraEnabled = false;
      renderCameraToggle();
      const recorder = state.cameraRecorder;
      state.cameraRecorder = null;
      if (recorder && recorder.state !== "inactive") {
        try {
          recorder.stop();
        } catch {
          // Ignore recorder shutdown races after upload failure.
        }
      }
      if (state.cameraStream) {
        for (const track of state.cameraStream.getTracks()) {
          track.stop();
        }
        state.cameraStream = null;
      }
      void notifyCameraStop().catch((stopError) => {
        showToast("camera_stop_failed", stopError.message || String(stopError));
      });
      throw error;
    });
}

async function notifyCameraStop() {
  if (!state.sessionId) return;
  const response = await fetch(apiUrl("/api/camera/stop"), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ session_id: state.sessionId }),
  });
  if (!response.ok) {
    throw new Error(await response.text().catch(() => "Failed to stop camera uplink"));
  }
}

async function startCameraCapture() {
  if (!window.isSecureContext && !isLocalBrowserOrigin()) {
    showToast("camera_insecure_context", "Open the client over HTTPS or localhost to allow camera access");
    state.cameraEnabled = false;
    renderCameraToggle();
    return;
  }
  if (!navigator.mediaDevices?.getUserMedia || !window.MediaRecorder) {
    showToast("camera_unavailable", "Camera recording is unavailable in this browser");
    state.cameraEnabled = false;
    renderCameraToggle();
    return;
  }
  if (!state.sessionId) {
    showToast("camera_session_missing", "Wait for the remote session to connect first");
    state.cameraEnabled = false;
    renderCameraToggle();
    return;
  }
  if (state.cameraRecorder || state.cameraStarting || !state.cameraEnabled) {
    return;
  }

  const mimeType = pickCameraMimeType();
  if (!mimeType) {
    showToast("camera_recording_unsupported", "This browser cannot record camera uplink as MP4 or WebM");
    state.cameraEnabled = false;
    renderCameraToggle();
    return;
  }

  state.cameraMimeType = mimeType;
  state.cameraStarting = true;
  renderCameraToggle();
  try {
    const stream = await navigator.mediaDevices.getUserMedia({
      video: {
        width: { ideal: 1280 },
        height: { ideal: 720 },
        frameRate: { ideal: 30, max: 30 },
      },
    });

    if (!state.cameraEnabled) {
      for (const track of stream.getTracks()) {
        track.stop();
      }
      return;
    }

    const recorder = new MediaRecorder(stream, {
      mimeType,
      videoBitsPerSecond: 2_500_000,
    });

    recorder.ondataavailable = (event) => {
      if (event.data && event.data.size > 0) {
        queueCameraChunkUpload(event.data);
      }
    };
    recorder.onerror = () => {
      showToast("camera_record_failed", "Camera recorder stopped unexpectedly");
      state.cameraEnabled = false;
      renderCameraToggle();
      void stopCameraCapture({ notifyServer: true, keepEnabled: false });
    };

    state.cameraSeq = 0;
    state.cameraUploadTail = Promise.resolve();
    state.cameraRecorder = recorder;
    state.cameraStream = stream;
    recorder.start(CAMERA_CHUNK_MS);
  } catch (error) {
    showToast("camera_access_failed", error.message || String(error));
    state.cameraEnabled = false;
    state.cameraMimeType = "";
  } finally {
    state.cameraStarting = false;
    renderCameraToggle();
  }
}

async function stopCameraCapture({ notifyServer = true, keepEnabled = false } = {}) {
  state.cameraStarting = false;
  const recorder = state.cameraRecorder;
  state.cameraRecorder = null;
  if (recorder && recorder.state !== "inactive") {
    await new Promise((resolve) => {
      recorder.addEventListener("stop", resolve, { once: true });
      try {
        recorder.stop();
      } catch {
        resolve();
      }
    });
  }
  if (state.cameraStream) {
    for (const track of state.cameraStream.getTracks()) {
      track.stop();
    }
    state.cameraStream = null;
  }
  const uploadTail = state.cameraUploadTail.catch(() => {});
  state.cameraUploadTail = Promise.resolve();
  await uploadTail;
  state.cameraMimeType = "";
  if (notifyServer) {
    try {
      await notifyCameraStop();
    } catch (error) {
      showToast("camera_stop_failed", error.message || String(error));
    }
  }
  state.cameraEnabled = keepEnabled ? state.cameraEnabled : false;
  renderCameraToggle();
}

async function playAudioData(audioData) {
  const audioDuration = audioData.numberOfFrames / audioData.sampleRate;
  state.audioDecodingDuration = Math.max(0, state.audioDecodingDuration - audioDuration);
  if (state.audioMuted) {
    audioData.close();
    return;
  }
  const audioContext = await ensureAudioContext().catch(() => null);
  if (!audioContext) {
    state.audioPlaybackBlocked = true;
    renderAudioToggle();
    audioData.close();
    return;
  }
  const audioBuffer = audioContext.createBuffer(
    audioData.numberOfChannels,
    audioData.numberOfFrames,
    audioData.sampleRate,
  );
  for (let channel = 0; channel < audioData.numberOfChannels; channel += 1) {
    audioData.copyTo(audioBuffer.getChannelData(channel), {
      planeIndex: channel,
      format: "f32-planar",
    });
  }
  state.audioPlaybackBlocked = false;
  renderAudioToggle();
  state.pendingAudioBuffers.push(audioBuffer);
  state.pendingAudioDuration += audioBuffer.duration;
  driveAudioPlayback(audioContext);
  audioData.close();
}

function sendNow(message) {
  const socket = shouldUseInputSocket(message) && isInputConnected()
    ? state.inputSocket
    : state.socket;
  if (!isSocketOpen(socket)) return;
  noteAutoDisconnectActivity(message);
  socket.send(JSON.stringify(message));
}

function shouldUseInputSocket(message) {
  return INPUT_SOCKET_MESSAGE_TYPES.has(message?.type);
}

function clearPendingPointerMotion() {
  if (state.pointerFlushTimer) {
    clearTimeout(state.pointerFlushTimer);
    state.pointerFlushTimer = 0;
  }
  state.pendingPointerMotion = null;
}

function flushPendingPointerMotion() {
  if (state.pointerFlushTimer) {
    clearTimeout(state.pointerFlushTimer);
    state.pointerFlushTimer = 0;
  }
  const message = state.pendingPointerMotion;
  state.pendingPointerMotion = null;
  if (message) {
    sendNow(message);
  }
}

function schedulePointerMotionFlush() {
  if (state.pointerFlushTimer) return;
  state.pointerFlushTimer = setTimeout(() => {
    state.pointerFlushTimer = 0;
    flushPendingPointerMotion();
  }, POINTER_FLUSH_INTERVAL_MS);
}

function send(message) {
  flushPendingPointerMotion();
  sendNow(message);
}

function sendBinary(bytes) {
  if (isSocketOpen(state.micSocket)) {
    state.micSocket.send(bytes);
  }
}

function setServerAudioMuted(muted) {
  const nextMuted = muted === true;
  if (state.audioMuted === nextMuted) {
    if (!nextMuted) {
      void primeAudioPlayback();
    }
    renderAudioToggle();
    renderAudioBufferMetric();
    return;
  }
  state.audioMuted = nextMuted;
  if (audioMuteInput) {
    audioMuteInput.checked = nextMuted;
  }
  resetAudioDecoderForLiveCatchup();
  if (!state.audioMuted) {
    void primeAudioPlayback();
  }
  renderAudioToggle();
  renderAudioBufferMetric();
  persistCurrentSettings();
}

function toggleCamera() {
  if (state.cameraEnabled) {
    void stopCameraCapture({ notifyServer: true, keepEnabled: false });
    return;
  }
  state.cameraEnabled = true;
  renderCameraToggle();
  void startCameraCapture();
}

function logInputState(kind, event, extra = {}) {
  console.debug("[input]", {
    kind,
    key: event.key,
    code: event.code,
    repeat: event.repeat,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    altKey: event.altKey,
    shiftKey: event.shiftKey,
    pressedKeys: [...state.pressedKeys],
    ...extra,
  });
}

function requestRemoteClipboard() {
  send({ type: "clipboard_get" });
}

function sendPressedKeyState() {
  if (state.pressedKeys.size === 0) return;
  send({ type: "key_state", pressed_keys: [...state.pressedKeys] });
}

function startKeyStateSync() {
  stopKeyStateSync();
  sendPressedKeyState();
  state.keyStateSyncTimer = setInterval(() => {
    sendPressedKeyState();
  }, KEY_STATE_SYNC_INTERVAL_MS);
}

function stopKeyStateSync() {
  clearInterval(state.keyStateSyncTimer);
  state.keyStateSyncTimer = 0;
}

function sendPasteClipboard(payload) {
  send({ type: "paste_clipboard", payload });
}

function startPing() {
  clearInterval(startPing.timer);
  sendLatencyProbe();
  startPing.timer = setInterval(() => {
    sendLatencyProbe();
  }, LATENCY_PROBE_INTERVAL_MS);
}

function sendLatencyProbe() {
  if (!isConnected()) return;
  state.latencyProbeSeq += 1;
  const seq = state.latencyProbeSeq;
  state.latencyProbeSentAt.set(seq, performance.now());
  if (state.latencyProbeSentAt.size > 4) {
    const oldestSeq = state.latencyProbeSentAt.keys().next().value;
    if (oldestSeq !== undefined) {
      state.latencyProbeSentAt.delete(oldestSeq);
    }
  }
  send({ type: "ping", seq });
}

function startRemoteClipboardPolling() {
  stopRemoteClipboardPolling();
  requestRemoteClipboard();
  startRemoteClipboardPolling.timer = setInterval(() => {
    requestRemoteClipboard();
  }, 3000);
}

function stopRemoteClipboardPolling() {
  clearInterval(startRemoteClipboardPolling.timer);
}

function resetKeys() {
  for (const key of state.pressedKeys) {
    send({ type: "key", key, down: false });
  }
  state.pressedKeys.clear();
  state.tappedPrintableKeys.clear();
  state.modifierChordKeys.clear();
  sendPressedKeyState();
}

function releasePressedKey(key, event = null) {
  state.modifierChordKeys.delete(key);
  if (!state.pressedKeys.has(key)) return false;
  state.pressedKeys.delete(key);
  sendKeyEvent(key, false, event);
  sendPressedKeyState();
  return true;
}

function keyLogicalModifier(key) {
  if (key === "Control_L" || key === "Control_R") return "Control";
  if (key === "Super_L" || key === "Super_R") return "Meta";
  if (key === "Alt_L" || key === "Alt_R") return "Alt";
  if (key === "Shift_L" || key === "Shift_R") return "Shift";
  return null;
}

function hasShortcutModifier(event) {
  return eventHasShortcutModifier(event);
}

function trackModifierChordKey(event, key) {
  if (keyLogicalModifier(key)) return;
  if (hasShortcutModifier(event)) {
    state.modifierChordKeys.add(key);
    return;
  }
  state.modifierChordKeys.delete(key);
}

function releaseStaleModifierChordKeys(event, { exceptKey = null } = {}) {
  if (state.modifierChordKeys.size === 0 || hasShortcutModifier(event)) return;
  for (const key of [...state.modifierChordKeys]) {
    if (key === exceptKey) continue;
    releasePressedKey(key);
  }
}

function modifierLogicalState(event, modifier) {
  let fallback = false;
  if (modifier === "Control") fallback = !!event.ctrlKey;
  if (modifier === "Meta") fallback = !!event.metaKey;
  if (modifier === "Alt") fallback = !!event.altKey;
  if (modifier === "Shift") fallback = !!event.shiftKey;
  if (typeof event.getModifierState === "function") {
    return !!event.getModifierState(modifier) || fallback;
  }
  return fallback;
}

function releaseModifierKeys(keys) {
  for (const key of keys) {
    releasePressedKey(key);
  }
}

function synchronizeModifierState(event, { pressMissing = false, skipLogical = null } = {}) {
  const modifiers = [
    {
      logical: "Control",
      keys: ["Control_L", "Control_R"],
      fallback: "Control_L",
    },
    {
      logical: "Meta",
      keys: ["Super_L", "Super_R"],
      fallback: "Super_L",
    },
    {
      logical: "Alt",
      keys: ["Alt_L", "Alt_R"],
      fallback: "Alt_L",
    },
    {
      logical: "Shift",
      keys: ["Shift_L", "Shift_R"],
      fallback: "Shift_L",
    },
  ];
  for (const modifier of modifiers) {
    if (!modifierLogicalState(event, modifier.logical)) {
      releaseModifierKeys(modifier.keys);
      continue;
    }
    if (!pressMissing) continue;
    if (modifier.logical === skipLogical) continue;
    if (modifier.keys.some((key) => state.pressedKeys.has(key))) continue;
    state.pressedKeys.add(modifier.fallback);
    sendKeyEvent(modifier.fallback, true, event);
    sendPressedKeyState();
  }
}

function normalizeKey(event) {
  const modifierCodeMap = {
    ShiftLeft: "Shift_L",
    ShiftRight: "Shift_R",
    ControlLeft: "Control_L",
    ControlRight: "Control_R",
    AltLeft: "Alt_L",
    AltRight: "Alt_R",
    MetaLeft: "Super_L",
    MetaRight: "Super_R",
  };
  if (modifierCodeMap[event.code]) return modifierCodeMap[event.code];

  const modifierKeyMap = {
    Shift: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Shift_R" : "Shift_L",
    Control: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Control_R" : "Control_L",
    Ctrl: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Control_R" : "Control_L",
    Alt: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Alt_R" : "Alt_L",
    AltGraph: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Alt_R" : "Alt_L",
    Meta: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Super_R" : "Super_L",
    OS: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Super_R" : "Super_L",
  };
  if (modifierKeyMap[event.key]) return modifierKeyMap[event.key];

  const namedKeyMap = {
    Backspace: "BackSpace",
    Delete: "Delete",
    Enter: event.location === KeyboardEvent.DOM_KEY_LOCATION_NUMPAD ? "KP_Enter" : "Return",
    Escape: "Escape",
    Tab: "Tab",
    " ": "space",
    Spacebar: "space",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Home: "Home",
    End: "End",
    PageUp: "Page_Up",
    PageDown: "Page_Down",
  };
  if (namedKeyMap[event.key]) return namedKeyMap[event.key];

  const codeMap = {
    Backquote: "grave",
    Backslash: "backslash",
    Backspace: "BackSpace",
    BracketLeft: "bracketleft",
    BracketRight: "bracketright",
    CapsLock: "Caps_Lock",
    Comma: "comma",
    Delete: "Delete",
    End: "End",
    Enter: "Return",
    Equal: "equal",
    Escape: "Escape",
    Home: "Home",
    Insert: "Insert",
    Minus: "minus",
    PageDown: "Page_Down",
    PageUp: "Page_Up",
    Period: "period",
    Quote: "apostrophe",
    Semicolon: "semicolon",
    Slash: "slash",
    Space: "space",
    Tab: "Tab",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
  };
  if (codeMap[event.code]) return codeMap[event.code];
  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (/^Numpad[0-9]$/.test(event.code)) return event.code.slice(6);
  if (event.code === "NumpadDecimal") return "KP_Decimal";
  if (event.code === "NumpadEnter") return "KP_Enter";
  if (/^F([1-9]|1[0-2])$/.test(event.key)) return event.key;
  return event.key?.length === 1 ? event.key : null;
}

function isReleaseInputChord(event) {
  return (
    state.inputCaptured
    && event.ctrlKey
    && event.altKey
    && event.shiftKey
    && event.key === "Escape"
  );
}

function shouldHandleKeyboard(event) {
  if (!state.inputCaptured) return false;
  const target = event.target;
  if (target instanceof HTMLElement) {
    if (target === mobileKeyboardInput) return false;
    if (target.closest("#control-panel")) return false;
    if (target.matches("input, select, textarea, button")) return false;
  }
  return true;
}

function syncMobileKeyboardButton() {
  mobileKeyboardTrigger.classList.toggle("is-active", document.activeElement === mobileKeyboardInput);
}

function tapRemoteKey(key) {
  sendKeyEvent(key, true);
  sendKeyEvent(key, false);
}

function keyModifierSnapshot(event) {
  return {
    ctrl: modifierLogicalState(event, "Control"),
    shift: modifierLogicalState(event, "Shift"),
    alt: modifierLogicalState(event, "Alt"),
    meta: modifierLogicalState(event, "Meta"),
  };
}

function modifierLabels(modifiers) {
  const labels = [];
  if (modifiers.ctrl) labels.push("ctrl");
  if (modifiers.shift) labels.push("shift");
  if (modifiers.alt) labels.push("alt");
  if (modifiers.meta) labels.push("meta");
  return labels;
}

function formatKeyEventMessage(key, down, modifiers = {}) {
  const labels = modifierLabels(modifiers);
  return `${down ? "keydown" : "keyup"}: ${[key, ...labels].filter(Boolean).join(" ")}`;
}

function sendKeyEvent(key, down, event = null) {
  const message = { type: "key", key, down };
  if (event) {
    message.modifiers = keyModifierSnapshot(event);
  }
  pushDebug("key_event", formatKeyEventMessage(key, down, message.modifiers || {}));
  send(message);
}

function eventHasActiveModifier(event) {
  const modifiers = keyModifierSnapshot(event);
  return modifiers.ctrl || modifiers.shift || modifiers.alt || modifiers.meta;
}

function eventHasShortcutModifier(event) {
  const modifiers = keyModifierSnapshot(event);
  return modifiers.ctrl || modifiers.alt || modifiers.meta;
}

function shouldTapPrintableKey(event, key) {
  return !event.repeat
    && event.key?.length === 1
    && !eventHasShortcutModifier(event)
    && !keyLogicalModifier(key);
}

function tapRemoteKeyFromEvent(key, event) {
  sendKeyEvent(key, true, event);
  sendKeyEvent(key, false, event);
}

function sendRemoteText(text) {
  if (!text) return;
  send({ type: "text_input", text });
}

function clearMobileKeyboardFlushTimer() {
  clearTimeout(state.mobileKeyboardFlushTimer);
  state.mobileKeyboardFlushTimer = 0;
}

function scheduleMobileKeyboardTextFlush(delayMs = 0, fallbackText = "") {
  clearMobileKeyboardFlushTimer();
  state.mobileKeyboardFlushTimer = setTimeout(() => {
    state.mobileKeyboardComposing = false;
    flushMobileKeyboardText(fallbackText);
  }, delayMs);
}

function flushMobileKeyboardText(fallbackText = "") {
  clearMobileKeyboardFlushTimer();
  const text = mobileKeyboardInput.value || fallbackText;
  if (text) {
    sendRemoteText(text);
  }
  mobileKeyboardInput.value = "";
  if (document.activeElement === mobileKeyboardInput) {
    try {
      mobileKeyboardInput.setSelectionRange(0, 0);
    } catch {
      // Some mobile keyboards do not allow selection changes mid-update.
    }
  }
}

function focusMobileKeyboard() {
  captureInputTarget(mobileKeyboardInput);
  mobileKeyboardInput.value = "";
  mobileKeyboardInput.setSelectionRange(0, 0);
  window.navigator.virtualKeyboard?.show?.();
  syncMobileKeyboardButton();
}

function handleMobileKeyboardBeforeInput(event) {
  if (!state.inputCaptured || document.activeElement !== mobileKeyboardInput) {
    captureInputTarget(mobileKeyboardInput);
  }
  if (event.isComposing || event.inputType?.includes("Composition")) {
    state.mobileKeyboardComposing = true;
    scheduleMobileKeyboardTextFlush(MOBILE_KEYBOARD_COMPOSITION_IDLE_FLUSH_MS, event.data || "");
    return;
  }
  if (state.mobileKeyboardComposing) {
    state.mobileKeyboardComposing = false;
    scheduleMobileKeyboardTextFlush(0, event.data || "");
    return;
  }
  let handled = false;
  switch (event.inputType) {
    case "insertText":
    case "insertFromPaste":
    case "insertReplacementText":
      if (event.data) {
        sendRemoteText(event.data);
        handled = true;
      }
      break;
    case "insertLineBreak":
    case "insertParagraph":
      tapRemoteKey("Return");
      handled = true;
      break;
    case "deleteContentBackward":
      tapRemoteKey("BackSpace");
      handled = true;
      break;
    case "deleteContentForward":
      tapRemoteKey("Delete");
      handled = true;
      break;
    default:
      break;
  }
  if (handled) {
    event.preventDefault();
    mobileKeyboardInput.value = "";
  }
}

function handleMobileKeyboardKeydown(event) {
  if (isReleaseInputChord(event)) {
    event.preventDefault();
    releaseInput();
    return;
  }
  const key = normalizeKey(event);
  if (state.mobileKeyboardComposing || event.isComposing) return;
  if (!key) return;
  if (!shouldHandleMobileKeyboardKeyEvent(event, key)) return;
  event.preventDefault();
  releaseStaleModifierChordKeys(event, { exceptKey: key });
  synchronizeModifierState(event, {
    pressMissing: true,
    skipLogical: keyLogicalModifier(key),
  });
  if (MOBILE_KEYBOARD_SPECIAL_KEYS.has(event.key) && !eventHasShortcutModifier(event) && !keyLogicalModifier(key)) {
    tapRemoteKeyFromEvent(key, event);
    return;
  }
  if (!event.repeat && state.pressedKeys.has(key)) {
    return;
  }
  if (!event.repeat) {
    state.pressedKeys.add(key);
    trackModifierChordKey(event, key);
  }
  sendKeyEvent(key, true, event);
  if (!event.repeat) {
    sendPressedKeyState();
  }
}

function handleMobileKeyboardKeyup(event) {
  const key = normalizeKey(event);
  if (!key || !shouldHandleMobileKeyboardKeyEvent(event, key)) return;
  event.preventDefault();
  const released = releasePressedKey(key, event);
  releaseStaleModifierChordKeys(event, { exceptKey: key });
  synchronizeModifierState(event);
  if (released) {
    return;
  }
  sendKeyEvent(key, false, event);
}

function shouldHandleMobileKeyboardKeyEvent(event, key) {
  if (keyLogicalModifier(key)) return true;
  if (MOBILE_KEYBOARD_SPECIAL_KEYS.has(event.key)) return true;
  if (eventHasShortcutModifier(event)) return true;
  return eventHasActiveModifier(event) && event.key?.length !== 1;
}

function scrollSpeedScale() {
  const value = Number(scrollSpeedInput.value);
  if (!Number.isFinite(value)) return 1;
  return Math.min(Math.max(value * 0.25, 0.1), 5);
}

function sendWheelDelta(deltaX, deltaY, deltaMode = 0, scrollSpeed = scrollSpeedScale()) {
  if (!Number.isFinite(deltaX) || !Number.isFinite(deltaY)) return;
  if (!deltaX && !deltaY) return;
  send({
    type: "pointer_wheel",
    delta_x: deltaX,
    delta_y: deltaY,
    delta_mode: deltaMode,
    scroll_speed: scrollSpeed,
  });
}

function queuePointerMove(x, y) {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return;
  if (state.pendingPointerMotion?.type === "pointer_move") {
    flushPendingPointerMotion();
  }
  state.pendingPointerMotion = {
    type: "pointer_absolute",
    x: Math.round(x),
    y: Math.round(y),
  };
  schedulePointerMotionFlush();
}

function queueRelativePointerMove(dx, dy) {
  if (!Number.isFinite(dx) || !Number.isFinite(dy)) return;
  if (!dx && !dy) return;
  if (state.pendingPointerMotion?.type === "pointer_absolute") {
    flushPendingPointerMotion();
  }
  if (state.pendingPointerMotion?.type === "pointer_move") {
    state.pendingPointerMotion.dx += dx;
    state.pendingPointerMotion.dy += dy;
  } else {
    state.pendingPointerMotion = { type: "pointer_move", dx, dy };
  }
  schedulePointerMotionFlush();
}

function clientPointToCanvas(clientX, clientY) {
  const rect = canvas.getBoundingClientRect();
  if (!rect.width || !rect.height) return null;
  const surfaceWidth = videoSurfaceWidth();
  const surfaceHeight = videoSurfaceHeight();
  if (!Number.isFinite(surfaceWidth) || !Number.isFinite(surfaceHeight)) return null;
  const x = Math.min(Math.max(clientX - rect.left, 0), rect.width);
  const y = Math.min(Math.max(clientY - rect.top, 0), rect.height);
  return {
    x: Math.round((x / rect.width) * surfaceWidth),
    y: Math.round((y / rect.height) * surfaceHeight),
  };
}

function pointerToCanvas(event) {
  return clientPointToCanvas(event.clientX, event.clientY);
}

function queuePointerEvent(event) {
  const point = pointerToCanvas(event);
  if (!point) return;
  queuePointerMove(point.x, point.y);
}

function clientDeltaToRemote(dx, dy) {
  const rect = canvas.getBoundingClientRect();
  if (!rect.width || !rect.height) return null;
  const surfaceWidth = videoSurfaceWidth();
  const surfaceHeight = videoSurfaceHeight();
  if (!Number.isFinite(surfaceWidth) || !Number.isFinite(surfaceHeight)) return null;
  return {
    dx: (dx / rect.width) * surfaceWidth,
    dy: (dy / rect.height) * surfaceHeight,
  };
}

function isTouchPointer(event) {
  return event.pointerType === "touch" || event.pointerType === "pen";
}

function getTouchMode() {
  return touchModeSelect.value;
}

function isSmartTouchMode() {
  return getTouchMode() === "smart_touch";
}

function isDirectTouchScrollEnabled() {
  return getTouchMode() === "direct_touch" && directTouchScrollInput.checked;
}

function clearTouchLongPress() {
  if (state.touchLongPressTimer) {
    clearTimeout(state.touchLongPressTimer);
    state.touchLongPressTimer = 0;
  }
  state.touchLongPressPointerId = null;
}

function resetTouchInteraction() {
  clearTouchLongPress();
  clearSmartTouchTimers();
  releaseSmartTouchDrag();
  state.touchPointers.clear();
  state.touchDragPointerId = null;
  state.touchScrollLastY = null;
  state.touchTwoFingerTap = null;
  state.smartTouchAction = null;
  state.smartTouchScrollLastX = null;
  state.smartTouchScrollLastY = null;
}

function getAverageTouchClientY() {
  if (!state.touchPointers.size) return null;
  let total = 0;
  for (const touch of state.touchPointers.values()) {
    total += touch.clientY;
  }
  return total / state.touchPointers.size;
}

function getTouchCenter() {
  if (!state.touchPointers.size) return null;
  let totalX = 0;
  let totalY = 0;
  for (const touch of state.touchPointers.values()) {
    totalX += touch.clientX;
    totalY += touch.clientY;
  }
  return {
    x: totalX / state.touchPointers.size,
    y: totalY / state.touchPointers.size,
  };
}

function startTwoFingerTapCandidate() {
  const entries = [...state.touchPointers.entries()];
  if (entries.length !== 2) return;
  const center = getTouchCenter();
  if (!center) return;
  const now = performance.now();
  const firstStartedAt = Math.min(...entries.map(([, touch]) => touch.startAt || now));
  state.touchTwoFingerTap = {
    startedAt: now,
    pointerIds: new Set(entries.map(([pointerId]) => pointerId)),
    endedPointerIds: new Set(),
    center,
    cancelled: now - firstStartedAt > TWO_FINGER_TAP_MAX_MS,
  };
}

function updateTwoFingerTapCandidate(pointerId) {
  const candidate = state.touchTwoFingerTap;
  if (!candidate || !candidate.pointerIds.has(pointerId)) return;
  if (state.touchPointers.size !== 2) {
    candidate.cancelled = true;
    return;
  }
  for (const id of candidate.pointerIds) {
    const touch = state.touchPointers.get(id);
    if (!touch) {
      candidate.cancelled = true;
      return;
    }
    const moved = Math.hypot(touch.clientX - touch.startX, touch.clientY - touch.startY);
    if (moved > TWO_FINGER_TAP_MOVE_PX) {
      candidate.cancelled = true;
      return;
    }
  }
}

function isVideoRecoveryNeeded() {
  if (!isConnectionOpen()) return true;
  const now = performance.now();
  if (state.decoderRecovering || state.streamWarning) return true;
  if (state.lastVideoPacketAt && now - state.lastVideoPacketAt > state.mediaStallResetMs) return true;
  if (state.lastVideoFrameRenderedAt && now - state.lastVideoFrameRenderedAt > state.mediaStallResetMs) return true;
  return false;
}

function recoverFromTouchGesture() {
  clearReconnectTimer();
  if (state.connecting) {
    setStatus("Connecting...", { hideAfterMs: 1500 });
    return;
  }
  if (!isConnectionOpen()) {
    setStatus("Reconnecting...");
    void connect();
    return;
  }
  if (isVideoRecoveryNeeded()) {
    forceReconnect("Recovery gesture: reconnecting stream");
    return;
  }
  setStatus("Stream active", { hideAfterMs: 1500 });
}

function registerRecoveryTap(center) {
  const now = performance.now();
  const previous = state.recoveryTapCenter;
  const sameArea = previous
    && Math.hypot(center.x - previous.x, center.y - previous.y) <= RECOVERY_TAP_SAME_AREA_PX;
  if (sameArea && now - state.recoveryTapLastAt <= RECOVERY_TAP_WINDOW_MS) {
    state.recoveryTapCount += 1;
  } else {
    state.recoveryTapCount = 1;
  }
  state.recoveryTapLastAt = now;
  state.recoveryTapCenter = center;
  if (state.recoveryTapCount < 3) return;
  state.recoveryTapCount = 0;
  state.recoveryTapCenter = null;
  recoverFromTouchGesture();
}

function consumeTwoFingerTapEnd(event) {
  const candidate = state.touchTwoFingerTap;
  if (!candidate || !candidate.pointerIds.has(event.pointerId)) return false;
  const touch = state.touchPointers.get(event.pointerId);
  if (!touch || Math.hypot(touch.clientX - touch.startX, touch.clientY - touch.startY) > TWO_FINGER_TAP_MOVE_PX) {
    candidate.cancelled = true;
  }
  if (performance.now() - candidate.startedAt > TWO_FINGER_TAP_MAX_MS) {
    candidate.cancelled = true;
  }
  candidate.endedPointerIds.add(event.pointerId);
  const completed = candidate.endedPointerIds.size >= candidate.pointerIds.size;
  const shouldRecover = completed && !candidate.cancelled;
  const center = candidate.center;
  if (completed) {
    state.touchTwoFingerTap = null;
  }
  if (shouldRecover) {
    registerRecoveryTap(center);
  }
  return true;
}

function updateTouchPointer(event) {
  const touch = state.touchPointers.get(event.pointerId);
  if (!touch) return null;
  const previous = { clientX: touch.clientX, clientY: touch.clientY };
  touch.clientX = event.clientX;
  touch.clientY = event.clientY;
  return previous;
}

function resetRemainingTouchStart() {
  if (state.touchPointers.size !== 1) return;
  const [pointerId, touch] = state.touchPointers.entries().next().value;
  touch.startX = touch.clientX;
  touch.startY = touch.clientY;
  state.touchPointers.set(pointerId, touch);
  if (state.touchDragPointerId === pointerId) return;
  if (isSmartTouchMode()) {
    scheduleSmartTouch(pointerId);
  } else {
    scheduleTouchLongPress(pointerId);
  }
}

function scheduleTouchLongPress(pointerId) {
  clearTouchLongPress();
  if (state.touchPointers.size !== 1) return;
  const touch = state.touchPointers.get(pointerId);
  if (!touch) return;
  state.touchLongPressPointerId = pointerId;
  state.touchLongPressTimer = window.setTimeout(() => {
    state.touchLongPressTimer = 0;
    const activeTouch = state.touchPointers.get(pointerId);
    if (!activeTouch || state.touchPointers.size !== 1) return;
    if (getTouchMode() === "direct_touch") {
      const point = clientPointToCanvas(activeTouch.clientX, activeTouch.clientY);
      if (point) send({ type: "pointer_absolute", x: point.x, y: point.y });
    }
    send({ type: "pointer_button", button: 1, down: true });
    state.touchDragPointerId = pointerId;
  }, TOUCH_LONG_PRESS_MS);
}

function maybeCancelTouchLongPress(pointerId) {
  if (state.touchLongPressPointerId !== pointerId) return;
  const touch = state.touchPointers.get(pointerId);
  if (!touch) {
    clearTouchLongPress();
    return;
  }
  const movedX = touch.clientX - touch.startX;
  const movedY = touch.clientY - touch.startY;
  if (Math.hypot(movedX, movedY) >= TOUCH_MOVE_CANCEL_PX) {
    clearTouchLongPress();
  }
}

function startTouchScroll() {
  clearTouchLongPress();
  clearSmartTouchTimers();
  state.touchScrollLastY = getAverageTouchClientY();
}

function clearSmartTouchTimers() {
  if (state.smartTouchDragTimer) {
    clearTimeout(state.smartTouchDragTimer);
    state.smartTouchDragTimer = 0;
  }
  if (state.smartTouchRightClickTimer) {
    clearTimeout(state.smartTouchRightClickTimer);
    state.smartTouchRightClickTimer = 0;
  }
}

function smartTouchMovedDistance(touch) {
  if (!touch) return TOUCH_MOVE_CANCEL_PX;
  return Math.hypot(touch.clientX - touch.startX, touch.clientY - touch.startY);
}

function smartTouchPoint(touch) {
  if (!touch) return null;
  return clientPointToCanvas(touch.clientX, touch.clientY);
}

function movePointerToTouch(touch) {
  const point = smartTouchPoint(touch);
  if (!point) return false;
  send({ type: "pointer_absolute", x: point.x, y: point.y });
  return true;
}

function releaseSmartTouchDrag() {
  if (state.touchDragPointerId === null || state.smartTouchAction !== "drag") return;
  send({ type: "pointer_button", button: 1, down: false });
  state.touchDragPointerId = null;
}

function startSmartTouchDrag(pointerId) {
  if (!isSmartTouchMode() || state.touchPointers.size !== 1 || state.smartTouchAction) return;
  const touch = state.touchPointers.get(pointerId);
  if (!touch || smartTouchMovedDistance(touch) >= TOUCH_MOVE_CANCEL_PX) return;
  clearTouchLongPress();
  movePointerToTouch(touch);
  send({ type: "pointer_button", button: 1, down: true });
  state.touchDragPointerId = pointerId;
  state.smartTouchAction = "drag";
}

function performSmartTouchRightClick(pointerId) {
  if (!isSmartTouchMode() || state.touchPointers.size !== 1) return;
  const touch = state.touchPointers.get(pointerId);
  if (!touch || smartTouchMovedDistance(touch) >= TOUCH_MOVE_CANCEL_PX) return;
  if (state.smartTouchAction === "drag") {
    send({ type: "pointer_button", button: 1, down: false });
  }
  clearSmartTouchTimers();
  state.touchDragPointerId = null;
  state.smartTouchAction = "right_click";
  movePointerToTouch(touch);
  send({ type: "pointer_button", button: 3, down: true });
  send({ type: "pointer_button", button: 3, down: false });
}

function scheduleSmartTouch(pointerId) {
  clearTouchLongPress();
  clearSmartTouchTimers();
  state.smartTouchAction = null;
  state.smartTouchScrollLastX = null;
  state.smartTouchScrollLastY = null;
  if (state.touchPointers.size !== 1) return;
  state.smartTouchDragTimer = window.setTimeout(() => {
    state.smartTouchDragTimer = 0;
    startSmartTouchDrag(pointerId);
  }, SMART_TOUCH_DRAG_HOLD_MS);
  state.smartTouchRightClickTimer = window.setTimeout(() => {
    state.smartTouchRightClickTimer = 0;
    performSmartTouchRightClick(pointerId);
  }, SMART_TOUCH_RIGHT_CLICK_MS);
}

function handleSmartTouchMove(event, previous) {
  const touch = state.touchPointers.get(event.pointerId);
  if (!touch) return;
  const movedDistance = smartTouchMovedDistance(touch);
  if (state.smartTouchAction === "right_click") {
    return;
  }
  if (state.smartTouchAction === "drag") {
    const point = pointerToCanvas(event);
    if (point) queuePointerMove(point.x, point.y);
    return;
  }
  if (movedDistance < TOUCH_MOVE_CANCEL_PX) {
    return;
  }
  clearSmartTouchTimers();
  state.smartTouchAction = "scroll";
  if (state.smartTouchScrollLastX === null || state.smartTouchScrollLastY === null) {
    // Anchor at the gesture's start point (not the previous event) so the
    // travel that crossed the move threshold is scrolled too — the screen
    // reacts the moment the finger starts moving instead of dropping the
    // first few millimetres of the swipe.
    state.smartTouchScrollLastX = touch.startX;
    state.smartTouchScrollLastY = touch.startY;
  }
  sendWheelDelta(
    (state.smartTouchScrollLastX - event.clientX) * SMART_TOUCH_SCROLL_MULTIPLIER,
    (state.smartTouchScrollLastY - event.clientY) * SMART_TOUCH_SCROLL_MULTIPLIER,
    0,
  );
  state.smartTouchScrollLastX = event.clientX;
  state.smartTouchScrollLastY = event.clientY;
}

function handleTouchPointerDown(event) {
  captureInput();
  state.touchPointers.set(event.pointerId, {
    startX: event.clientX,
    startY: event.clientY,
    clientX: event.clientX,
    clientY: event.clientY,
    startAt: performance.now(),
  });
  canvas.setPointerCapture(event.pointerId);
  if (state.touchPointers.size === 1) {
    if (getTouchMode() === "direct_touch" || isSmartTouchMode()) {
      const point = pointerToCanvas(event);
      if (point) queuePointerMove(point.x, point.y);
    }
    if (isSmartTouchMode()) {
      scheduleSmartTouch(event.pointerId);
    } else {
      scheduleTouchLongPress(event.pointerId);
    }
  } else if (state.touchPointers.size === 2) {
    startTwoFingerTapCandidate();
    clearSmartTouchTimers();
    if (state.smartTouchAction === "drag") {
      releaseSmartTouchDrag();
    }
    state.smartTouchAction = null;
    if (state.touchDragPointerId === null) {
      startTouchScroll();
    } else {
      clearTouchLongPress();
    }
  } else {
    if (state.touchTwoFingerTap) {
      state.touchTwoFingerTap.cancelled = true;
    }
    clearTouchLongPress();
  }
  event.preventDefault();
}

function handleTouchPointerMove(event) {
  const previous = updateTouchPointer(event);
  if (!previous) return;
  updateTwoFingerTapCandidate(event.pointerId);
  if (state.touchTwoFingerTap?.endedPointerIds.size) {
    event.preventDefault();
    return;
  }
  const isDragging = state.touchDragPointerId === event.pointerId;
  if (state.touchPointers.size >= 2) {
    if (state.touchDragPointerId !== null) {
      event.preventDefault();
      return;
    }
    if (state.touchScrollLastY === null) {
      startTouchScroll();
    } else {
      const averageY = getAverageTouchClientY();
      if (averageY !== null) {
        sendWheelDelta(0, averageY - state.touchScrollLastY, 0);
        state.touchScrollLastY = averageY;
      }
    }
    event.preventDefault();
    return;
  }
  maybeCancelTouchLongPress(event.pointerId);
  if (isSmartTouchMode()) {
    handleSmartTouchMove(event, previous);
  } else if (getTouchMode() === "direct_touch") {
    if (isDragging) {
      const point = pointerToCanvas(event);
      if (point) queuePointerMove(point.x, point.y);
    } else if (isDirectTouchScrollEnabled()) {
      const touch = state.touchPointers.get(event.pointerId);
      if (!touch) {
        event.preventDefault();
        return;
      }
      const movedX = touch.clientX - touch.startX;
      const movedY = touch.clientY - touch.startY;
      if (Math.hypot(movedX, movedY) < TOUCH_MOVE_CANCEL_PX) {
        event.preventDefault();
        return;
      }
      clearTouchLongPress();
      // On the first scroll event, measure from where the finger first touched
      // down so the travel that crossed the threshold scrolls immediately;
      // afterwards track frame-to-frame movement.
      const fromY = touch.scrolling ? previous.clientY : touch.startY;
      touch.scrolling = true;
      sendWheelDelta(0, (fromY - event.clientY) * DIRECT_TOUCH_SCROLL_MULTIPLIER, 0);
    } else {
      const point = pointerToCanvas(event);
      if (point) queuePointerMove(point.x, point.y);
    }
  } else {
    const delta = clientDeltaToRemote(event.clientX - previous.clientX, event.clientY - previous.clientY);
    if (delta) queueRelativePointerMove(delta.dx, delta.dy);
  }
  event.preventDefault();
}

function handleTouchPointerEnd(event) {
  if (!state.touchPointers.has(event.pointerId)) return;
  const touch = state.touchPointers.get(event.pointerId);
  const wasSingleTouch = state.touchPointers.size === 1;
  const wasDragging = state.touchDragPointerId === event.pointerId;
  const smartAction = state.smartTouchAction;
  const movedDistance = touch
    ? Math.hypot(touch.clientX - touch.startX, touch.clientY - touch.startY)
    : TOUCH_MOVE_CANCEL_PX;
  const isTap = wasSingleTouch
    && !wasDragging
    && !smartAction
    && movedDistance < TOUCH_MOVE_CANCEL_PX;
  const consumedByTwoFingerTap = consumeTwoFingerTapEnd(event);
  if (state.touchLongPressPointerId === event.pointerId) {
    clearTouchLongPress();
  }
  if (isSmartTouchMode()) {
    clearSmartTouchTimers();
  }
  state.touchPointers.delete(event.pointerId);
  if (wasDragging) {
    send({ type: "pointer_button", button: 1, down: false });
    state.touchDragPointerId = null;
  } else if (!consumedByTwoFingerTap && isTap) {
    if (touch && isSmartTouchMode()) {
      movePointerToTouch(touch);
    }
    send({ type: "pointer_button", button: 1, down: true });
    send({ type: "pointer_button", button: 1, down: false });
  }
  if (isSmartTouchMode() && wasSingleTouch) {
    state.smartTouchAction = null;
    state.smartTouchScrollLastX = null;
    state.smartTouchScrollLastY = null;
  }
  if (state.touchPointers.size >= 2) {
    state.touchScrollLastY = getAverageTouchClientY();
  } else {
    state.touchScrollLastY = null;
  }
  if (state.touchPointers.size === 1 && !consumedByTwoFingerTap) {
    resetRemainingTouchStart();
  } else if (!state.touchPointers.size) {
    clearTouchLongPress();
  }
  event.preventDefault();
}

function normalizeClipboardPayload(payload) {
  return {
    text: typeof payload?.text === "string" && payload.text.length ? payload.text : null,
    image_png_b64: typeof payload?.image_png_b64 === "string" && payload.image_png_b64.length
      ? payload.image_png_b64
      : null,
  };
}

function clipboardSignature(payload) {
  return JSON.stringify(normalizeClipboardPayload(payload));
}

function clipboardPreview(payload) {
  const normalized = normalizeClipboardPayload(payload);
  if (normalized.text) {
    return normalized.text.replace(/\s+/g, " ").trim().slice(0, 120);
  }
  if (normalized.image_png_b64) {
    return "[image]";
  }
  return "Empty";
}

function clipboardLine(payload) {
  const normalized = normalizeClipboardPayload(payload);
  if (normalized.text) {
    const singleLine = normalized.text.replace(/\s+/g, " ").trim();
    if (singleLine.length > 33) {
      return `${singleLine.slice(0, 20)}...${singleLine.slice(-10)}`;
    }
    return singleLine;
  }
  if (normalized.image_png_b64) {
    return "[image]";
  }
  return "Empty";
}

function hasClipboardContent(payload) {
  const normalized = normalizeClipboardPayload(payload);
  return !!(normalized.text || normalized.image_png_b64);
}

function canUseBrowserClipboard() {
  return document.visibilityState === "visible" && document.hasFocus();
}

function isClipboardFocusError(error) {
  const message = error?.message || String(error || "");
  return /document is not focused|document.*focus|not focused/i.test(message);
}

function renderClipboardHistory() {
  clipboardHistoryList.replaceChildren();
  const entries = state.clipboardHistory;
  clipboardHistoryEmpty.classList.toggle("hidden", entries.length > 0);
  if (!entries.length) {
    return;
  }
  const fragment = document.createDocumentFragment();
  for (const entry of entries) {
    const item = document.createElement("div");
    item.className = "clipboard-history-item";

    const side = document.createElement("span");
    side.className = "clipboard-history-side";
    side.textContent = entry.side;

    const image = document.createElement("img");
    image.className = "clipboard-history-image";
    image.alt = `${entry.side} clipboard history preview`;
    if (entry.payload.image_png_b64) {
      image.src = `data:image/png;base64,${entry.payload.image_png_b64}`;
    } else {
      image.classList.add("hidden");
    }

    const text = document.createElement("span");
    text.className = "clipboard-history-text";
    text.textContent = clipboardLine(entry.payload);
    text.title = entry.payload.text || clipboardPreview(entry.payload);
    if (entry.payload.text) {
      text.classList.add("is-copyable");
      text.addEventListener("click", async () => {
        if (!canUseBrowserClipboard()) {
          setStatus("Focus window before copying");
          return;
        }
        try {
          await navigator.clipboard.writeText(entry.payload.text);
          showToast("clipboard_history_copied", "History text copied");
        } catch (error) {
          if (isClipboardFocusError(error)) {
            setStatus("Focus window before copying");
            return;
          }
          showToast("clipboard_history_copy_failed", error.message || String(error));
        }
      });
    }

    item.append(side, image, text);
    fragment.append(item);
  }
  clipboardHistoryList.append(fragment);
}

function pushClipboardHistory(side, payload) {
  const normalized = normalizeClipboardPayload(payload);
  if (!hasClipboardContent(normalized)) {
    return;
  }
  state.clipboardHistory.unshift({ side, payload: normalized });
  if (state.clipboardHistory.length > CLIPBOARD_HISTORY_LIMIT) {
    state.clipboardHistory.length = CLIPBOARD_HISTORY_LIMIT;
  }
  renderClipboardHistory();
  if (!state.authenticated) {
    return;
  }
  void saveClipboardHistory().catch((error) => {
    showToast("clipboard_history_save_failed", error.message || String(error));
  });
}

function flashClipboardCard(side) {
  const card = $(`${side}-clipboard-card`);
  flashClipboardCard.timers ??= {};
  card.classList.remove("is-flashing");
  void card.offsetWidth;
  card.classList.add("is-flashing");
  clearTimeout(flashClipboardCard.timers[side]);
  flashClipboardCard.timers[side] = setTimeout(() => {
    card.classList.remove("is-flashing");
  }, 2000);
}

function updateClipboardState(side, payload, { announce = false } = {}) {
  const normalized = normalizeClipboardPayload(payload);
  const signature = clipboardSignature(normalized);
  const payloadKey = side === "local" ? "localClipboard" : "remoteClipboard";
  const sigKey = side === "local" ? "localClipboardSig" : "remoteClipboardSig";
  const timeKey = side === "local" ? "localClipboardUpdatedAt" : "remoteClipboardUpdatedAt";
  const previousPayload = state[payloadKey];
  const previousSignature = state[sigKey];
  const changed = previousSignature !== signature;

  state[payloadKey] = normalized;
  if (changed) {
    if (previousSignature) {
      pushClipboardHistory(side, previousPayload);
    }
    state[sigKey] = signature;
    state[timeKey] = Date.now();
    if (announce && (normalized.text || normalized.image_png_b64) && previousSignature) {
      flashClipboardCard(side);
    }
  }
  renderClipboardCard(side, normalized);
  return normalized;
}

function renderClipboardCard(side, payload) {
  const textEl = $(`${side}-clipboard-text`);
  const metaEl = $(`${side}-clipboard-meta`);
  const imageEl = $(`${side}-clipboard-image`);
  const hasText = !!payload.text;
  const hasImage = !!payload.image_png_b64;
  const line = clipboardLine(payload);
  textEl.textContent = line;
  textEl.title = line === "Empty" ? "" : (payload.text || (hasImage ? "[image]" : line));
  if (hasImage) {
    imageEl.src = `data:image/png;base64,${payload.image_png_b64}`;
    imageEl.classList.remove("hidden");
  } else {
    imageEl.src = "";
    imageEl.classList.add("hidden");
  }
  if (hasText && hasImage) {
    metaEl.textContent = "Text + image";
  } else if (hasImage) {
    metaEl.textContent = "Image";
  } else if (hasText) {
    metaEl.textContent = "Text";
  } else {
    metaEl.textContent = side === "local" ? "Ready to paste remotely" : "Auto-refresh every 3s";
  }
}

async function readLocalClipboard() {
  if (!canUseBrowserClipboard()) {
    return state.localClipboard;
  }
  const payload = { text: null, image_png_b64: null };
  if (!navigator.clipboard?.read) {
    if (navigator.clipboard?.readText) {
      const text = await navigator.clipboard.readText().catch(() => "");
      if (text) payload.text = text;
    }
    return payload;
  }
  const items = await navigator.clipboard.read();
  for (const item of items) {
    if (!payload.image_png_b64 && item.types.includes("image/png")) {
      const blob = await item.getType("image/png");
      if (blob.size > CLIPBOARD_IMAGE_LIMIT_BYTES) {
        showToast("clipboard_image_too_large", "Clipboard image is too large to paste remotely");
      } else {
        payload.image_png_b64 = await blobToBase64(blob);
      }
    }
    if (!payload.text) {
      const textType = item.types.find((type) => type.startsWith("text/plain"));
      if (textType) {
        const blob = await item.getType(textType);
        payload.text = await blob.text();
      }
    }
  }
  if (!payload.text && navigator.clipboard?.readText) {
    const text = await navigator.clipboard.readText().catch(() => "");
    if (text) payload.text = text;
  }
  return payload;
}

async function refreshLocalClipboard() {
  if (!canUseBrowserClipboard()) {
    return state.localClipboard;
  }
  try {
    return updateClipboardState("local", await readLocalClipboard(), { announce: true });
  } catch (error) {
    if (isClipboardFocusError(error)) {
      return state.localClipboard;
    }
    showToast("local_clipboard_read_failed", error.message || String(error));
    return state.localClipboard;
  }
}

async function writeLocalClipboard(payload) {
  if (!canUseBrowserClipboard()) {
    return false;
  }
  const normalized = normalizeClipboardPayload(payload);
  if (navigator.clipboard?.write && normalized.image_png_b64) {
    const items = {};
    items["image/png"] = base64ToBlob(normalized.image_png_b64, "image/png");
    if (normalized.text) {
      items["text/plain"] = new Blob([normalized.text], { type: "text/plain" });
    }
    await navigator.clipboard.write([new ClipboardItem(items)]);
  } else if (normalized.text && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(normalized.text);
  } else {
    throw new Error("Browser clipboard write is unavailable");
  }
  updateClipboardState("local", normalized, { announce: true });
  return true;
}

async function pasteLocalClipboardToRemote() {
  if (!isConnected()) {
    showToast("paste_unavailable", "Connect to the server first");
    return;
  }
  if (!canUseBrowserClipboard()) {
    setStatus("Focus window before reading clipboard");
    return;
  }
  const payload = await refreshLocalClipboard();
  if (!payload.text && !payload.image_png_b64) {
    showToast("local_clipboard_empty", "Local clipboard is empty");
    return;
  }
  sendPasteClipboard(payload);
  showToast("paste_sent", "Local clipboard pasted on the server");
}

async function copyRemoteClipboardToLocal() {
  if (!state.remoteClipboard.text && !state.remoteClipboard.image_png_b64) {
    requestRemoteClipboard();
    return;
  }
  try {
    const copied = await writeLocalClipboard(state.remoteClipboard);
    if (!copied) {
      setStatus("Focus window before copying");
      return;
    }
    showToast("clipboard_copied", "Remote clipboard copied locally");
  } catch (error) {
    if (isClipboardFocusError(error)) {
      setStatus("Focus window before copying");
      return;
    }
    showToast("local_clipboard_write_failed", error.message || String(error));
  }
}

async function uploadSelectedFile() {
  const file = uploadInput.files?.[0];
  if (!file) return;
  const formData = new FormData();
  formData.append("file", file, file.name);
  try {
    setStatus("Uploading...");
    const response = await fetch(apiUrl("/api/upload"), {
      method: "POST",
      credentials: "include",
      body: formData,
    });
    if (!response.ok) {
      throw new Error(await response.text());
    }
    const body = await response.json();
    setStatus(isConnected() ? "Connected" : "Disconnected");
    showToast("upload_ok", `Saved to ${body.saved_as}`);
  } catch (error) {
    showToast("upload_failed", error.message || String(error));
  } finally {
    uploadInput.value = "";
  }
}

function blobToBase64(blob) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      resolve(result.split(",", 2)[1] || "");
    };
    reader.onerror = () => reject(reader.error || new Error("Failed to read blob"));
    reader.readAsDataURL(blob);
  });
}

function base64ToBlob(base64, type) {
  const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
  return new Blob([bytes], { type });
}

function initControls() {
  try {
    if (window.navigator.virtualKeyboard) {
      window.navigator.virtualKeyboard.overlaysContent = true;
    }
  } catch {
    // Some browsers expose a partial VirtualKeyboard API surface.
  }
  authForm.addEventListener("submit", (event) => {
    event.preventDefault();
    void connect();
  });
  authOriginInput.addEventListener("input", () => {
    state.apiOrigin = normalizeApiOrigin(authOriginInput.value);
    saveApiOrigin(state.apiOrigin);
    authError.textContent = "";
    authError.classList.add("hidden");
  });
  authInput.addEventListener("input", () => {
    authError.textContent = "";
    authError.classList.add("hidden");
  });
  renderCodecOptions();
  renderEncodePreferenceRadioGroup();
  renderTouchModeCheckboxGroup();
  codecSelect.addEventListener("change", () => {
    renderCodecOptions();
    void handleCodecSettingChange();
  });
  encodePreferenceSelect.addEventListener("change", () => {
    renderEncodePreferenceRadioGroup();
    handleEncoderSettingChange();
  });
  bitrateInput.addEventListener("input", persistCurrentSettings);
  bitrateInput.addEventListener("change", persistCurrentSettings);
  audioBitrateSelect.addEventListener("change", persistCurrentSettings);
  micBitrateSelect.addEventListener("change", () => {
    persistCurrentSettings();
    if (!state.micEnabled || !isSocketOpen(state.micSocket)) return;
    stopMicrophoneCapture();
    void startMicrophoneCapture();
  });
  fpsInput.addEventListener("input", persistCurrentSettings);
  fpsInput.addEventListener("change", persistCurrentSettings);
  encoderLatencySelect.addEventListener("change", persistCurrentSettings);
  encoderQualitySelect.addEventListener("change", persistCurrentSettings);
  videoScaleSelect.addEventListener("change", persistCurrentSettings);
  gopMsInput.addEventListener("input", persistCurrentSettings);
  gopMsInput.addEventListener("change", persistCurrentSettings);
  bufferMsInput.addEventListener("input", persistCurrentSettings);
  bufferMsInput.addEventListener("change", persistCurrentSettings);
  staleDropMsInput.addEventListener("input", persistCurrentSettings);
  staleDropMsInput.addEventListener("change", persistCurrentSettings);
  stallResetMsInput.addEventListener("input", persistCurrentSettings);
  stallResetMsInput.addEventListener("change", persistCurrentSettings);
  decodeQueueInput.addEventListener("input", persistCurrentSettings);
  decodeQueueInput.addEventListener("change", persistCurrentSettings);
  performancePresetSelect.addEventListener("change", () => {
    if (performancePresetSelect.value === "custom") {
      syncPerformancePresetSelect();
      return;
    }
    applyPerformancePreset(performancePresetSelect.value);
  });
  scrollSpeedInput.addEventListener("input", persistCurrentSettings);
  scrollSpeedInput.addEventListener("change", persistCurrentSettings);
  audioLatencyInput.addEventListener("input", persistCurrentSettings);
  audioLatencyInput.addEventListener("change", persistCurrentSettings);
  audioVolumeInput.addEventListener("input", persistCurrentSettings);
  audioVolumeInput.addEventListener("change", persistCurrentSettings);
  audioClockRateInput.addEventListener("input", persistCurrentSettings);
  audioClockRateInput.addEventListener("change", persistCurrentSettings);
  audioClockAutoInput.addEventListener("change", () => {
    state.audioClockAutoLastIncreaseAt = 0;
    state.audioClockAutoLastIncreaseLead = 0;
    state.audioClockAutoLastSlowTuneAt = 0;
    persistCurrentSettings();
  });
  audioRealOutputInput.addEventListener("change", () => {
    persistCurrentSettings();
    void setAudioOutputMode(audioRealOutputInput.checked);
  });
  audioMuteInput?.addEventListener("change", () => {
    setServerAudioMuted(audioMuteInput.checked);
  });
  autoDisconnectMinutesInput.addEventListener("input", persistCurrentSettings);
  autoDisconnectMinutesInput.addEventListener("change", persistCurrentSettings);
  touchModeSelect.addEventListener("change", () => {
    renderTouchModeCheckboxGroup();
    persistCurrentSettings();
  });
  directTouchScrollInput.addEventListener("change", persistCurrentSettings);
  for (const button of tabButtons) {
    button.addEventListener("click", () => {
      setActiveTab(button.dataset.tabTarget || "status");
    });
  }
  for (const button of controlMenuTabButtons) {
    button.addEventListener("click", () => {
      openControlCard(button.dataset.controlTab || "status");
    });
  }
  controlTrigger.addEventListener("click", () => {
    if (!controlPanel.open) {
      controlPanel.classList.remove("is-card-open");
    }
  });
  controlQuickTab.addEventListener("click", () => {
    if (controlPanel.open && controlPanel.classList.contains("is-card-open")) {
      controlPanel.open = false;
      return;
    }
    openControlCard(state.lastControlTab);
  });
  errorPanel.addEventListener("toggle", () => {
    if (errorPanel.open) {
      controlPanel.open = false;
      transferPanel.open = false;
      audioPanel.open = false;
      releaseInput();
    }
  });
  for (const button of messageTabButtons) {
    button.addEventListener("click", () => {
      setActiveMessageLevel(button.dataset.messageLevel || "error");
    });
  }
  errorClear.addEventListener("click", clearErrors);
  controlPanel.addEventListener("toggle", () => {
    if (controlPanel.open) {
      closeMicDeviceMenu();
      releaseInput();
    }
    if (controlPanel.open && transferPanel.open) {
      transferPanel.open = false;
    }
    if (controlPanel.open && audioPanel.open) {
      audioPanel.open = false;
    }
    if (controlPanel.open && errorPanel.open) {
      errorPanel.open = false;
    }
    if (!controlPanel.open) {
      controlPanel.classList.remove("is-card-open");
    }
  });
  audioPanel.addEventListener("toggle", () => {
    if (audioPanel.open) {
      controlPanel.open = false;
      transferPanel.open = false;
      errorPanel.open = false;
      closeMicDeviceMenu();
      releaseInput();
      renderAudioBufferMetric();
      void refreshAudioOutputStatus({ silent: true });
    }
    renderAudioToggle();
  });
  transferPanel.addEventListener("toggle", () => {
    if (transferPanel.open) {
      controlPanel.open = false;
      audioPanel.open = false;
      errorPanel.open = false;
      releaseInput();
    }
  });
  webclientsToggle.addEventListener("click", () => {
    setWebclientsManagerOpen(!state.webclientsManagerOpen);
  });
  webclientsCloseOthers.addEventListener("click", () => {
    void closeOtherWebClients();
  });
  protocolToggle?.addEventListener("click", (event) => {
    event.stopPropagation();
    setProtocolMenuOpen(!state.protocolMenuOpen);
  });
  protocolMenu?.addEventListener("click", (event) => {
    const option = event.target.closest(".protocol-option");
    if (!option || option.disabled) return;
    void selectTransport(option.dataset.protocol);
  });
  $("connect").addEventListener("click", () => connect());
  $("disconnect").addEventListener("click", disconnect);
  micToggle.addEventListener("click", toggleMicDeviceMenu);
  micDeviceMenu.addEventListener("click", (event) => event.stopPropagation());
  cameraToggle.addEventListener("click", () => {
    controlPanel.open = false;
    void toggleCamera();
  });
  fullscreenToggle?.addEventListener("click", () => {
    void toggleFullscreen();
  });
  document.addEventListener("fullscreenchange", syncFullscreenToggle);
  uploadAction.addEventListener("click", () => uploadInput.click());
  uploadInput.addEventListener("change", () => {
    void uploadSelectedFile();
  });
  localClipboardSyncBtn.addEventListener("click", () => {
    void pasteLocalClipboardToRemote();
  });
  remoteClipboardSyncBtn.addEventListener("click", () => {
    void copyRemoteClipboardToLocal();
  });
  zoomOutButton.addEventListener("click", () => {
    adjustZoom(-VIEW_ZOOM_STEP_PERCENT);
  });
  zoomInButton.addEventListener("click", () => {
    adjustZoom(VIEW_ZOOM_STEP_PERCENT);
  });
  viewCanvasRenderAction.addEventListener("click", setCanvasDisplayRenderWidth);
  viewCanvasCssAction.addEventListener("click", setCanvasDisplayCurrentCssSize);
  viewRemoteScreenAction.addEventListener("click", setCanvasDisplayRemoteScreenSize);
  viewStreamAction.addEventListener("click", setCanvasDisplayStreamSize);
  viewViewportAction.addEventListener("click", setCanvasDisplayViewportWidth);
  viewWindowAction.addEventListener("click", setCanvasDisplayWindowSize);
  mobileKeyboardTrigger.addEventListener("click", () => {
    if (document.activeElement === mobileKeyboardInput) {
      releaseInput();
      return;
    }
    focusMobileKeyboard();
  });
  mobileKeyboardInput.addEventListener("beforeinput", handleMobileKeyboardBeforeInput);
  mobileKeyboardInput.addEventListener("input", () => {
    if (state.mobileKeyboardComposing) {
      scheduleMobileKeyboardTextFlush(MOBILE_KEYBOARD_COMPOSITION_IDLE_FLUSH_MS);
      return;
    }
    flushMobileKeyboardText();
  });
  mobileKeyboardInput.addEventListener("compositionstart", () => {
    clearMobileKeyboardFlushTimer();
    state.mobileKeyboardComposing = true;
  });
  mobileKeyboardInput.addEventListener("compositionend", (event) => {
    state.mobileKeyboardComposing = false;
    scheduleMobileKeyboardTextFlush(0, event.data || "");
  });
  mobileKeyboardInput.addEventListener("keydown", handleMobileKeyboardKeydown);
  mobileKeyboardInput.addEventListener("keyup", handleMobileKeyboardKeyup);
  mobileKeyboardInput.addEventListener("focus", syncMobileKeyboardButton);
  mobileKeyboardInput.addEventListener("blur", () => {
    state.mobileKeyboardComposing = false;
    clearMobileKeyboardFlushTimer();
    mobileKeyboardInput.value = "";
    syncMobileKeyboardButton();
  });
  for (const element of [viewportCard, canvas]) {
    element.addEventListener("selectstart", (event) => {
      event.preventDefault();
    });
    element.addEventListener("dragstart", (event) => {
      event.preventDefault();
    });
  }
  controlPanel.addEventListener("toggle", () => {
    if (controlPanel.open) {
      releaseInput();
    }
  });
  document.addEventListener("pointerdown", (event) => {
    if (!(event.target instanceof HTMLElement && event.target.closest("#audio-toggle"))) {
      void primeAudioPlayback();
    }
    if (event.target instanceof HTMLElement && event.target.closest("#error-panel, #control-panel, #transfer-panel")) {
      releaseInput();
    }
  });
  canvas.addEventListener("pointerdown", (event) => {
    if (!isTouchPointer(event) && reconnectFromViewport()) {
      event.preventDefault();
      return;
    }
    if (isTouchPointer(event)) {
      logInputState("pointerdown", event, {
        button: event.button,
        pointerType: event.pointerType,
      });
      handleTouchPointerDown(event);
      return;
    }
    const point = pointerToCanvas(event);
    if (!point) return;
    captureInput();
    synchronizeModifierState(event);
    logInputState("pointerdown", event, {
      button: event.button,
      pointerType: event.pointerType,
    });
    queuePointerMove(point.x, point.y);
    if (event.button === 0 || event.button === 2 || event.button === 1) {
      send({ type: "pointer_button", button: event.button + 1, down: true });
      event.preventDefault();
    }
  });
  const mousePointerEventName = "onpointerrawupdate" in window ? "pointerrawupdate" : "pointermove";
  canvas.addEventListener(mousePointerEventName, (event) => {
    if (isTouchPointer(event)) {
      handleTouchPointerMove(event);
      return;
    }
    queuePointerEvent(event);
  });
  canvas.addEventListener("pointerup", (event) => {
    synchronizeModifierState(event);
    if (isTouchPointer(event)) {
      handleTouchPointerEnd(event);
      return;
    }
    if (event.button === 0 || event.button === 2 || event.button === 1) {
      send({ type: "pointer_button", button: event.button + 1, down: false });
      event.preventDefault();
    }
  });
  canvas.addEventListener("pointercancel", (event) => {
    if (isTouchPointer(event)) {
      if (state.touchTwoFingerTap) {
        state.touchTwoFingerTap.cancelled = true;
      }
      handleTouchPointerEnd(event);
    }
  });
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    synchronizeModifierState(event);
    sendWheelDelta(event.deltaX, event.deltaY, event.deltaMode);
  }, { passive: false });
  window.addEventListener("keydown", (event) => {
    if (!(event.target instanceof HTMLElement && event.target.closest("#audio-toggle"))) {
      void primeAudioPlayback();
    }
    if (isReleaseInputChord(event)) {
      event.preventDefault();
      releaseInput();
      return;
    }
    if (!shouldHandleKeyboard(event)) return;
    const key = normalizeKey(event);
    if (!key) return;
    releaseStaleModifierChordKeys(event, { exceptKey: key });
    synchronizeModifierState(event, {
      pressMissing: true,
      skipLogical: keyLogicalModifier(key),
    });
    if (shouldTapPrintableKey(event, key)) {
      state.tappedPrintableKeys.add(key);
      logInputState("keydown-text", event, { normalizedKey: key });
      sendRemoteText(event.key);
      event.preventDefault();
      return;
    }
    if (!event.repeat && state.pressedKeys.has(key)) {
      logInputState("keydown-duplicate", event, { normalizedKey: key });
      event.preventDefault();
      return;
    }
    if (!event.repeat) {
      state.pressedKeys.add(key);
      trackModifierChordKey(event, key);
    }
    logInputState("keydown", event, { normalizedKey: key });
    sendKeyEvent(key, true, event);
    if (!event.repeat) {
      sendPressedKeyState();
    }
    event.preventDefault();
  });
  window.addEventListener("keyup", (event) => {
    const key = normalizeKey(event);
    if (!key) return;
    if (state.tappedPrintableKeys.delete(key)) {
      event.preventDefault();
      return;
    }
    const released = releasePressedKey(key, event);
    releaseStaleModifierChordKeys(event, { exceptKey: key });
    synchronizeModifierState(event);
    if (released) {
      event.preventDefault();
      return;
    }
    if (!shouldHandleKeyboard(event)) return;
    sendKeyEvent(key, false, event);
    event.preventDefault();
  });
  window.addEventListener("focus", () => {
    void refreshLocalClipboard();
    requestRemoteClipboard();
  });
  window.addEventListener("resize", applyCanvasZoom);
  window.visualViewport?.addEventListener("resize", applyCanvasZoom);
  window.addEventListener("blur", releaseInput);
  document.addEventListener("click", (event) => {
    if (!state.micDeviceMenuOpen) return;
    if (micToggle.contains(event.target) || micDeviceMenu.contains(event.target)) return;
    closeMicDeviceMenu();
  });
  document.addEventListener("click", (event) => {
    if (!state.protocolMenuOpen) return;
    if (protocolToggle?.contains(event.target) || protocolMenu?.contains(event.target)) return;
    setProtocolMenuOpen(false);
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      closeMicDeviceMenu();
      setProtocolMenuOpen(false);
    }
  });
  navigator.mediaDevices?.addEventListener?.("devicechange", () => {
    void refreshMicDevices({ silent: true });
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      releaseInput();
      return;
    }
    void refreshLocalClipboard();
    requestRemoteClipboard();
  });
  if ("ResizeObserver" in window) {
    const observer = new ResizeObserver(() => {
      applyCanvasZoom();
    });
    observer.observe(viewportCard);
  }
}

async function registerAppServiceWorker() {
  if (!("serviceWorker" in navigator)) return;
  try {
    if ("caches" in window) {
      const cacheNames = await caches.keys();
      await Promise.all(
        cacheNames
          .filter((cacheName) => cacheName.startsWith("vibe-rdesk-"))
          .map((cacheName) => caches.delete(cacheName)),
      );
    }
    await navigator.serviceWorker.register("/sw.js", { scope: "/" });
  } catch (error) {
    showToast("sw_register_failed", error.message || String(error));
  }
}

updateClipboardState("local", state.localClipboard);
updateClipboardState("remote", state.remoteClipboard);
renderClipboardHistory();
syncMobileKeyboardButton();
setActiveTab("status");
syncFullscreenToggle();
state.transport = loadStoredTransport();
renderProtocolStatus();
applySettings(loadStoredSettings());
applyCanvasZoom();
resetStatusMetrics();
initVideoRenderer();
initControls();
void registerAppServiceWorker();
renderMicToggle();
renderAudioToggle();
renderCameraToggle();
void refreshMicDevices({ silent: true });
setInterval(renderAudioBufferMetric, 1000);
setInterval(monitorConnectionHealth, HEALTH_WATCHDOG_INTERVAL_MS);
setInterval(refreshWebClients, 3000);
state.apiOrigin = loadStoredApiOrigin();
authOriginInput.value = state.apiOrigin;
state.sessionPasswd = loadStoredPasswd();
authInput.value = state.sessionPasswd;

async function bootstrapAuth() {
  state.authenticated = await probeAuth();
  if (state.authenticated) {
    clearAuthPrompt();
    void connect();
  } else {
    setAuthPrompt();
  }
}

void bootstrapAuth();
