"use strict";

(() => {
  const video = document.getElementById("demo-video");
  const recording = document.getElementById("recording");
  const still = document.getElementById("video-still");
  const motion = matchMedia("(prefers-reduced-motion: reduce)");
  const connection = navigator.connection;
  /** @type {{ready: boolean, visible: boolean, pending: boolean, blocked: boolean}} */
  const state = { ready: false, visible: false, pending: false, blocked: false };
  video.muted = true;
  video.defaultMuted = true;
  const automatic = () => !motion.matches && !connection?.saveData;
  const allowed = () => state.ready && state.visible && !document.hidden && automatic() && !state.blocked;

  async function sync() {
    // Set autoplay only after checking preferences, never during HTML parsing.
    video.autoplay = allowed();
    still.hidden = automatic() && !state.blocked;
    if (!allowed()) {
      video.pause();
      return;
    }
    if (state.pending || !video.paused) return;
    state.pending = true;
    let interrupted = false;
    try {
      await video.play();
    } catch (error) {
      interrupted = error.name === "AbortError";
      if (!interrupted && allowed()) {
        state.blocked = true;
        video.autoplay = false;
        still.hidden = false;
      }
    } finally {
      state.pending = false;
      if (!allowed()) video.pause();
      // A visibility change can cancel an in-flight play request.
      if (interrupted && allowed()) sync();
    }
  }

  video.addEventListener("loadedmetadata", () => {
    state.ready = true;
    recording.hidden = false;
    document.getElementById("how").classList.add("has-recording");
    sync();
  });
  video.addEventListener("error", () => {
    state.ready = false;
    video.pause();
    recording.hidden = true;
    document.getElementById("how").classList.remove("has-recording");
  });
  video.addEventListener("play", () => {
    if (!allowed()) video.pause();
  });
  document.addEventListener("visibilitychange", sync);
  motion.addEventListener("change", sync);
  connection?.addEventListener("change", sync);
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(entries => {
      state.visible = entries[0].isIntersecting && entries[0].intersectionRatio >= 0.01;
      sync();
    }, { threshold: 0.01 }).observe(video);
  } else {
    const checkVisibility = () => {
      const bounds = video.getBoundingClientRect();
      state.visible = bounds.width > 0 && bounds.height > 0 && bounds.bottom > 0 &&
        bounds.top < innerHeight && bounds.right > 0 && bounds.left < innerWidth;
      sync();
    };
    addEventListener("scroll", checkVisibility, { passive: true });
    addEventListener("resize", checkVisibility);
    video.addEventListener("loadedmetadata", checkVisibility);
    checkVisibility();
  }
  // Metadata may already be cached before the deferred script runs.
  if (video.readyState >= 1) video.dispatchEvent(new Event("loadedmetadata"));

  const copy = document.getElementById("copy-sha");
  copy.addEventListener("click", async () => {
    const text = document.getElementById(copy.dataset.copy).textContent;
    try {
      await navigator.clipboard.writeText(text);
      copy.textContent = "Copied";
    } catch {
      getSelection().selectAllChildren(document.getElementById(copy.dataset.copy));
      copy.textContent = "Selected";
    }
    setTimeout(() => { copy.textContent = "Copy"; }, 2000);
  });
})();
