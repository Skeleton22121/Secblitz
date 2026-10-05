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
})();
