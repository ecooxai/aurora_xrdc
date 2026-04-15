let canvas = null;
let ctx = null;
let pendingFrame = null;
let renderingFrame = false;
let renderHandle = 0;

const scheduleFrame = typeof self.requestAnimationFrame === "function"
  ? (callback) => self.requestAnimationFrame(callback)
  : (callback) => self.setTimeout(callback, 16);

const cancelScheduledFrame = typeof self.cancelAnimationFrame === "function"
  ? (handle) => self.cancelAnimationFrame(handle)
  : (handle) => self.clearTimeout(handle);

function clearPendingFrame() {
  if (!pendingFrame) return;
  pendingFrame.close();
  pendingFrame = null;
}

function cancelRender() {
  if (!renderHandle) return;
  cancelScheduledFrame(renderHandle);
  renderHandle = 0;
}

function resizeCanvas(width, height) {
  if (!canvas) return;
  if (canvas.width === width && canvas.height === height) return;
  canvas.width = width;
  canvas.height = height;
}

function scheduleRender() {
  if (renderHandle || renderingFrame || !pendingFrame || !ctx) return;
  renderHandle = scheduleFrame(() => {
    renderHandle = 0;
    void renderLatestFrame();
  });
}

async function renderLatestFrame() {
  if (renderingFrame || !ctx) return;
  const frame = pendingFrame;
  if (!frame) return;
  pendingFrame = null;
  renderingFrame = true;
  try {
    resizeCanvas(frame.displayWidth, frame.displayHeight);
    try {
      ctx.drawImage(frame, 0, 0, canvas.width, canvas.height);
    } catch {
      const bitmap = await createImageBitmap(frame);
      ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
      bitmap.close();
    }
  } catch (error) {
    self.postMessage({
      type: "error",
      code: "video_worker_render_failed",
      message: error?.message || String(error),
    });
  } finally {
    frame.close();
    renderingFrame = false;
    if (pendingFrame) {
      scheduleRender();
    }
  }
}

self.onmessage = (event) => {
  const message = event.data;
  if (!message || typeof message !== "object") return;

  switch (message.type) {
    case "init":
      canvas = message.canvas || null;
      ctx = canvas?.getContext("2d", { alpha: false, desynchronized: true }) || null;
      if (!ctx) {
        self.postMessage({
          type: "error",
          code: "video_worker_init_failed",
          message: "OffscreenCanvas 2D context is unavailable",
        });
      }
      break;
    case "resize":
      resizeCanvas(message.width, message.height);
      break;
    case "clear":
      cancelRender();
      clearPendingFrame();
      break;
    case "frame":
      if (!ctx || !message.frame) {
        message.frame?.close?.();
        return;
      }
      if (pendingFrame) {
        pendingFrame.close();
      }
      pendingFrame = message.frame;
      scheduleRender();
      break;
    default:
      break;
  }
};
