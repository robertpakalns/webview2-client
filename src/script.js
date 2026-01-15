document.addEventListener(
  "DOMContentLoaded",
  () => {
    const _requestPointerLock = HTMLCanvasElement.prototype.requestPointerLock;
    HTMLCanvasElement.prototype.requestPointerLock = function (options) {
      return _requestPointerLock.call(this, {
        ...options,
        unadjustedMovement: true,
      });
    };
  },
  { once: true },
);

document.addEventListener("keydown", (e) => {
  if (e.key !== "F11") return;

  e.preventDefault();
  window.chrome.webview.postMessage("toggle_fullscreen");
});
