const $ = (id) => document.getElementById(id);
const AUTH_STORAGE_KEY = "vibe_rdesk.passwd";
const SETTINGS_STORAGE_KEY = "vibe_rdesk.settings";
const TOUCH_LONG_PRESS_MS = 1000;
const TOUCH_MOVE_CANCEL_PX = 14;
const DIRECT_TOUCH_SCROLL_MULTIPLIER = 3;
const state = {
  socket: null,
  decoder: null,
  audioDecoder: null,
  audioContext: null,
  audioSources: new Set(),
  activeCodec: "h264",
  codecString: "avc1.64001f",
  description: null,
  decoderConfigKey: "",
  audioConfigKey: "",
  audioEnabled: false,
  audioNextTime: 0,
  waitingForKeyframe: true,
  frameCount: 0,
  bytesReceived: 0,
  lastNetAt: performance.now(),
  netKbps: 0,
  manualDisconnect: false,
  reconnectTimer: null,
  reconnectAttempt: 0,
  touchPointers: new Map(),
  touchLongPressTimer: 0,
  touchLongPressPointerId: null,
  touchDragPointerId: null,
  touchScrollLastY: null,
  inputCaptured: false,
  pressedKeys: new Set(),
  suppressedKeyups: new Set(),
  activePasteShortcut: false,
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
  clipboardRefreshTimers: [],
  passwd: "",
  connecting: false,
};

const status = $("status");
const canvas = $("screen");
const ctx = canvas.getContext("2d", { alpha: false });
const toast = $("toast");
const authModal = $("auth-modal");
const authForm = $("auth-form");
const authInput = $("auth-passwd");
const authError = $("auth-error");
const settingsPanel = $("settings-panel");
const encoderStatus = $("encoder-status");
const codecSelect = $("codec");
const bitrateInput = $("bitrate");
const bitrateValue = $("bitrate-value");
const fpsInput = $("fps");
const fpsValue = $("fps-value");
const scrollSpeedInput = $("scroll-speed");
const scrollSpeedValue = $("scroll-speed-value");
const touchModeSelect = $("touch-mode");
const directTouchScrollInput = $("direct-touch-scroll");
const directTouchScrollLabel = $("direct-touch-scroll-label");
const uploadPanel = $("upload-panel");
const uploadAction = $("upload-action");
const uploadInput = $("upload-input");
const localClipboardCopyBtn = $("local-clipboard-copy-btn");
const localClipboardSyncBtn = $("local-clipboard-sync-btn");
const remoteClipboardCopyBtn = $("remote-clipboard-copy-btn");
const remoteClipboardSyncBtn = $("remote-clipboard-sync-btn");
const AAC_SAMPLE_RATES = [
  96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050,
  16000, 12000, 11025, 8000, 7350,
];
const AUDIO_TARGET_LEAD_SECONDS = 0.02;
const AUDIO_MAX_QUEUE_SECONDS = 0.2;
const AUDIO_RESET_GRACE_SECONDS = 0.05;

function setStatus(text) {
  status.textContent = text;
  status.classList.toggle("hidden", text === "Connected");
}

function setEncoderStatus(text) {
  encoderStatus.textContent = text;
}

function showToast(code, message) {
  toast.textContent = `${code}: ${message}`;
  toast.dataset.copy = `${code}: ${message}`;
  toast.classList.remove("hidden");
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(() => toast.classList.add("hidden"), 10000);
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
  const allowedTouchModes = new Set(Array.from(touchModeSelect.options, (option) => option.value));
  const defaultBitrate = Number(bitrateInput.value);
  const defaultFps = Number(fpsInput.value);
  const defaultScrollSpeed = Number(scrollSpeedInput.value);
  return {
    codec: allowedCodecs.has(settings.codec) ? settings.codec : codecSelect.value,
    bitrate: clampControlValue(bitrateInput, settings.bitrate, defaultBitrate),
    fps: clampControlValue(fpsInput, settings.fps, defaultFps),
    scrollSpeed: clampControlValue(scrollSpeedInput, settings.scrollSpeed, defaultScrollSpeed),
    touchMode: allowedTouchModes.has(settings.touchMode) ? settings.touchMode : touchModeSelect.value,
    directTouchScroll: settings.directTouchScroll === true,
  };
}

function readSettingsFromControls() {
  return normalizeSettings({
    codec: codecSelect.value,
    bitrate: bitrateInput.value,
    fps: fpsInput.value,
    scrollSpeed: scrollSpeedInput.value,
    touchMode: touchModeSelect.value,
    directTouchScroll: directTouchScrollInput.checked,
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
}

function syncTouchModeControls(settings = readSettingsFromControls()) {
  const isDirectTouch = settings.touchMode === "direct_touch";
  directTouchScrollInput.disabled = !isDirectTouch;
  directTouchScrollLabel.classList.toggle("is-disabled", !isDirectTouch);
}

function applySettings(settings) {
  const normalized = normalizeSettings(settings);
  codecSelect.value = normalized.codec;
  bitrateInput.value = String(normalized.bitrate);
  fpsInput.value = String(normalized.fps);
  scrollSpeedInput.value = String(normalized.scrollSpeed);
  touchModeSelect.value = normalized.touchMode;
  directTouchScrollInput.checked = normalized.directTouchScroll;
  renderSettingsValues(normalized);
  syncTouchModeControls(normalized);
}

function persistCurrentSettings() {
  const settings = readSettingsFromControls();
  renderSettingsValues(settings);
  syncTouchModeControls(settings);
  saveSettings(settings);
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
    state.manualDisconnect = false;
    clearTimeout(state.reconnectTimer);
    void primeAudioPlayback();
    const { codec, bitrate, fps } = readSettingsFromControls();
    state.activeCodec = codec;
    state.frameCount = 0;
    state.bytesReceived = 0;
    state.lastNetAt = performance.now();
    const url = new URL("/ws", window.location.href);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.searchParams.set("codec", codec);
    url.searchParams.set("bitrate_kbps", bitrate);
    url.searchParams.set("fps", fps);
    url.searchParams.set("passwd", passwd);
    setStatus("Connecting...");
    setEncoderStatus("Connecting...");
    state.socket = new WebSocket(url);
    state.socket.binaryType = "arraybuffer";
    state.socket.onopen = () => {
      state.reconnectAttempt = 0;
      setStatus("Connected");
      startPing();
      void refreshLocalClipboard();
      requestRemoteClipboard();
    };
    state.socket.onclose = () => {
      setStatus("Disconnected");
      clearInterval(startPing.timer);
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
  clearClipboardRefreshTimers();
  clearInterval(startPing.timer);
  state.decoder?.close();
  state.decoder = null;
  state.audioDecoder?.close();
  state.audioDecoder = null;
  resetAudioPlayback();
  state.decoderConfigKey = "";
  state.audioConfigKey = "";
  state.audioEnabled = false;
  state.waitingForKeyframe = true;
  resetKeys();
  if (state.socket) {
    const socket = state.socket;
    state.socket = null;
    socket.onclose = null;
    socket.close();
  }
  if (!preserveStatus) setEncoderStatus("Not connected");
  if (!preserveStatus) setStatus("Disconnected");
}

function disconnect() {
  closeConnection({ manual: true });
}

function captureInput() {
  state.inputCaptured = true;
  canvas.focus({ preventScroll: true });
  void primeAudioPlayback();
}

function releaseInput() {
  resetTouchInteraction();
  state.inputCaptured = false;
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
    if (message.codec_string) {
      state.codecString = message.codec_string;
    }
    if (message.description_b64) {
      state.description = Uint8Array.from(atob(message.description_b64), (c) => c.charCodeAt(0));
    }
    state.audioEnabled = !!message.audio_enabled;
    setupDecoder();
    if (!state.audioEnabled) {
      state.audioDecoder?.close();
      state.audioDecoder = null;
      resetAudioPlayback();
      state.audioConfigKey = "";
    }
    setEncoderStatus(`${message.active_encoder || "ready"} ${message.encoder_mode || ""}`.trim());
  } else if (message.type === "error") {
    showToast(message.code, message.message);
  } else if (message.type === "clipboard" && message.side === "remote") {
    updateClipboardState("remote", message.payload, { announce: true });
  }
}

function setupDecoder() {
  if (!("VideoDecoder" in window)) {
    showToast("webcodecs_missing", "This browser does not support WebCodecs");
    return;
  }
  const configKey = `${state.codecString}:${state.description ? btoa(String.fromCharCode(...state.description)) : ""}`;
  if (state.decoder && state.decoder.state !== "closed" && state.decoderConfigKey === configKey) {
    return;
  }
  state.decoder?.close();
  state.decoder = new VideoDecoder({
    output: async (frame) => {
      await drawFrame(frame);
      frame.close();
    },
    error: (err) => showToast("decoder_error", err.message || String(err)),
  });
  const config = { codec: state.codecString, optimizeForLatency: true };
  if (state.description) config.description = state.description;
  state.decoder.configure(config);
  state.decoderConfigKey = configKey;
  state.waitingForKeyframe = true;
}

async function drawFrame(frame) {
  if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
    canvas.width = frame.displayWidth;
    canvas.height = frame.displayHeight;
  }
  const bitmap = await createImageBitmap(frame);
  ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
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
  const length = view.getUint32(10, true);
  const bytes = new Uint8Array(buffer, 14, length);
  state.bytesReceived += length;
  const now = performance.now();
  const deltaSec = Math.max(0.2, (now - state.lastNetAt) / 1000);
  state.netKbps = (state.bytesReceived * 8) / deltaSec / 1000;
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
  const length = view.getUint32(9, true);
  const bytes = new Uint8Array(buffer, 13, length);
  const frame = parseAdtsFrame(bytes);
  if (!frame) return;
  setupAudioDecoder(frame);
  if (!state.audioDecoder || state.audioDecoder.state === "closed") return;
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
}

async function ensureAudioContext() {
  if (!window.AudioContext) return null;
  if (!state.audioContext || state.audioContext.state === "closed") {
    state.audioContext = new AudioContext({ latencyHint: "interactive", sampleRate: 48000 });
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
  const source = audioContext.createBufferSource();
  source.buffer = audioBuffer;
  source.connect(audioContext.destination);
  const now = audioContext.currentTime;
  const queuedFor = state.audioNextTime > now ? state.audioNextTime - now : 0;
  if (!state.audioNextTime || state.audioNextTime < now - AUDIO_RESET_GRACE_SECONDS) {
    state.audioNextTime = now + AUDIO_TARGET_LEAD_SECONDS;
  } else if (queuedFor > AUDIO_MAX_QUEUE_SECONDS) {
    resetAudioPlayback();
    state.audioNextTime = now + AUDIO_TARGET_LEAD_SECONDS;
  }
  state.audioSources.add(source);
  source.start(state.audioNextTime);
  state.audioNextTime += audioBuffer.duration;
  source.onended = () => {
    state.audioSources.delete(source);
    source.disconnect();
  };
  audioData.close();
}

function send(message) {
  if (state.socket?.readyState === WebSocket.OPEN) {
    state.socket.send(JSON.stringify(message));
  }
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

function sendRemoteClipboard(payload) {
  send({ type: "clipboard_set", payload });
}

function startPing() {
  clearInterval(startPing.timer);
  startPing.timer = setInterval(() => {
    send({ type: "ping", sent_at_ms: Date.now() });
  }, 1000);
}

function resetKeys() {
  for (const key of state.pressedKeys) {
    send({ type: "key", key, down: false });
  }
  state.pressedKeys.clear();
  state.suppressedKeyups.clear();
  state.activePasteShortcut = false;
}

function releasePressedKey(key) {
  if (!state.pressedKeys.has(key)) return false;
  state.pressedKeys.delete(key);
  send({ type: "key", key, down: false });
  return true;
}

function releaseClipboardChord(primaryKey) {
  releasePressedKey(primaryKey);
  releasePressedKey("Control_L");
  releasePressedKey("Control_R");
  releasePressedKey("Super_L");
  releasePressedKey("Super_R");
}

function currentClipboardModifiers() {
  return ["Control_L", "Control_R", "Super_L", "Super_R"].filter((key) => state.pressedKeys.has(key));
}

function markSuppressedKeyups(keys) {
  for (const key of keys) {
    state.suppressedKeyups.add(key);
  }
}

function dropPressedKey(key) {
  state.pressedKeys.delete(key);
}

function reconcileModifierState(event) {
  if (!event.ctrlKey) {
    releasePressedKey("Control_L");
    releasePressedKey("Control_R");
  }
  if (!event.metaKey) {
    releasePressedKey("Super_L");
    releasePressedKey("Super_R");
  }
  if (!event.altKey) {
    releasePressedKey("Alt_L");
    releasePressedKey("Alt_R");
  }
  if (!event.shiftKey) {
    releasePressedKey("Shift_L");
    releasePressedKey("Shift_R");
  }
}

function normalizeKey(event) {
  const modifierMap = {
    Shift: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Shift_R" : "Shift_L",
    Control: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Control_R" : "Control_L",
    Alt: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Alt_R" : "Alt_L",
    Meta: event.location === KeyboardEvent.DOM_KEY_LOCATION_RIGHT ? "Super_R" : "Super_L",
  };
  if (modifierMap[event.key]) return modifierMap[event.key];

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

function shouldHandleKeyboard(event) {
  if (!state.inputCaptured) return false;
  const target = event.target;
  if (target instanceof HTMLElement) {
    if (target.closest("#settings-panel")) return false;
    if (target.matches("input, select, textarea, button")) return false;
  }
  return true;
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

function updateClipboardState(side, payload, { announce = false } = {}) {
  const normalized = normalizeClipboardPayload(payload);
  const signature = clipboardSignature(normalized);
  const payloadKey = side === "local" ? "localClipboard" : "remoteClipboard";
  const sigKey = side === "local" ? "localClipboardSig" : "remoteClipboardSig";
  const timeKey = side === "local" ? "localClipboardUpdatedAt" : "remoteClipboardUpdatedAt";
  const previousSignature = state[sigKey];
  const changed = previousSignature !== signature;

  state[payloadKey] = normalized;
  if (changed) {
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
  textEl.textContent = hasText ? payload.text : "Empty";
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
    metaEl.textContent = side === "local" ? "Ready to push remote" : "Ready to copy locally";
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

async function pushLocalClipboardToRemote() {
  const payload = await refreshLocalClipboard();
  if (!payload.text && !payload.image_png_b64) {
    showToast("local_clipboard_empty", "Local clipboard is empty");
    return;
  }
  sendRemoteClipboard(payload);
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

async function copyClipboardText(side) {
  const payload = side === "local" ? state.localClipboard : state.remoteClipboard;
  if (!payload.text) {
    showToast("clipboard_text_empty", `${side} clipboard has no text`);
    return;
  }
  try {
    await navigator.clipboard.writeText(payload.text);
    showToast("clipboard_text_copied", `${side} clipboard text copied`);
  } catch (error) {
    showToast("clipboard_text_copy_failed", error.message || String(error));
  }
}

function clearClipboardRefreshTimers() {
  for (const timer of state.clipboardRefreshTimers) {
    clearTimeout(timer);
  }
  state.clipboardRefreshTimers = [];
}

function scheduleRemoteClipboardRefresh(delaysMs = [250]) {
  clearClipboardRefreshTimers();
  state.clipboardRefreshTimers = delaysMs.map((delayMs) => setTimeout(() => {
    requestRemoteClipboard();
  }, delayMs));
}

async function maybeSyncClipboardShortcut({ ctrlKey, metaKey, altKey, key }) {
  const modifier = ctrlKey || metaKey;
  if (!modifier || altKey) return;
  if (key === "c" || key === "x") {
    scheduleRemoteClipboardRefresh([180, 500, 1000, 1800]);
  }
}

async function syncLocalClipboardForPaste() {
  try {
    const payload = await refreshLocalClipboard();
    if (!payload.text && !payload.image_png_b64) {
      return;
    }
    if (
      state.remoteClipboardUpdatedAt > state.localClipboardUpdatedAt
      && state.remoteClipboardSig
      && state.remoteClipboardSig !== state.localClipboardSig
    ) {
      requestRemoteClipboard();
      showToast("remote_clipboard_kept", clipboardPreview(state.remoteClipboard));
      return;
    }
    sendRemoteClipboard(payload);
  } catch (error) {
    showToast("remote_clipboard_push_failed", error.message || String(error));
  }
}

async function triggerRemotePasteShortcut() {
  if (state.activePasteShortcut) return;
  state.activePasteShortcut = true;
  const modifierKeys = currentClipboardModifiers();
  markSuppressedKeyups([...modifierKeys, "v"]);
  try {
    for (const key of modifierKeys) {
      dropPressedKey(key);
    }
    send({ type: "reset_input" });
    await syncLocalClipboardForPaste();
    send({ type: "paste" });
  } finally {
    state.activePasteShortcut = false;
  }
}

function handleClipboardShortcut(primaryKey, action) {
  if (!state.inputCaptured) return;
  setTimeout(() => {
    releaseClipboardChord(primaryKey);
  }, 0);
  void action();
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
    setStatus("Connected");
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
  fpsInput.addEventListener("input", persistCurrentSettings);
  fpsInput.addEventListener("change", persistCurrentSettings);
  scrollSpeedInput.addEventListener("input", persistCurrentSettings);
  scrollSpeedInput.addEventListener("change", persistCurrentSettings);
  touchModeSelect.addEventListener("change", persistCurrentSettings);
  directTouchScrollInput.addEventListener("change", persistCurrentSettings);
  $("connect").addEventListener("click", () => connect());
  $("disconnect").addEventListener("click", disconnect);
  uploadAction.addEventListener("click", () => uploadInput.click());
  uploadInput.addEventListener("change", () => {
    void uploadSelectedFile();
  });
  localClipboardSyncBtn.addEventListener("click", () => {
    void pushLocalClipboardToRemote();
  });
  remoteClipboardSyncBtn.addEventListener("click", () => {
    void copyRemoteClipboardToLocal();
  });
  localClipboardCopyBtn.addEventListener("click", () => {
    void copyClipboardText("local");
  });
  remoteClipboardCopyBtn.addEventListener("click", () => {
    void copyClipboardText("remote");
  });
  settingsPanel.addEventListener("toggle", () => {
    if (settingsPanel.open) {
      uploadPanel.open = false;
      releaseInput();
    }
  });
  uploadPanel.addEventListener("toggle", () => {
    if (uploadPanel.open) {
      settingsPanel.open = false;
      releaseInput();
    }
  });
  document.addEventListener("pointerdown", (event) => {
    if (event.target instanceof HTMLElement && event.target.closest("#settings-panel, #upload-panel")) {
      releaseInput();
    }
  });
  canvas.addEventListener("pointerdown", (event) => {
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
    reconcileModifierState(event);
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
    reconcileModifierState(event);
    queuePointerMove(point.x, point.y);
  });
  canvas.addEventListener("pointerup", (event) => {
    reconcileModifierState(event);
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
    reconcileModifierState(event);
    queueWheel(event.deltaY);
  }, { passive: false });
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      releaseInput();
      return;
    }
    if (!shouldHandleKeyboard(event)) return;
    reconcileModifierState(event);
    const key = normalizeKey(event);
    if (!key) return;
    if ((event.ctrlKey || event.metaKey) && !event.altKey && key === "v") {
      event.preventDefault();
      if (!event.repeat) {
        void triggerRemotePasteShortcut();
      }
      return;
    }
    if (!event.repeat && state.pressedKeys.has(key)) {
      logInputState("keydown-duplicate", event, { normalizedKey: key });
      return;
    }
    if (!event.repeat) state.pressedKeys.add(key);
    logInputState("keydown", event, { normalizedKey: key });
    send({ type: "key", key, down: true });
    event.preventDefault();
    void maybeSyncClipboardShortcut({
      ctrlKey: event.ctrlKey,
      metaKey: event.metaKey,
      altKey: event.altKey,
      key: event.key.toLowerCase(),
    });
  });
  window.addEventListener("paste", () => {
    if (!state.inputCaptured || state.activePasteShortcut) return;
    void triggerRemotePasteShortcut();
  });
  window.addEventListener("copy", () => {
    handleClipboardShortcut("c", async () => scheduleRemoteClipboardRefresh([180, 500, 1000, 1800]));
  });
  window.addEventListener("cut", () => {
    handleClipboardShortcut("x", async () => scheduleRemoteClipboardRefresh([180, 500, 1000, 1800]));
  });
  window.addEventListener("keyup", (event) => {
    const key = normalizeKey(event);
    if (!key) return;
    if (state.suppressedKeyups.delete(key)) {
      event.preventDefault();
      return;
    }
    const released = releasePressedKey(key);
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
  window.addEventListener("blur", releaseInput);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      releaseInput();
      return;
    }
    void refreshLocalClipboard();
    requestRemoteClipboard();
  });
}

async function registerServiceWorker() {
  if (!("serviceWorker" in navigator)) return;
  try {
    const reg = await navigator.serviceWorker.register("/sw.js");
    setTimeout(() => reg.update(), 10000);
  } catch (error) {
    showToast("sw_register_failed", error.message || String(error));
  }
}

updateClipboardState("local", state.localClipboard);
updateClipboardState("remote", state.remoteClipboard);
applySettings(loadStoredSettings());
initControls();
registerServiceWorker();
state.passwd = loadStoredPassword();
authInput.value = state.passwd;
if (state.passwd) {
  clearAuthPrompt();
  void connect();
} else {
  setAuthPrompt();
}
