"use strict";
// Runs before the page paints so a saved theme never flashes the wrong one.
try {
  const saved = localStorage.getItem("secblitz-theme");
  if (saved === "light" || saved === "dark") document.documentElement.dataset.theme = saved;
} catch {
  // Storage blocked: follow the system setting.
}
