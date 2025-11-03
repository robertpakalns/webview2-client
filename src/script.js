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
