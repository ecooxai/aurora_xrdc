const $ = (id) => document.getElementById(id);
const AUTH_STORAGE_KEY = "vibe_rdesk.passwd";
const SETTINGS_STORAGE_KEY = "vibe_rdesk.settings";
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
const HEALTH_WATCHDOG_INTERVAL_MS = 250;
const KEY_STATE_SYNC_INTERVAL_MS = 500;
const MAX_VIDEO_DECODE_QUEUE = 4;
const MAX_AUDIO_DECODE_QUEUE = 24;
const AUTO_DISCONNECT_DISABLED_MINUTES = 0;
const AUTO_DISCONNECT_ACTIVITY_REFRESH_MS = 1000;
const SETTINGS_RECONNECT_DELAY_MS = 3000;
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
  audioNextTime: 0,
  pendingAudioBuffers: [],
  pendingAudioDuration: 0,
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
  keyStateSyncTimer: 0,
  pendingPointer: null,
  pointerRaf: 0,
  pendingRelativePointer: null,
  relativePointerRaf: 0,
  wheelAccumulator: 0,
  localClipboard: { text: null, image_png_b64: null },
  remoteClipboard: { text: null, image_png_b64: null },
  localClipboardSig: "",
  remoteClipboardSig: "",
  localClipboardUpdatedAt: 0,
  remoteClipboardUpdatedAt: 0,
  clipboardHistory: [],
  passwd: "",
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
  autoDisconnectTimer: null,
  lastAutoDisconnectActivityAt: 0,
};

const status = $("status");
const viewportCard = $("viewport-card");
const canvas = $("screen");
const ctx = canvas.getContext("2d", { alpha: false });
const toast = $("toast");
const streamWarning = $("stream-warning");
const streamWarningText = $("stream-warning-text");
const authModal = $("auth-modal");
const authForm = $("auth-form");
const authInput = $("auth-passwd");
const authError = $("auth-error");
const controlPanel = $("control-panel");
const mobileKeyboardTrigger = $("mobile-keyboard-trigger");
const micToggle = $("mic-toggle");
const cameraToggle = $("camera-toggle");
const mobileKeyboardInput = $("mobile-keyboard-input");
const encoderStatus = $("encoder-status");
const codecSelect = $("codec");
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

function setStatus(text) {
  status.textContent = text;
  status.classList.toggle("hidden", text === "Connected");
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
  statusSpeedDownload.textContent = `↓ ${formatMbPerSecond(net_rx_kbps)}`;
  statusSpeedUpload.textContent = `↑ ${formatMbPerSecond(net_tx_kbps)}`;
  statusUpdatedAt.textContent = `Updated ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`;
}

function renderLatencyMetric() {
  statusLatency.textContent = Number.isFinite(state.wsLatencyMs)
    ? `${Math.round(state.wsLatencyMs)} ms`
    : "--";
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
    bitrate: settings.bitrate,
    audioBitrateKbps: settings.audioBitrateKbps,
    fps: settings.fps,
  });
}

function markAppliedStreamSettings(settings) {
  state.appliedStreamSettingsKey = streamReconnectSettingsKey(settings);
}

function reconnectForSettings(reason) {
  if (state.connecting) return;
  clearSettingsReconnectTimer();
  showToast("settings_reconnect", reason);
  closeConnection({ manual: false, preserveStatus: true });
  setStatus("Reconnecting...");
  setTimeout(() => {
    if (state.connecting) return;
    void connect();
  }, 150);
}

function maybeScheduleSettingsReconnect(settings = readSettingsFromControls()) {
  const socketOpen = state.socket?.readyState === WebSocket.OPEN;
  if (!socketOpen || state.connecting || state.reconnectingForLatency) {
    clearSettingsReconnectTimer();
    return;
  }
  const nextKey = streamReconnectSettingsKey(settings);
  if (!state.appliedStreamSettingsKey || nextKey === state.appliedStreamSettingsKey) {
    clearSettingsReconnectTimer();
    return;
  }
  clearSettingsReconnectTimer();
  state.settingsReconnectTimer = setTimeout(() => {
    state.settingsReconnectTimer = null;
    if (state.socket?.readyState !== WebSocket.OPEN || state.connecting) return;
    const latestSettings = readSettingsFromControls();
    if (streamReconnectSettingsKey(latestSettings) === state.appliedStreamSettingsKey) return;
    reconnectForSettings("Reconnect to apply stream settings");
  }, SETTINGS_RECONNECT_DELAY_MS);
}

function monitorConnectionHealth() {
  const now = performance.now();
  const socketOpen = state.socket?.readyState === WebSocket.OPEN;

  let warning = "";
  if (socketOpen && state.lastVideoPacketAt && now - state.lastVideoPacketAt > MEDIA_STALL_RESET_MS) {
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

function loadStoredPassword() {
  try {
    return localStorage.getItem(AUTH_STORAGE_KEY) || "";
  } catch {
    return "";
  }
}

function savePassword(passwd) {
  try {
    localStorage.setItem(AUTH_STORAGE_KEY, passwd);
  } catch {
    // Ignore storage failures; the session still works for this visit.
  }
}

function clampControlValue(control, value, fallback) {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) return fallback;
  const min = Number(control.min);
  const max = Number(control.max);
  return Math.min(max, Math.max(min, numeric));
}

function normalizeSettings(settings = {}) {
  const allowedCodecs = new Set(Array.from(codecSelect.options, (option) => option.value));
  const allowedAudioBitrates = new Set(Array.from(audioBitrateSelect.options, (option) => Number(option.value)));
  const allowedMicBitrates = new Set(Array.from(micBitrateSelect.options, (option) => Number(option.value)));
  const allowedTouchModes = new Set(Array.from(touchModeSelect.options, (option) => option.value));
  const defaultBitrate = Number(bitrateInput.value);
  const defaultAudioBitrate = Number(audioBitrateSelect.value);
  const defaultMicBitrate = Number(micBitrateSelect.value);
  const defaultFps = Number(fpsInput.value);
  const defaultScrollSpeed = Number(scrollSpeedInput.value);
  const defaultAutoDisconnectMinutes = Number(autoDisconnectMinutesInput.value);
  return {
    codec: allowedCodecs.has(settings.codec) ? settings.codec : codecSelect.value,
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
    autoDisconnectMinutes: clampControlValue(
      autoDisconnectMinutesInput,
      settings.autoDisconnectMinutes,
      defaultAutoDisconnectMinutes,
    ),
    viewZoomPercent: clampZoomPercent(settings.viewZoomPercent),
  };
}

function readSettingsFromControls() {
  return normalizeSettings({
    codec: codecSelect.value,
    bitrate: bitrateInput.value,
    audioBitrateKbps: audioBitrateSelect.value,
    micBitrateKbps: micBitrateSelect.value,
    fps: fpsInput.value,
    scrollSpeed: scrollSpeedInput.value,
    audioLatencyMs: audioLatencyInput.value,
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
  bitrateInput.value = String(normalized.bitrate);
  audioBitrateSelect.value = String(normalized.audioBitrateKbps);
  micBitrateSelect.value = String(normalized.micBitrateKbps);
  fpsInput.value = String(normalized.fps);
  scrollSpeedInput.value = String(normalized.scrollSpeed);
  audioLatencyInput.value = String(normalized.audioLatencyMs);
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

function persistCurrentSettings() {
  const settings = readSettingsFromControls();
  renderSettingsValues(settings);
  syncTouchModeControls(settings);
  saveSettings(settings);
  syncAutoDisconnectTimer(settings);
  maybeScheduleSettingsReconnect(settings);
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

function getPassword() {
  return state.passwd || authInput.value.trim();
}

function authUrl(path) {
  const url = new URL(path, window.location.href);
  const passwd = getPassword();
  if (passwd) url.searchParams.set("passwd", passwd);
  return url;
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
  const url = new URL("/api/auth", window.location.href);
  url.searchParams.set("passwd", passwd);
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) {
    const message = await response.text().catch(() => "");
    throw new Error(message || "Authentication failed");
  }
}

async function loadClipboardHistory() {
  const response = await fetch(authUrl("/api/clipboard/history"), { cache: "no-store" });
  if (!response.ok) {
    throw new Error(await response.text().catch(() => "Failed to load clipboard history"));
  }
  state.clipboardHistory = normalizeClipboardHistory(await response.json());
  renderClipboardHistory();
}

async function saveClipboardHistory() {
  const response = await fetch(authUrl("/api/clipboard/history"), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(state.clipboardHistory),
  });
  if (!response.ok) {
    throw new Error(await response.text().catch(() => "Failed to save clipboard history"));
  }
}

async function connect() {
  if (state.connecting) return;
  const passwd = authInput.value.trim();
  if (!passwd) {
    setAuthPrompt("Enter the server password.");
    return;
  }
  state.connecting = true;
  try {
    setStatus("Authenticating...");
    await verifyPassword(passwd);
    savePassword(passwd);
    state.passwd = passwd;
    clearAuthPrompt();
    closeConnection({ manual: false, preserveStatus: true });
    await loadClipboardHistory();
    state.manualDisconnect = false;
    clearTimeout(state.reconnectTimer);
    void primeAudioPlayback();
    const {
      codec,
      bitrate,
      audioBitrateKbps,
      fps,
    } = readSettingsFromControls();
    const requestedStreamSettings = { codec, bitrate, audioBitrateKbps, fps };
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
    const url = new URL("/ws", window.location.href);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.searchParams.set("codec", codec);
    url.searchParams.set("bitrate_kbps", bitrate);
    url.searchParams.set("audio_bitrate_kbps", audioBitrateKbps);
    url.searchParams.set("fps", fps);
    url.searchParams.set("passwd", passwd);
    setStatus("Connecting...");
    setEncoderStatus("Connecting...");
    state.socket = new WebSocket(url);
    state.socket.binaryType = "arraybuffer";
    state.socket.onopen = () => {
      markAppliedStreamSettings(requestedStreamSettings);
      state.reconnectAttempt = 0;
      state.reconnectingForLatency = false;
      state.highLatencySinceAt = 0;
      setStreamWarning("");
      setStatus("Connected");
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
    state.socket.onclose = () => {
      setStatus("Disconnected");
      stopKeyStateSync();
      clearInterval(startPing.timer);
      stopRemoteClipboardPolling();
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
    showToast("auth_failed", error.message || String(error));
    setAuthPrompt(error.message || "Authentication failed");
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
  clearPendingVideoFrame();
  state.renderingVideoFrame = false;
  state.decoder?.close();
  state.decoder = null;
  state.audioDecoder?.close();
  state.audioDecoder = null;
  resetAudioPlayback();
  state.decoderConfigKey = "";
  state.audioConfigKey = "";
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
    showToast("decoder_config_failed", `Decoder does not support ${state.codecString}`);
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
    error: (err) => showToast("decoder_error", err.message || String(err)),
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

function clearPendingVideoFrame() {
  if (state.pendingVideoFrame) {
    state.pendingVideoFrame.close();
    state.pendingVideoFrame = null;
  }
}

function resetVideoDecoderForLiveCatchup() {
  clearPendingVideoFrame();
  state.renderingVideoFrame = false;
  state.decoder?.close();
  state.decoder = null;
  state.decoderConfigKey = "";
  void setupDecoder();
  state.waitingForKeyframe = true;
}

function queueVideoFrameForRender(frame) {
  if (state.pendingVideoFrame) {
    state.pendingVideoFrame.close();
    markStaleDrop("Dropping delayed video");
  }
  state.pendingVideoFrame = frame;
  if (!state.renderingVideoFrame) {
    void renderLatestVideoFrame();
  }
}

async function renderLatestVideoFrame() {
  if (state.renderingVideoFrame) return;
  state.renderingVideoFrame = true;
  try {
    while (state.pendingVideoFrame) {
      const frame = state.pendingVideoFrame;
      state.pendingVideoFrame = null;
      const sentAtMs = Number(frame.timestamp ?? 0) / 1000;
      if (estimateMediaAgeMs(sentAtMs) > LIVE_MEDIA_MAX_AGE_MS) {
        frame.close();
        markStaleDrop("Dropping delayed video");
        continue;
      }
      await drawFrame(frame);
      frame.close();
    }
  } finally {
    state.renderingVideoFrame = false;
    if (state.pendingVideoFrame) {
      void renderLatestVideoFrame();
    }
  }
}

async function drawFrame(frame) {
  const remoteSizeChanged = (
    state.remoteScreenWidth !== frame.displayWidth
    || state.remoteScreenHeight !== frame.displayHeight
  );
  if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
    canvas.width = frame.displayWidth;
    canvas.height = frame.displayHeight;
  }
  if (remoteSizeChanged) {
    state.remoteScreenWidth = frame.displayWidth;
    state.remoteScreenHeight = frame.displayHeight;
    applyCanvasZoom();
  }
  try {
    ctx.drawImage(frame, 0, 0, canvas.width, canvas.height);
  } catch {
    const bitmap = await createImageBitmap(frame);
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
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
    showToast("decode_submit_failed", error.message || String(error));
  }
}

function handleAudioFrame(buffer, view) {
  if (!state.audioEnabled) return;
  const sentAt = Number(view.getBigUint64(1, true));
  const receivedAt = performance.now();
  const stalledForMs = state.lastAudioPacketAt ? receivedAt - state.lastAudioPacketAt : 0;
  state.lastAudioPacketAt = receivedAt;
  const mediaAgeMs = estimateMediaAgeMs(sentAt);
  if (stalledForMs >= MEDIA_STALL_RESET_MS || mediaAgeMs > LIVE_MEDIA_MAX_AGE_MS) {
    markStaleDrop("Dropping delayed audio");
    resetAudioDecoderForLiveCatchup();
    return;
  }
  const length = view.getUint32(9, true);
  const bytes = new Uint8Array(buffer, 13, length);
  const frame = parseAdtsFrame(bytes);
  if (!frame) return;
  setupAudioDecoder(frame);
  if (!state.audioDecoder || state.audioDecoder.state === "closed") return;
  if ((state.audioDecoder.decodeQueueSize ?? 0) > MAX_AUDIO_DECODE_QUEUE) {
    markStaleDrop("Audio decoder catching up");
    resetAudioDecoderForLiveCatchup();
    return;
  }
  try {
    state.audioDecoder.decode(new EncodedAudioChunk({
      type: "key",
      timestamp: sentAt * 1000,
      data: frame.payload,
    }));
  } catch (error) {
    showToast("audio_decode_submit_failed", error.message || String(error));
  }
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
  state.pendingAudioBuffers = [];
  state.pendingAudioDuration = 0;
}

function currentConfiguredAudioLatencyMs() {
  return clampControlValue(
    audioLatencyInput,
    audioLatencyInput.value,
    Number(audioLatencyInput.value),
  );
}

function currentAudioBufferProfile() {
  const latencyMs = Number.isFinite(state.wsLatencyMs) ? state.wsLatencyMs : 0;
  const profile = AUDIO_BUFFER_PROFILES.find((entry) => latencyMs >= entry.minLatencyMs)
    || AUDIO_BUFFER_PROFILES[AUDIO_BUFFER_PROFILES.length - 1];
  const extraLatencyMs = currentConfiguredAudioLatencyMs();
  const extraSeconds = extraLatencyMs / 1000;
  const targetLeadSeconds = profile.targetLeadSeconds + extraSeconds;
  return {
    targetLeadSeconds,
    continueLeadSeconds: Math.max(0.06, targetLeadSeconds * 0.5),
    maxQueueSeconds: profile.maxQueueSeconds + extraSeconds,
    resetGraceSeconds: Math.max(profile.resetGraceSeconds, 0.05),
  };
}

function currentAudioStartSlack(audioContext) {
  const baseLatency = Number.isFinite(audioContext?.baseLatency) ? audioContext.baseLatency : 0;
  return Math.max(0.03, Math.min(0.08, Math.max(baseLatency * 3, 0.05)));
}

function scheduleAudioBuffer(audioContext, audioBuffer, profile = currentAudioBufferProfile()) {
  const source = audioContext.createBufferSource();
  source.buffer = audioBuffer;
  source.connect(audioContext.destination);
  const now = audioContext.currentTime;
  const queuedFor = state.audioNextTime > now ? state.audioNextTime - now : 0;
  if (!state.audioNextTime || state.audioNextTime < now - profile.resetGraceSeconds) {
    state.audioNextTime = now + currentAudioStartSlack(audioContext);
  } else if (queuedFor > profile.maxQueueSeconds) {
    resetAudioPlayback();
    state.audioNextTime = now + currentAudioStartSlack(audioContext);
  }
  state.audioSources.add(source);
  source.start(state.audioNextTime);
  state.audioNextTime += audioBuffer.duration;
  source.onended = () => {
    state.audioSources.delete(source);
    source.disconnect();
  };
}

function flushPendingAudioPlayback(audioContext) {
  const profile = currentAudioBufferProfile();
  while (state.pendingAudioBuffers.length > 0) {
    const now = audioContext.currentTime;
    const queuedFor = state.audioNextTime > now ? state.audioNextTime - now : 0;
    const playbackStale = !state.audioNextTime || state.audioNextTime < now - profile.resetGraceSeconds;
    if (playbackStale && state.pendingAudioDuration < profile.targetLeadSeconds) {
      break;
    }
    if (!playbackStale) {
      const totalAvailableLead = queuedFor + state.pendingAudioDuration;
      if (queuedFor < profile.continueLeadSeconds && totalAvailableLead < profile.targetLeadSeconds) {
        break;
      }
    }
    if (!playbackStale && queuedFor >= profile.maxQueueSeconds) {
      break;
    }
    const nextBuffer = state.pendingAudioBuffers.shift();
    if (!nextBuffer) {
      break;
    }
    state.pendingAudioDuration = Math.max(0, state.pendingAudioDuration - nextBuffer.duration);
    scheduleAudioBuffer(audioContext, nextBuffer, profile);
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
  try {
    await ensureAudioContext();
  } catch {
    // Autoplay policy may require a user gesture; playback will retry later.
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
  const response = await fetch(authUrl("/api/camera/chunk"), {
    method: "POST",
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
  const response = await fetch(authUrl("/api/camera/stop"), {
    method: "POST",
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
  const audioContext = await ensureAudioContext().catch(() => null);
  if (!audioContext) {
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
  state.pendingAudioBuffers.push(audioBuffer);
  state.pendingAudioDuration += audioBuffer.duration;
  flushPendingAudioPlayback(audioContext);
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
  if (!state.inputCaptured && state.pressedKeys.size === 0) return;
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
  sendPressedKeyState();
}

function releasePressedKey(key) {
  if (!state.pressedKeys.has(key)) return false;
  state.pressedKeys.delete(key);
  send({ type: "key", key, down: false });
  sendPressedKeyState();
  return true;
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

function synchronizeModifierState(event, { pressMissing = false } = {}) {
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

function flushWheel(direction) {
  send({ type: "pointer_wheel", delta_y: direction });
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
  const clampedSteps = Math.max(-3, Math.min(3, steps));
  for (let i = 0; i < Math.abs(clampedSteps); i += 1) {
    flushWheel(clampedSteps > 0 ? 1 : -1);
  }
}

function queuePointerMove(x, y) {
  state.pendingPointer = { x, y };
  if (state.pointerRaf) return;
  state.pointerRaf = requestAnimationFrame(() => {
    state.pointerRaf = 0;
    const point = state.pendingPointer;
    state.pendingPointer = null;
    if (!point) return;
    send({ type: "pointer_absolute", x: point.x, y: point.y });
  });
}

function queueRelativePointerMove(dx, dy) {
  if (!dx && !dy) return;
  const pending = state.pendingRelativePointer || { dx: 0, dy: 0 };
  pending.dx += dx;
  pending.dy += dy;
  state.pendingRelativePointer = pending;
  if (state.relativePointerRaf) return;
  state.relativePointerRaf = requestAnimationFrame(() => {
    state.relativePointerRaf = 0;
    const delta = state.pendingRelativePointer;
    state.pendingRelativePointer = null;
    if (!delta || (!delta.dx && !delta.dy)) return;
    send({ type: "pointer_move", dx: delta.dx, dy: delta.dy });
  });
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
  state.pendingPointer = null;
  state.pendingRelativePointer = null;
  if (state.pointerRaf) {
    cancelAnimationFrame(state.pointerRaf);
    state.pointerRaf = 0;
  }
  if (state.relativePointerRaf) {
    cancelAnimationFrame(state.relativePointerRaf);
    state.relativePointerRaf = 0;
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
  if (!state.passwd) {
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
    const response = await fetch(authUrl("/api/upload"), { method: "POST", body: formData });
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
  authInput.addEventListener("input", () => {
    authError.textContent = "";
    authError.classList.add("hidden");
  });
  codecSelect.addEventListener("change", persistCurrentSettings);
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
  canvas.addEventListener("pointermove", (event) => {
    if (isTouchPointer(event)) {
      handleTouchPointerMove(event);
      return;
    }
    const point = pointerToCanvas(event);
    if (!point) return;
    synchronizeModifierState(event);
    queuePointerMove(point.x, point.y);
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
    if (isReleaseInputChord(event)) {
      event.preventDefault();
      releaseInput();
      return;
    }
    if (!shouldHandleKeyboard(event)) return;
    synchronizeModifierState(event, { pressMissing: true });
    const key = normalizeKey(event);
    if (!key) return;
    if (!event.repeat && state.pressedKeys.has(key)) {
      logInputState("keydown-duplicate", event, { normalizedKey: key });
      event.preventDefault();
      return;
    }
    if (!event.repeat) {
      state.pressedKeys.add(key);
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
initControls();
void removeServiceWorker();
state.passwd = loadStoredPassword();
authInput.value = state.passwd;
renderMicToggle();
renderCameraToggle();
setInterval(monitorConnectionHealth, HEALTH_WATCHDOG_INTERVAL_MS);
if (state.passwd) {
  clearAuthPrompt();
  void connect();
} else {
  setAuthPrompt();
}
