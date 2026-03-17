const $ = (id) => document.getElementById(id);
const params = new URLSearchParams(window.location.search);
const state = {
  socket: null,
  decoder: null,
  writer: null,
  activeCodec: "h264",
  codecString: "avc1.64001f",
  description: null,
  decoderConfigKey: "",
  waitingForKeyframe: true,
  stats: {},
  frameCount: 0,
  bytesReceived: 0,
  lastNetAt: performance.now(),
  netKbps: 0,
  manualDisconnect: false,
  reconnectTimer: null,
  reconnectAttempt: 0,
  activePointerId: null,
  touchGesture: null,
  inputCaptured: false,
  pressedKeys: new Set(),
  pendingPointer: null,
  pointerRaf: 0,
  wheelAccumulator: 0,
};

const status = $("status");
const debug = $("debug");
const canvas = $("screen");
const ctx = canvas.getContext("2d", { alpha: false });
const toast = $("toast");
const settingsPanel = $("settings-panel");

function setStatus(text) {
  status.textContent = text;
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

async function connect() {
  closeConnection({ manual: false, preserveStatus: true });
  state.manualDisconnect = false;
  clearTimeout(state.reconnectTimer);
  const codec = $("codec").value;
  const bitrate = Number($("bitrate").value);
  const fps = Number($("fps").value);
  state.activeCodec = codec;
  state.frameCount = 0;
  state.bytesReceived = 0;
  const url = new URL("/ws", window.location.href);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("codec", codec);
  url.searchParams.set("bitrate_kbps", bitrate);
  url.searchParams.set("fps", fps);
  setStatus("Connecting...");
  state.socket = new WebSocket(url);
  state.socket.binaryType = "arraybuffer";
  state.socket.onopen = () => {
    state.reconnectAttempt = 0;
    setStatus("Connected");
    startPing();
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
}

function closeConnection({ manual = true, preserveStatus = false } = {}) {
  state.manualDisconnect = manual;
  clearTimeout(state.reconnectTimer);
  clearInterval(startPing.timer);
  state.decoder?.close();
  state.decoder = null;
  state.decoderConfigKey = "";
  state.waitingForKeyframe = true;
  resetKeys();
  if (state.socket) {
    const socket = state.socket;
    state.socket = null;
    socket.onclose = null;
    socket.close();
  }
  if (!preserveStatus) setStatus("Disconnected");
}

function disconnect() {
  closeConnection({ manual: true });
}

function captureInput() {
  state.inputCaptured = true;
  canvas.focus({ preventScroll: true });
}

function releaseInput() {
  state.inputCaptured = false;
  resetKeys();
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
    setupDecoder();
    setStatus(`${message.active_encoder || "ready"} ${message.encoder_mode || ""}`.trim());
  } else if (message.type === "stats") {
    state.stats = message;
    renderDebug();
  } else if (message.type === "error") {
    showToast(message.code, message.message);
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
  if (!state.decoder || state.decoder.state === "closed") return;
  const view = new DataView(buffer);
  const kind = view.getUint8(0);
  if (kind !== 1) return;
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

function renderDebug() {
  if (params.get("debug") !== "1") return;
  debug.classList.remove("hidden");
  const latency = state.stats.latency_ms ?? 0;
  debug.textContent = [
    `codec: ${state.stats.codec ?? state.activeCodec}`,
    `encoder: ${state.stats.active_encoder ?? "-"}`,
    `mode: ${state.stats.encoder_mode ?? "-"}`,
    `capture fps: ${state.stats.capture_fps ?? 0}`,
    `client frames: ${state.frameCount}`,
    `latency: ${latency} ms`,
    `cpu: ${(state.stats.cpu_usage ?? 0).toFixed(1)}%`,
    `memory: ${state.stats.memory_used_mb ?? 0} MB`,
    `server net: ${(state.stats.net_tx_kbps ?? 0).toFixed(1)} kbps`,
    `client net: ${state.netKbps.toFixed(1)} kbps`,
  ].join("\n");
}

function send(message) {
  if (state.socket?.readyState === WebSocket.OPEN) {
    state.socket.send(JSON.stringify(message));
  }
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

function pointerToCanvas(event) {
  const rect = canvas.getBoundingClientRect();
  if (!rect.width || !rect.height) return null;
  const x = Math.min(Math.max(event.clientX - rect.left, 0), rect.width);
  const y = Math.min(Math.max(event.clientY - rect.top, 0), rect.height);
  return {
    x: Math.round((x / rect.width) * canvas.width),
    y: Math.round((y / rect.height) * canvas.height),
  };
}

function initControls() {
  $("bitrate").addEventListener("input", (event) => {
    $("bitrate-value").textContent = `${event.target.value} kbps`;
  });
  $("fps").addEventListener("input", (event) => {
    $("fps-value").textContent = `${event.target.value} fps`;
  });
  $("scroll-speed").addEventListener("input", (event) => {
    $("scroll-speed-value").textContent = `${event.target.value} / 10`;
  });
  $("connect").addEventListener("click", () => connect());
  $("disconnect").addEventListener("click", disconnect);
  settingsPanel.addEventListener("toggle", () => {
    if (settingsPanel.open) {
      releaseInput();
    }
  });
  document.addEventListener("pointerdown", (event) => {
    if (event.target instanceof HTMLElement && event.target.closest("#settings-panel")) {
      releaseInput();
    }
  });
  canvas.addEventListener("pointerdown", (event) => {
    const point = pointerToCanvas(event);
    if (!point) return;
    captureInput();
    queuePointerMove(point.x, point.y);
    if (event.pointerType === "touch" || event.pointerType === "pen") {
      state.activePointerId = event.pointerId;
      state.touchGesture = {
        startX: point.x,
        startY: point.y,
        moved: false,
      };
      canvas.setPointerCapture(event.pointerId);
      event.preventDefault();
      return;
    }
    if (event.button === 0 || event.button === 2 || event.button === 1) {
      send({ type: "pointer_button", button: event.button + 1, down: true });
      event.preventDefault();
    }
  });
  canvas.addEventListener("pointermove", (event) => {
    const point = pointerToCanvas(event);
    if (!point) return;
    if (event.pointerType === "touch" || event.pointerType === "pen") {
      if (event.pointerId !== state.activePointerId) return;
      const gesture = state.touchGesture;
      if (gesture && (Math.abs(point.x - gesture.startX) > 8 || Math.abs(point.y - gesture.startY) > 8)) {
        gesture.moved = true;
      }
      event.preventDefault();
    }
    queuePointerMove(point.x, point.y);
  });
  canvas.addEventListener("pointerup", (event) => {
    if (event.pointerType === "touch" || event.pointerType === "pen") {
      if (event.pointerId !== state.activePointerId) return;
      const point = pointerToCanvas(event);
      if (point) queuePointerMove(point.x, point.y);
      if (state.touchGesture && !state.touchGesture.moved) {
        send({ type: "pointer_button", button: 1, down: true });
        send({ type: "pointer_button", button: 1, down: false });
      }
      state.activePointerId = null;
      state.touchGesture = null;
      event.preventDefault();
      return;
    }
    if (event.button === 0 || event.button === 2 || event.button === 1) {
      send({ type: "pointer_button", button: event.button + 1, down: false });
      event.preventDefault();
    }
  });
  canvas.addEventListener("pointercancel", (event) => {
    if (event.pointerId === state.activePointerId) {
      state.activePointerId = null;
      state.touchGesture = null;
    }
  });
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    queueWheel(event.deltaY);
  }, { passive: false });
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      releaseInput();
      return;
    }
    if (!shouldHandleKeyboard(event)) return;
    const key = normalizeKey(event);
    if (!key) return;
    if (!event.repeat && state.pressedKeys.has(key)) return;
    if (!event.repeat) state.pressedKeys.add(key);
    send({ type: "key", key, down: true });
    event.preventDefault();
  });
  window.addEventListener("keyup", (event) => {
    if (!shouldHandleKeyboard(event)) return;
    const key = normalizeKey(event);
    if (!key) return;
    state.pressedKeys.delete(key);
    send({ type: "key", key, down: false });
    event.preventDefault();
  });
  window.addEventListener("blur", releaseInput);
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) releaseInput();
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

initControls();
registerServiceWorker();
connect();
