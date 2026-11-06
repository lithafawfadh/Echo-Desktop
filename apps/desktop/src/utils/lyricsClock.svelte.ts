export function createLyricsClock() {
  let anchorPositionMs = $state(0);
  let anchorPerfTime = $state(performance.now());
  let isPlaying = $state(false);
  let playbackRate = $state(1.0);

  let currentMs = $state(0);
  let frameHandle: number | null = null;

  function updateAnchor(posMs: number, playing: boolean, rate: number = 1.0) {
    anchorPositionMs = posMs;
    anchorPerfTime = performance.now();
    isPlaying = playing;
    playbackRate = rate;
    currentMs = posMs;
  }

  function loop() {
    if (isPlaying && !document.hidden) {
      const elapsed = performance.now() - anchorPerfTime;
      currentMs = anchorPositionMs + elapsed * playbackRate;
    }
    frameHandle = requestAnimationFrame(loop);
  }

  function start() {
    if (frameHandle !== null) return;
    frameHandle = requestAnimationFrame(loop);
  }

  function stop() {
    if (frameHandle !== null) {
      cancelAnimationFrame(frameHandle);
      frameHandle = null;
    }
  }

  return {
    get currentMs() {
      return currentMs;
    },
    updateAnchor,
    start,
    stop,
  };
}
