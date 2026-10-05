"use strict";

(() => {
  const root = document.documentElement;

  // ---- Theme: follows the system until the visitor picks one ----
  const toggle = document.getElementById("theme-toggle");
  const systemDark = matchMedia("(prefers-color-scheme: dark)");
  const effective = () => root.dataset.theme || (systemDark.matches ? "dark" : "light");
  const label = () => {
    toggle.setAttribute("aria-label", effective() === "dark" ? "Switch to light mode" : "Switch to dark mode");
  };
  if (toggle) {
    toggle.hidden = false;
    label();
    toggle.addEventListener("click", () => {
      const next = effective() === "dark" ? "light" : "dark";
      root.dataset.theme = next;
      try {
        localStorage.setItem("secblitz-theme", next);
      } catch {
        // Not saved; the choice still applies to this visit.
      }
      label();
    });
    systemDark.addEventListener("change", label);
  }

  // ---- Copy the installer checksum ----
  const copy = document.getElementById("copy-sha");
  if (copy) {
    copy.addEventListener("click", async () => {
      const target = document.getElementById(copy.dataset.copy);
      try {
        await navigator.clipboard.writeText(target.textContent.trim());
        copy.textContent = "Copied";
      } catch {
        getSelection().selectAllChildren(target);
        copy.textContent = "Selected";
      }
      setTimeout(() => { copy.textContent = "Copy"; }, 2000);
    });
  }

  // ---- Optional demo video: plays only when visible, never on reduced motion or data saver ----
  const video = document.getElementById("demo-video");
  if (!video) return;
  const motion = matchMedia("(prefers-reduced-motion: reduce)");
  const connection = navigator.connection;
  const state = { ready: false, visible: false };
  video.muted = true;
  const allowed = () => state.ready && state.visible && !document.hidden && !motion.matches && !connection?.saveData;
  // The themed still underneath stays visible until the recording really plays,
  // and comes back for reduced motion, Save-Data or a media error.
  const still = () => motion.matches || connection?.saveData;
  const sync = () => {
    if (allowed()) video.play().catch(() => {});
    else {
      video.pause();
      if (still()) video.hidden = true;
    }
  };
  video.addEventListener("playing", () => { if (!still()) video.hidden = false; });
  video.addEventListener("error", () => { video.hidden = true; }, true);
  video.addEventListener("loadedmetadata", () => { state.ready = true; sync(); });
  document.addEventListener("visibilitychange", sync);
  motion.addEventListener("change", sync);
  new IntersectionObserver(entries => {
    state.visible = entries[0].isIntersecting;
    sync();
  // Watch the frame, not the video: the video stays hidden until it plays.
  }, { threshold: 0.25 }).observe(video.parentElement);
  if (video.readyState >= 1) video.dispatchEvent(new Event("loadedmetadata"));
})();
