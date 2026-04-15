const $ = (id) => document.getElementById(id);
const SETTINGS_STORAGE_KEY = "vibe_rdesk.settings";
const API_ORIGIN_STORAGE_KEY = "vibe_rdesk.api_origin";
const PASSWD_STORAGE_KEY = "vibe_rdesk.passwd";
const TOUCH_LONG_PRESS_MS = 1000;
const TOUCH_MOVE_CANCEL_PX = 14;
const DIRECT_TOUCH_SCROLL_MULTIPLIER = 3;
const CLIPBOARD_HISTORY_LIMIT = 100;
const VIEW_ZOOM_STEP_PERCENT = 10;
const VIEW_ZOOM_MIN_PERCENT = 10;
const VIEW_ZOOM_MAX_PERCENT = 300;
const LATENCY_PROBE_INTERVAL_MS = 3000;
const LIVE_MEDIA_MAX_AGE_MS = 1000;
const MEDIA_STALL_RESET_MS = 1000;
const HIGH_LATENCY_RECONNECT_MS = 1500;
const HIGH_LATENCY_RECONNECT_GRACE_MS = 5000;
const HEALTH_WATCHDOG_INTERVAL_MS = 1000;
const ENABLE_VIDEO_RENDER_WORKER = false;
const KEY_STATE_SYNC_INTERVAL_MS = 500;
const MAX_VIDEO_DECODE_QUEUE = 4;
const MAX_AUDIO_DECODE_QUEUE = 24;
const AUDIO_MIN_BUFFER_SECONDS = 0.05;
const AUDIO_UNDERRUN_RETRY_MS = 2000;
const AUDIO_UNDERRUN_RESUME_SECONDS = 2;
const AUDIO_UNDERRUN_POLL_MS = 100;
const AUDIO_REBUFFER_BUFFER_SECONDS = 3;
const AUDIO_REBUFFER_HOLD_MS = 4000;
const AUDIO_TRIM_LATENCY_LOW_BUFFER_SECONDS = 1.9;
const AUDIO_TRIM_LATENCY_BUFFER_SECONDS = 2.1;
const AUDIO_TRIM_LATENCY_HOLD_MS = 20000;
const AUDIO_TRIM_TARGET_EXTRA_SECONDS = 0.2;
const AUDIO_UNDERRUN_WARNING = "Audio buffer too small, pausing playback";
const AAC_SAMPLES_PER_FRAME = 1024;
const AUDIO_BASE_PLAYBACK_RATE = 0.9650;
const AUDIO_AUTO_CLOCK_STEP = 0.005;
const AUDIO_AUTO_CLOCK_INCREASE_BUFFER_SECONDS = 2.4;
const AUDIO_AUTO_CLOCK_INCREASE_INTERVAL_MS = 7000;
const AUDIO_AUTO_CLOCK_INCREASE_MIN_GROWTH_SECONDS = 0.1;
const AUDIO_AUTO_CLOCK_SLOW_TUNE_STEP = 0.001;
const AUDIO_AUTO_CLOCK_SLOW_TUNE_TARGET_EXTRA_SECONDS = 0.2;
const AUDIO_AUTO_CLOCK_SLOW_TUNE_INTERVAL_MS = 30000;
const AUDIO_DRIFT_SLOWDOWN_MAX = 0.05;
const AUDIO_DRIFT_SPEEDUP_MAX = 0.02;
const AUDIO_DRIFT_CORRECTION_DEADZONE_SECONDS = 0.015;
const AUDIO_DRIFT_PROPORTIONAL_GAIN = 0.02;
const AUDIO_DRIFT_INTEGRAL_GAIN = 0.015;
const AUDIO_DRIFT_INTEGRAL_MAX = 0.03;
const AUTO_DISCONNECT_DISABLED_MINUTES = 0;
const AUTO_DISCONNECT_ACTIVITY_REFRESH_MS = 1000;
const SETTINGS_RECONNECT_DELAY_MS = 3000;
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
const state = {
  socket: null,
  decoder: null,
  audioDecoder: null,
  audioContext: null,
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
  micEnabled: false,
  micStarting: false,
  cameraRecorder: null,
  cameraStream: null,
  cameraEnabled: false,
  cameraStarting: false,
  cameraUploadTail: Promise.resolve(),
  cameraSeq: 0,
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
  audioUserActivated: false,
  audioLargeBufferSinceAt: 0,
  audioHighLatencySinceAt: 0,
  audioRateIntegral: 0,
  audioRateLastUpdatedAt: 0,
  audioClockAutoLastIncreaseAt: 0,
  audioClockAutoLastIncreaseLead: 0,
  audioClockAutoLastSlowTuneAt: 0,
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
  touchPointers: new Map(),
  touchLongPressTimer: 0,
  touchLongPressPointerId: null,
  touchDragPointerId: null,
  touchScrollLastY: null,
  inputCaptured: false,
  pressedKeys: new Set(),
  modifierChordKeys: new Set(),
  keyStateSyncTimer: 0,
  wheelAccumulator: 0,
  pendingWheelSteps: 0,
  wheelRaf: 0,
  localClipboard: { text: null, image_png_b64: null },
  remoteClipboard: { text: null, image_png_b64: null },
  localClipboardSig: "",
  remoteClipboardSig: "",
  localClipboardUpdatedAt: 0,
  remoteClipboardUpdatedAt: 0,
  clipboardHistory: [],
  encoderOptionsByCodec: {},
  apiOrigin: window.location.origin,
  authenticated: false,
  sessionPasswd: "",
  connecting: false,
  remoteScreenWidth: null,
  remoteScreenHeight: null,
  viewZoomPercent: 100,
  wsLatencyMs: null,
  latencyProbeSeq: 0,
  latencyProbeSentAt: new Map(),
  serverClockOffsetMs: 0,
  lastVideoPacketAt: 0,
  lastAudioPacketAt: 0,
  streamWarning: "",
  lastStaleDropAt: 0,
  highLatencySinceAt: 0,
  reconnectingForLatency: false,
  appliedStreamSettingsKey: "",
  pendingStreamSettingsKey: "",
  lastLocalStreamSettingsAt: 0,
  statusTimer: null,
  autoDisconnectTimer: null,
  lastAutoDisconnectActivityAt: 0,
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
const mobileKeyboardTrigger = $("mobile-keyboard-trigger");
const micToggle = $("mic-toggle");
const cameraToggle = $("camera-toggle");
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
const audioClockRateInput = $("audio-clock-rate");
const audioClockRateValue = $("audio-clock-rate-value");
const audioClockAutoInput = $("audio-clock-auto");
const autoDisconnectMinutesInput = $("auto-disconnect-minutes");
const touchModeSelect = $("touch-mode");
const directTouchScrollInput = $("direct-touch-scroll");
const directTouchScrollLabel = $("direct-touch-scroll-label");
const uploadAction = $("upload-action");
const uploadInput = $("upload-input");
const tabButtons = Array.from(document.querySelectorAll(".tab-button"));
const tabPanels = Array.from(document.querySelectorAll(".tab-panel"));
const statusCpu = $("status-cpu");
const statusRam = $("status-ram");
const statusSwap = $("status-swap");
const statusLatency = $("status-latency");
const statusSpeedDownload = $("status-speed-download");
const statusSpeedUpload = $("status-speed-upload");
const statusAudioBuffer = $("status-audio-buffer");
const statusUpdatedAt = $("status-updated-at");
const localClipboardSyncBtn = $("local-clipboard-sync-btn");
const remoteClipboardSyncBtn = $("remote-clipboard-sync-btn");
const clipboardHistoryList = $("clipboard-history-list");
const clipboardHistoryEmpty = $("clipboard-history-empty");
const viewVideoSize = $("view-video-size");
const viewViewportSize = $("view-viewport-size");
const viewWindowSize = $("view-window-size");
const viewRemoteScreenSize = $("view-remote-screen-size");
const viewZoomValue = $("view-zoom-value");
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
  "video/mp4;codecs=avc1.42E01E,mp4a.40.2",
  "video/mp4;codecs=avc1,mp4a.40.2",
  "video/mp4",
];
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
      setStatus(state.socket?.readyState === WebSocket.OPEN ? "Connected" : "Disconnected");
    }, hideAfterMs);
  }
}

function setStreamWarning(message = "") {
  state.streamWarning = message;
  streamWarning.classList.toggle("hidden", !message);
  streamWarningText.textContent = message || "Stream delayed";
}

function setEncoderStatus(text) {
  encoderStatus.textContent = text;
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

function currentBufferedAudioSeconds() {
  const audioContext = state.audioContext;
  const now = Number.isFinite(audioContext?.currentTime) ? audioContext.currentTime : 0;
  const queuedFor = state.audioNextTime > now ? state.audioNextTime - now : 0;
  return Math.max(0, queuedFor + state.pendingAudioDuration + state.audioDecodingDuration);
}

function renderAudioBufferMetric() {
  if (!document.getElementById("tab-panel-status")?.classList.contains("is-active")) {
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

function renderViewMetrics() {
  const rect = canvas.getBoundingClientRect();
  viewVideoSize.textContent = formatDimensions(rect.width, rect.height);
  viewViewportSize.textContent = formatDimensions(viewportCard.clientWidth, viewportCard.clientHeight);
  viewWindowSize.textContent = formatDimensions(window.innerWidth, window.innerHeight);
  viewRemoteScreenSize.textContent = formatDimensions(state.remoteScreenWidth, state.remoteScreenHeight);
  viewZoomValue.textContent = `${state.viewZoomPercent}%`;
  syncZoomButtons();
}

function applyCanvasZoom() {
  const remoteWidth = state.remoteScreenWidth ?? canvas.width;
  const remoteHeight = state.remoteScreenHeight ?? canvas.height;
  if (!Number.isFinite(remoteWidth) || !Number.isFinite(remoteHeight) || remoteWidth <= 0 || remoteHeight <= 0) {
    renderViewMetrics();
    return;
  }

  const viewportWidth = Math.max(viewportCard.clientWidth, 1);
  const viewportHeight = Math.max(viewportCard.clientHeight, 1);
  const fitScale = Math.min(viewportWidth / remoteWidth, viewportHeight / remoteHeight);
  const zoomScale = state.viewZoomPercent / 100;
  const displayWidth = Math.max(1, Math.round(remoteWidth * fitScale * zoomScale));
  const displayHeight = Math.max(1, Math.round(remoteHeight * fitScale * zoomScale));

  canvas.style.width = `${displayWidth}px`;
  canvas.style.height = `${displayHeight}px`;
  renderViewMetrics();
}

function adjustZoom(deltaPercent) {
  const nextZoom = clampZoomPercent(state.viewZoomPercent + deltaPercent);
  if (nextZoom === state.viewZoomPercent) {
    syncZoomButtons();
    return;
  }
  state.viewZoomPercent = nextZoom;
  applyCanvasZoom();
  saveSettings();
}

function setActiveTab(tabName) {
  for (const button of tabButtons) {
    const active = button.dataset.tabTarget === tabName;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  }
  for (const panel of tabPanels) {
    const active = panel.id === `tab-panel-${tabName}`;
    panel.classList.toggle("is-active", active);
    panel.hidden = !active;
  }
}

function showToast(code, message) {
  toast.textContent = `${code}: ${message}`;
  toast.dataset.copy = `${code}: ${message}`;
  toast.classList.remove("hidden");
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(() => toast.classList.add("hidden"), 10000);
}

function markStaleDrop(message) {
  state.lastStaleDropAt = performance.now();
  setStreamWarning(message);
}

function forceReconnect(reason) {
  if (state.reconnectingForLatency || state.connecting) return;
  state.reconnectingForLatency = true;
  state.highLatencySinceAt = 0;
  setStreamWarning(reason);
  showToast("stream_reconnect", reason);
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

function syncServerStreamSettings(streamConfig, audioConfig) {
  const incoming = normalizeSettings({
    codec: streamConfig?.codec,
    encodePreference: streamConfig?.encode_preference,
    bitrate: streamConfig?.bitrate_kbps,
    audioBitrateKbps: audioConfig?.bitrate_kbps,
    fps: streamConfig?.fps,
  });
  const current = readSettingsFromControls();
  const currentKey = streamReconnectSettingsKey(current);
  const incomingKey = streamReconnectSettingsKey(incoming);
  if (incomingKey === currentKey) {
    markAppliedStreamSettings(incoming);
    return;
  }
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
  const next = {
    ...current,
    codec: incoming.codec,
    encodePreference: incoming.encodePreference,
    bitrate: incoming.bitrate,
    audioBitrateKbps: incoming.audioBitrateKbps,
    fps: incoming.fps,
  };
  applySettings(next);
  const applied = readSettingsFromControls();
  saveSettings(applied);
  markAppliedStreamSettings(applied);
}

function pushSharedStreamSettings(reason) {
  if (state.connecting) return;
  if (state.socket?.readyState !== WebSocket.OPEN) return;
  clearSettingsReconnectTimer();
  const settings = readSettingsFromControls();
  send({
    type: "update_stream_settings",
    config: {
      codec: settings.codec,
      encode_preference: settings.encodePreference,
      bitrate_kbps: settings.bitrate,
      fps: settings.fps,
    },
    audio_config: {
      bitrate_kbps: settings.audioBitrateKbps,
    },
  });
  setStatus(reason, { hideAfterMs: 3000 });
}

function maybeScheduleSettingsReconnect(settings = readSettingsFromControls()) {
  const socketOpen = state.socket?.readyState === WebSocket.OPEN;
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
    if (state.socket?.readyState !== WebSocket.OPEN || state.connecting) return;
    const latestSettings = readSettingsFromControls();
    if (streamReconnectSettingsKey(latestSettings) === state.appliedStreamSettingsKey) return;
    pushSharedStreamSettings("Applying shared stream settings...");
  }, SETTINGS_RECONNECT_DELAY_MS);
}

function monitorConnectionHealth() {
  const now = performance.now();
  const socketOpen = state.socket?.readyState === WebSocket.OPEN;

  let warning = "";
  if (socketOpen && state.audioUnderrunActive) {
    warning = state.streamWarning || AUDIO_UNDERRUN_WARNING;
  } else if (socketOpen && state.lastVideoPacketAt && now - state.lastVideoPacketAt > MEDIA_STALL_RESET_MS) {
    warning = "Video stalled";
  } else if (socketOpen && state.lastStaleDropAt && now - state.lastStaleDropAt < 3000) {
    warning = state.streamWarning || "Dropping delayed frames";
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
  const defaultCodec = codecSelect.options[0]?.value || "h264";
  const defaultBitrate = Number(bitrateInput.value);
  const defaultAudioBitrate = Number(audioBitrateSelect.value);
  const defaultMicBitrate = Number(micBitrateSelect.value);
  const defaultFps = Number(fpsInput.value);
  const defaultScrollSpeed = Number(scrollSpeedInput.value);
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
    audioLatencyMs: clampControlValue(audioLatencyInput, settings.audioLatencyMs, Number(audioLatencyInput.value)),
    audioClockRate: clampControlValue(audioClockRateInput, settings.audioClockRate, defaultAudioClockRate),
    audioClockAuto: settings.audioClockAuto !== false,
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
    audioClockRate: audioClockRateInput.value,
    audioClockAuto: audioClockAutoInput.checked,
    autoDisconnectMinutes: autoDisconnectMinutesInput.value,
    touchMode: touchModeSelect.value,
    directTouchScroll: directTouchScrollInput.checked,
    micEnabled: state.micEnabled,
    viewZoomPercent: state.viewZoomPercent,
  });
}

function saveSettings(settings = readSettingsFromControls()) {
  try {
    localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // Ignore storage failures; the session still works for this visit.
  }
}

function loadStoredSettings() {
  try {
    const raw = localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) return readSettingsFromControls();
    return normalizeSettings(JSON.parse(raw));
  } catch {
    return readSettingsFromControls();
  }
}

function renderSettingsValues(settings = readSettingsFromControls()) {
  bitrateValue.textContent = `${settings.bitrate} kbps`;
  fpsValue.textContent = `${settings.fps} fps`;
  scrollSpeedValue.textContent = `${settings.scrollSpeed} / 10`;
  audioLatencyValue.textContent = `${settings.audioLatencyMs} ms`;
  audioClockRateValue.textContent = `${Number(settings.audioClockRate).toFixed(4)}x`;
}

function syncTouchModeControls(settings = readSettingsFromControls()) {
  const isDirectTouch = settings.touchMode === "direct_touch";
  directTouchScrollInput.disabled = !isDirectTouch;
  directTouchScrollLabel.classList.toggle("is-disabled", !isDirectTouch);
}

function renderMicToggle() {
  micToggle.classList.toggle("is-active", state.micEnabled);
  micToggle.classList.toggle("is-pending", state.micStarting);
  micToggle.setAttribute("aria-pressed", state.micEnabled ? "true" : "false");
  micToggle.setAttribute("aria-label", state.micEnabled ? "Disable microphone" : "Enable microphone");
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
  audioClockRateInput.value = String(normalized.audioClockRate);
  audioClockAutoInput.checked = normalized.audioClockAuto;
  autoDisconnectMinutesInput.value = String(normalized.autoDisconnectMinutes);
  touchModeSelect.value = normalized.touchMode;
  directTouchScrollInput.checked = normalized.directTouchScroll;
  state.micEnabled = normalized.micEnabled;
  state.viewZoomPercent = normalized.viewZoomPercent;
  renderSettingsValues(normalized);
  syncTouchModeControls(normalized);
  renderMicToggle();
  applyCanvasZoom();
}

function persistResolvedSettings(settings, { scheduleReconnect = true } = {}) {
  renderSettingsValues(settings);
  syncTouchModeControls(settings);
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

function clearAutoDisconnectTimer() {
  if (!state.autoDisconnectTimer) return;
  clearTimeout(state.autoDisconnectTimer);
  state.autoDisconnectTimer = null;
}

function syncAutoDisconnectTimer(settings = readSettingsFromControls()) {
  clearAutoDisconnectTimer();
  if (state.socket?.readyState !== WebSocket.OPEN) return;
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
  if (state.socket?.readyState !== WebSocket.OPEN) return true;
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
    closeConnection({ manual: false, preserveStatus: true });
    state.manualDisconnect = false;
    clearTimeout(state.reconnectTimer);
    void primeAudioPlayback();
    const {
      codec,
      encodePreference,
      bitrate,
      audioBitrateKbps,
      fps,
    } = readSettingsFromControls();
    const videoCodecSupport = await getVideoCodecSupport(codec);
    if (!videoCodecSupport.supported) {
      setStatus("Disconnected");
      setEncoderStatus("Not connected");
      showToast("codec_unsupported", videoCodecSupport.message);
      return;
    }
    state.activeCodec = codec;
    if (videoCodecSupport.codecString) {
      state.codecString = videoCodecSupport.codecString;
    }
    state.frameCount = 0;
    state.bytesReceived = 0;
    state.netWindowBytes = 0;
    state.netKbps = 0;
    state.lastNetAt = performance.now();
    const url = webSocketUrl("/ws");
    url.searchParams.set("codec", codec);
    url.searchParams.set("encode_preference", encodePreference);
    url.searchParams.set("bitrate_kbps", bitrate);
    url.searchParams.set("audio_bitrate_kbps", audioBitrateKbps);
    url.searchParams.set("fps", fps);
    if (state.sessionPasswd) {
      url.searchParams.set("passwd", state.sessionPasswd);
    }
    setStatus("Connecting...");
    setEncoderStatus("Connecting...");
    state.socket = new WebSocket(url);
    state.socket.binaryType = "arraybuffer";
    state.socket.onopen = () => {
      state.appliedStreamSettingsKey = "";
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
    };
    state.socket.onclose = (event) => {
      setStatus("Disconnected");
      stopKeyStateSync();
      clearInterval(startPing.timer);
      stopRemoteClipboardPolling();
      if (!state.manualDisconnect && event.code && event.code !== 1000) {
        showToast("ws_closed", `WebSocket closed (${event.code})`);
      }
      if (!state.manualDisconnect) scheduleReconnect();
    };
    state.socket.onerror = () => showToast("ws_error", "WebSocket error");
    state.socket.onmessage = (event) => {
      if (typeof event.data === "string") {
        handleServerMessage(JSON.parse(event.data));
      } else {
        handleFrame(event.data);
      }
    };
  } catch (error) {
    const message = error.message || String(error);
    if (needsLogin || isAuthFailureMessage(message)) {
      state.authenticated = false;
      showToast("auth_failed", message);
      setAuthPrompt(message || "Authentication failed");
    } else {
      showToast("connect_failed", message);
      if (!state.manualDisconnect) scheduleReconnect();
    }
    setStatus(state.socket?.readyState === WebSocket.OPEN ? "Connected" : "Disconnected");
  } finally {
    state.connecting = false;
  }
}

function closeConnection({ manual = true, preserveStatus = false } = {}) {
  state.manualDisconnect = manual;
  clearTimeout(state.reconnectTimer);
  clearSettingsReconnectTimer();
  clearAutoDisconnectTimer();
  stopKeyStateSync();
  clearInterval(startPing.timer);
  state.latencyProbeSentAt.clear();
  state.lastVideoPacketAt = 0;
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
  void stopCameraCapture({ notifyServer: true, keepEnabled: false });
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
  state.netWindowBytes = 0;
  state.netKbps = 0;
  resetKeys();
  if (state.socket) {
    const socket = state.socket;
    state.socket = null;
    socket.onclose = null;
    socket.close();
  }
  if (!preserveStatus) setEncoderStatus("Not connected");
  if (!preserveStatus) setStatus("Disconnected");
  if (!preserveStatus) resetStatusMetrics();
}

function disconnect() {
  closeConnection({ manual: true });
}

function isConnectionOpen() {
  return state.socket?.readyState === WebSocket.OPEN;
}

function isConnectionDisconnected() {
  return !state.socket || state.socket.readyState === WebSocket.CLOSED;
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
    if (message.session_id) {
      state.sessionId = message.session_id;
    }
    if (message.config) {
      syncServerStreamSettings(message.config, message.audio_config);
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
      state.description = Uint8Array.from(atob(message.description_b64), (c) => c.charCodeAt(0));
    } else if (codecStringChanged) {
      state.description = null;
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
    showToast(message.code, message.message);
  } else if (message.type === "clipboard" && message.side === "remote") {
    updateClipboardState("remote", message.payload, { announce: true });
  }
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
  } catch (error) {
    state.decoder?.close();
    state.decoder = null;
    state.decoderConfigKey = "";
    showToast("decoder_config_failed", error.message || String(error));
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
  state.videoRenderWorker?.postMessage({ type: "clear" });
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
    disableVideoRenderWorker();
    showToast(message.code || "video_worker_error", message.message || "Video worker failed");
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
      disableVideoRenderWorker();
      showToast("video_worker_error", event.message || "Video worker failed");
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
  const remoteSizeChanged = (
    state.remoteScreenWidth !== width
    || state.remoteScreenHeight !== height
  );
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
    state.videoRenderWorker?.postMessage({ type: "resize", width, height });
  }
  if (remoteSizeChanged) {
    state.remoteScreenWidth = width;
    state.remoteScreenHeight = height;
    applyCanvasZoom();
  }
}

function resetVideoDecoderForLiveCatchup() {
  recoverVideoDecoder();
}

function queueVideoFrameForRender(frame) {
  updateVideoSurfaceSize(frame.displayWidth, frame.displayHeight);
  if (state.videoRenderWorker) {
    state.frameCount += 1;
    state.videoRenderWorker.postMessage({ type: "frame", frame }, [frame]);
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
    if (estimateMediaAgeMs(sentAtMs) > LIVE_MEDIA_MAX_AGE_MS) {
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
  const length = view.getUint32(10, true);
  const bytes = new Uint8Array(buffer, 14, length);
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
  if (stalledForMs >= MEDIA_STALL_RESET_MS || mediaAgeMs > LIVE_MEDIA_MAX_AGE_MS) {
    markStaleDrop("Dropping delayed video");
    resetVideoDecoderForLiveCatchup();
    return;
  }
  if ((state.decoder.decodeQueueSize ?? 0) > MAX_VIDEO_DECODE_QUEUE) {
    markStaleDrop("Video decoder catching up");
    resetVideoDecoderForLiveCatchup();
    return;
  }
  const timestamp = sentAt * 1000;
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
  if (!state.audioEnabled || !state.audioUserActivated) return;
  const sentAt = Number(view.getBigUint64(1, true));
  const receivedAt = performance.now();
  const stalledForMs = state.lastAudioPacketAt ? receivedAt - state.lastAudioPacketAt : 0;
  state.lastAudioPacketAt = receivedAt;
  const mediaAgeMs = estimateMediaAgeMs(sentAt);
  if (stalledForMs >= MEDIA_STALL_RESET_MS || mediaAgeMs > LIVE_MEDIA_MAX_AGE_MS) {
    markStaleDrop("Buffered delayed audio");
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
  state.audioUnderrunActive = false;
  state.audioResumeBlockedUntil = 0;
  if (
    state.streamWarning.startsWith(AUDIO_UNDERRUN_WARNING)
    || state.streamWarning.startsWith("Audio rebuffering")
  ) {
    setStreamWarning("");
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

function currentConfiguredAudioLatencySeconds() {
  return Math.max(AUDIO_MIN_BUFFER_SECONDS, currentConfiguredAudioLatencyMs() / 1000);
}

function rebufferOversizedAudioBuffer() {
  state.audioLargeBufferSinceAt = 0;
  state.audioHighLatencySinceAt = 0;
  markStaleDrop("Rebuffering oversized audio buffer");
  resetAudioDecoderForLiveCatchup();
}

function trimAudioBufferToTarget(profile) {
  const keepSeconds = Math.max(
    profile.minBufferSeconds,
    profile.targetBufferSeconds + AUDIO_TRIM_TARGET_EXTRA_SECONDS,
  );
  stopActiveAudioPlayback();
  const keptBuffers = [];
  let keptDuration = 0;
  for (let index = state.pendingAudioBuffers.length - 1; index >= 0; index -= 1) {
    const buffer = state.pendingAudioBuffers[index];
    keptBuffers.unshift(buffer);
    keptDuration += buffer.duration;
    if (keptDuration >= keepSeconds) {
      break;
    }
  }
  state.pendingAudioBuffers = keptBuffers;
  state.pendingAudioDuration = keptDuration;
  state.audioHighLatencySinceAt = 0;
  state.audioLargeBufferSinceAt = 0;
  state.audioClockAutoLastIncreaseAt = 0;
  state.audioClockAutoLastIncreaseLead = 0;
  state.audioClockAutoLastSlowTuneAt = 0;
  markStaleDrop(`Trimming delayed audio to ${Math.round(keepSeconds * 1000)} ms`);
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

function maybeAutoIncreaseAudioClockRate(totalAvailableLead, wallNow) {
  if (!currentConfiguredAudioClockAutoEnabled() || state.audioUnderrunActive) {
    state.audioClockAutoLastIncreaseAt = 0;
    state.audioClockAutoLastIncreaseLead = 0;
    state.audioClockAutoLastSlowTuneAt = 0;
    return;
  }
  if (totalAvailableLead < AUDIO_AUTO_CLOCK_INCREASE_BUFFER_SECONDS) {
    state.audioClockAutoLastIncreaseAt = 0;
    state.audioClockAutoLastIncreaseLead = 0;
    return;
  }
  if (!state.audioClockAutoLastIncreaseAt) {
    state.audioClockAutoLastIncreaseAt = wallNow;
    state.audioClockAutoLastIncreaseLead = totalAvailableLead;
    return;
  }
  if (wallNow - state.audioClockAutoLastIncreaseAt < AUDIO_AUTO_CLOCK_INCREASE_INTERVAL_MS) {
    return;
  }
  const leadGrowth = totalAvailableLead - state.audioClockAutoLastIncreaseLead;
  if (leadGrowth >= AUDIO_AUTO_CLOCK_INCREASE_MIN_GROWTH_SECONDS) {
    setConfiguredAudioClockRate(currentConfiguredAudioClockRate() + AUDIO_AUTO_CLOCK_STEP);
  } else {
    trimAudioBufferToTarget(currentAudioBufferProfile());
  }
  state.audioClockAutoLastIncreaseAt = wallNow;
  state.audioClockAutoLastIncreaseLead = totalAvailableLead;
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
  source.connect(audioContext.destination);
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
  if (state.audioPlaybackBlocked) return;
  const profile = currentAudioBufferProfile();
  const decodeTargetSeconds = state.audioUnderrunActive
    ? currentAudioResumeBufferSeconds(profile)
    : AUDIO_REBUFFER_BUFFER_SECONDS;
  while (state.pendingEncodedAudioFrames.length > 0) {
    const bufferedSeconds = currentBufferedAudioSeconds();
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
    const totalAvailableLead = queuedFor + state.pendingAudioDuration + state.audioDecodingDuration;
    const playbackStale = !state.audioNextTime || state.audioNextTime < now - profile.resetGraceSeconds;
    if (totalAvailableLead > AUDIO_REBUFFER_BUFFER_SECONDS) {
      if (!state.audioLargeBufferSinceAt) {
        state.audioLargeBufferSinceAt = wallNow;
      } else if (wallNow - state.audioLargeBufferSinceAt >= AUDIO_REBUFFER_HOLD_MS) {
        rebufferOversizedAudioBuffer();
        break;
      }
    } else {
      state.audioLargeBufferSinceAt = 0;
    }
    const trimInBand = totalAvailableLead > AUDIO_TRIM_LATENCY_LOW_BUFFER_SECONDS
      && totalAvailableLead < AUDIO_TRIM_LATENCY_BUFFER_SECONDS;
    if (trimInBand) {
      if (!state.audioHighLatencySinceAt) {
        state.audioHighLatencySinceAt = wallNow;
      } else if (wallNow - state.audioHighLatencySinceAt >= AUDIO_TRIM_LATENCY_HOLD_MS) {
        trimAudioBufferToTarget(profile);
        break;
      }
    } else {
      state.audioHighLatencySinceAt = 0;
    }
    if (totalAvailableLead < profile.minBufferSeconds) {
      enterAudioUnderrun(audioContext, totalAvailableLead);
      break;
    }
    if (state.audioUnderrunActive) {
      const resumeBufferSeconds = currentAudioResumeBufferSeconds(profile);
      if (wallNow < state.audioResumeBlockedUntil || totalAvailableLead < resumeBufferSeconds) {
        const bufferedMs = Math.max(0, Math.round(totalAvailableLead * 1000));
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
    maybeAutoIncreaseAudioClockRate(totalAvailableLead, wallNow);
    maybeAutoSlowTuneAudioClockRate(totalAvailableLead, wallNow, profile);
    if (playbackStale && totalAvailableLead < profile.targetBufferSeconds) {
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
  }
  if (state.audioContext.state === "suspended") {
    await state.audioContext.resume();
  }
  return state.audioContext;
}

async function primeAudioPlayback() {
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
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        latency: 0.02,
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
    if (state.socket?.readyState !== WebSocket.OPEN) {
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
      if (state.socket?.readyState !== WebSocket.OPEN || !event.data || event.data.size === 0) return;
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

async function uploadCameraChunk(blob, seq) {
  const formData = new FormData();
  formData.append("session_id", state.sessionId);
  formData.append("seq", String(seq));
  formData.append("file", blob, `camera_${seq}.mp4`);
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
    showToast("camera_mp4_unsupported", "This browser cannot record camera uplink as MP4");
    state.cameraEnabled = false;
    renderCameraToggle();
    return;
  }

  state.cameraStarting = true;
  renderCameraToggle();
  try {
    const stream = await navigator.mediaDevices.getUserMedia({
      video: {
        width: { ideal: 1280 },
        height: { ideal: 720 },
        frameRate: { ideal: 30, max: 30 },
      },
      audio: {
        channelCount: 1,
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
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
      audioBitsPerSecond: 128_000,
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
  const audioContext = await ensureAudioContext().catch(() => null);
  if (!audioContext) {
    state.audioPlaybackBlocked = true;
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
  state.pendingAudioBuffers.push(audioBuffer);
  state.pendingAudioDuration += audioBuffer.duration;
  driveAudioPlayback(audioContext);
  audioData.close();
}

function send(message) {
  if (state.socket?.readyState === WebSocket.OPEN) {
    noteAutoDisconnectActivity(message);
    state.socket.send(JSON.stringify(message));
  }
}

function sendBinary(bytes) {
  if (state.socket?.readyState === WebSocket.OPEN) {
    state.socket.send(bytes);
  }
}

function toggleMicrophone() {
  state.micEnabled = !state.micEnabled;
  persistCurrentSettings();
  if (!state.micEnabled) {
    stopMicrophoneCapture();
    return;
  }
  if (state.socket?.readyState === WebSocket.OPEN) {
    void startMicrophoneCapture();
  } else {
    renderMicToggle();
  }
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
  if (state.socket?.readyState !== WebSocket.OPEN) return;
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
  state.modifierChordKeys.clear();
  sendPressedKeyState();
}

function releasePressedKey(key) {
  state.modifierChordKeys.delete(key);
  if (!state.pressedKeys.has(key)) return false;
  state.pressedKeys.delete(key);
  send({ type: "key", key, down: false });
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
  return modifierLogicalState(event, "Alt") || modifierLogicalState(event, "Meta");
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
  if (typeof event.getModifierState === "function") {
    return event.getModifierState(modifier);
  }
  if (modifier === "Control") return !!event.ctrlKey;
  if (modifier === "Meta") return !!event.metaKey;
  if (modifier === "Alt") return !!event.altKey;
  if (modifier === "Shift") return !!event.shiftKey;
  return false;
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
    send({ type: "key", key: modifier.fallback, down: true });
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
  send({ type: "key", key, down: true });
  send({ type: "key", key, down: false });
}

function sendRemoteText(text) {
  if (!text) return;
  send({ type: "text_input", text });
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
  let handled = false;
  switch (event.inputType) {
    case "insertText":
    case "insertCompositionText":
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
  if (!MOBILE_KEYBOARD_SPECIAL_KEYS.has(event.key)) return;
  const key = normalizeKey(event);
  if (!key) return;
  event.preventDefault();
  tapRemoteKey(key);
}

function flushQueuedWheel() {
  state.wheelRaf = 0;
  const steps = state.pendingWheelSteps;
  state.pendingWheelSteps = 0;
  if (!steps) return;
  const direction = steps > 0 ? 1 : -1;
  for (let i = 0; i < Math.abs(steps); i += 1) {
    send({ type: "pointer_wheel", delta_y: direction });
  }
}

function scheduleWheelFlush() {
  if (state.wheelRaf) return;
  state.wheelRaf = requestAnimationFrame(flushQueuedWheel);
}

function queueWheel(deltaY) {
  const speed = Number($("scroll-speed").value) * 0.25;
  state.wheelAccumulator += deltaY * speed;
  const threshold = 100;
  let steps = 0;
  while (state.wheelAccumulator >= threshold) {
    state.wheelAccumulator -= threshold;
    steps += 1;
  }
  while (state.wheelAccumulator <= -threshold) {
    state.wheelAccumulator += threshold;
    steps -= 1;
  }
  if (!steps) return;
  state.pendingWheelSteps += Math.max(-4, Math.min(4, steps));
  scheduleWheelFlush();
}

function queuePointerMove(x, y) {
  send({ type: "pointer_absolute", x, y });
}

function queueRelativePointerMove(dx, dy) {
  if (!dx && !dy) return;
  send({ type: "pointer_move", dx, dy });
}

function clientPointToCanvas(clientX, clientY) {
  const rect = canvas.getBoundingClientRect();
  if (!rect.width || !rect.height) return null;
  const x = Math.min(Math.max(clientX - rect.left, 0), rect.width);
  const y = Math.min(Math.max(clientY - rect.top, 0), rect.height);
  return {
    x: Math.round((x / rect.width) * canvas.width),
    y: Math.round((y / rect.height) * canvas.height),
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
  return {
    dx: (dx / rect.width) * canvas.width,
    dy: (dy / rect.height) * canvas.height,
  };
}

function isTouchPointer(event) {
  return event.pointerType === "touch" || event.pointerType === "pen";
}

function getTouchMode() {
  return touchModeSelect.value;
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
  state.touchPointers.clear();
  state.touchDragPointerId = null;
  state.touchScrollLastY = null;
  state.pendingWheelSteps = 0;
  state.wheelAccumulator = 0;
  if (state.wheelRaf) {
    cancelAnimationFrame(state.wheelRaf);
    state.wheelRaf = 0;
  }
}

function getAverageTouchClientY() {
  if (!state.touchPointers.size) return null;
  let total = 0;
  for (const touch of state.touchPointers.values()) {
    total += touch.clientY;
  }
  return total / state.touchPointers.size;
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
  scheduleTouchLongPress(pointerId);
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
  state.touchScrollLastY = getAverageTouchClientY();
}

function handleTouchPointerDown(event) {
  captureInput();
  state.touchPointers.set(event.pointerId, {
    startX: event.clientX,
    startY: event.clientY,
    clientX: event.clientX,
    clientY: event.clientY,
  });
  canvas.setPointerCapture(event.pointerId);
  if (state.touchPointers.size === 1) {
    if (getTouchMode() === "direct_touch") {
      const point = pointerToCanvas(event);
      if (point) queuePointerMove(point.x, point.y);
    }
    scheduleTouchLongPress(event.pointerId);
  } else if (state.touchPointers.size === 2) {
    if (state.touchDragPointerId === null) {
      startTouchScroll();
    } else {
      clearTouchLongPress();
    }
  } else {
    clearTouchLongPress();
  }
  event.preventDefault();
}

function handleTouchPointerMove(event) {
  const previous = updateTouchPointer(event);
  if (!previous) return;
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
        queueWheel(averageY - state.touchScrollLastY);
        state.touchScrollLastY = averageY;
      }
    }
    event.preventDefault();
    return;
  }
  maybeCancelTouchLongPress(event.pointerId);
  if (getTouchMode() === "direct_touch") {
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
      queueWheel((previous.clientY - event.clientY) * DIRECT_TOUCH_SCROLL_MULTIPLIER);
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
  const movedDistance = touch
    ? Math.hypot(touch.clientX - touch.startX, touch.clientY - touch.startY)
    : TOUCH_MOVE_CANCEL_PX;
  const isTap = wasSingleTouch && !wasDragging && movedDistance < TOUCH_MOVE_CANCEL_PX;
  if (state.touchLongPressPointerId === event.pointerId) {
    clearTouchLongPress();
  }
  state.touchPointers.delete(event.pointerId);
  if (wasDragging) {
    send({ type: "pointer_button", button: 1, down: false });
    state.touchDragPointerId = null;
  } else if (isTap) {
    send({ type: "pointer_button", button: 1, down: true });
    send({ type: "pointer_button", button: 1, down: false });
  }
  if (state.touchPointers.size >= 2) {
    state.touchScrollLastY = getAverageTouchClientY();
  } else {
    state.touchScrollLastY = null;
  }
  if (state.touchPointers.size === 1) {
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
        try {
          await navigator.clipboard.writeText(entry.payload.text);
          showToast("clipboard_history_copied", "History text copied");
        } catch (error) {
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
      showToast(`${side}_clipboard_changed`, clipboardPreview(normalized));
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
      payload.image_png_b64 = await blobToBase64(blob);
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
  try {
    return updateClipboardState("local", await readLocalClipboard(), { announce: true });
  } catch (error) {
    showToast("local_clipboard_read_failed", error.message || String(error));
    return state.localClipboard;
  }
}

async function writeLocalClipboard(payload) {
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
}

async function pasteLocalClipboardToRemote() {
  if (state.socket?.readyState !== WebSocket.OPEN) {
    showToast("paste_unavailable", "Connect to the server first");
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
    await writeLocalClipboard(state.remoteClipboard);
    showToast("clipboard_copied", "Remote clipboard copied locally");
  } catch (error) {
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
    setStatus(state.socket?.readyState === WebSocket.OPEN ? "Connected" : "Disconnected");
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
    if (!state.micEnabled || state.socket?.readyState !== WebSocket.OPEN) return;
    stopMicrophoneCapture();
    void startMicrophoneCapture();
  });
  fpsInput.addEventListener("input", persistCurrentSettings);
  fpsInput.addEventListener("change", persistCurrentSettings);
  scrollSpeedInput.addEventListener("input", persistCurrentSettings);
  scrollSpeedInput.addEventListener("change", persistCurrentSettings);
  audioLatencyInput.addEventListener("input", persistCurrentSettings);
  audioLatencyInput.addEventListener("change", persistCurrentSettings);
  audioClockRateInput.addEventListener("input", persistCurrentSettings);
  audioClockRateInput.addEventListener("change", persistCurrentSettings);
  audioClockAutoInput.addEventListener("change", () => {
    state.audioClockAutoLastIncreaseAt = 0;
    state.audioClockAutoLastIncreaseLead = 0;
    state.audioClockAutoLastSlowTuneAt = 0;
    persistCurrentSettings();
  });
  autoDisconnectMinutesInput.addEventListener("input", persistCurrentSettings);
  autoDisconnectMinutesInput.addEventListener("change", persistCurrentSettings);
  touchModeSelect.addEventListener("change", persistCurrentSettings);
  directTouchScrollInput.addEventListener("change", persistCurrentSettings);
  for (const button of tabButtons) {
    button.addEventListener("click", () => {
      setActiveTab(button.dataset.tabTarget || "status");
    });
  }
  $("connect").addEventListener("click", () => connect());
  $("disconnect").addEventListener("click", disconnect);
  micToggle.addEventListener("click", toggleMicrophone);
  cameraToggle.addEventListener("click", toggleCamera);
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
  mobileKeyboardTrigger.addEventListener("click", () => {
    if (document.activeElement === mobileKeyboardInput) {
      releaseInput();
      return;
    }
    focusMobileKeyboard();
  });
  mobileKeyboardInput.addEventListener("beforeinput", handleMobileKeyboardBeforeInput);
  mobileKeyboardInput.addEventListener("input", () => {
    if (mobileKeyboardInput.value) {
      sendRemoteText(mobileKeyboardInput.value);
    }
    mobileKeyboardInput.value = "";
  });
  mobileKeyboardInput.addEventListener("keydown", handleMobileKeyboardKeydown);
  mobileKeyboardInput.addEventListener("focus", syncMobileKeyboardButton);
  mobileKeyboardInput.addEventListener("blur", () => {
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
    void primeAudioPlayback();
    if (event.target instanceof HTMLElement && event.target.closest("#control-panel")) {
      releaseInput();
    }
  });
  canvas.addEventListener("pointerdown", (event) => {
    if (reconnectFromViewport()) {
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
      handleTouchPointerEnd(event);
    }
  });
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    synchronizeModifierState(event);
    queueWheel(event.deltaY);
  }, { passive: false });
  window.addEventListener("keydown", (event) => {
    void primeAudioPlayback();
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
    send({ type: "key", key, down: true });
    if (!event.repeat) {
      sendPressedKeyState();
    }
    event.preventDefault();
  });
  window.addEventListener("keyup", (event) => {
    const key = normalizeKey(event);
    if (!key) return;
    const released = releasePressedKey(key);
    releaseStaleModifierChordKeys(event, { exceptKey: key });
    synchronizeModifierState(event);
    if (released) {
      event.preventDefault();
      return;
    }
    if (!shouldHandleKeyboard(event)) return;
    send({ type: "key", key, down: false });
    event.preventDefault();
  });
  window.addEventListener("focus", () => {
    void refreshLocalClipboard();
    requestRemoteClipboard();
  });
  window.addEventListener("resize", applyCanvasZoom);
  window.visualViewport?.addEventListener("resize", applyCanvasZoom);
  window.addEventListener("blur", releaseInput);
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

async function removeServiceWorker() {
  if (!("serviceWorker" in navigator)) return;
  try {
    const registrations = await navigator.serviceWorker.getRegistrations();
    await Promise.all(registrations.map((registration) => registration.unregister()));
    if ("caches" in window) {
      const cacheNames = await caches.keys();
      await Promise.all(
        cacheNames
          .filter((cacheName) => cacheName.startsWith("vibe-rdesk-"))
          .map((cacheName) => caches.delete(cacheName)),
      );
    }
  } catch (error) {
    showToast("sw_remove_failed", error.message || String(error));
  }
}

updateClipboardState("local", state.localClipboard);
updateClipboardState("remote", state.remoteClipboard);
renderClipboardHistory();
syncMobileKeyboardButton();
setActiveTab("status");
applySettings(loadStoredSettings());
applyCanvasZoom();
resetStatusMetrics();
initVideoRenderer();
initControls();
void removeServiceWorker();
renderMicToggle();
renderCameraToggle();
setInterval(renderAudioBufferMetric, 1000);
setInterval(monitorConnectionHealth, HEALTH_WATCHDOG_INTERVAL_MS);
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
