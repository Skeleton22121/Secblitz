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

  // ---- Copy an installer checksum ----
  for (const copy of document.querySelectorAll("button[data-copy]")) {
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

  // ---- Offer the ARM installer to PCs with an ARM processor ----
  const arm = document.querySelector('a[data-arch="arm64"]');
  const high = navigator.userAgentData && navigator.userAgentData.getHighEntropyValues;
  if (arm && high) {
    navigator.userAgentData.getHighEntropyValues(["architecture"])
      .then(({ architecture }) => {
        if (architecture !== "arm") return;
        for (const button of document.querySelectorAll("a[data-auto-download]")) {
          button.href = arm.href;
        }
      })
      .catch(() => {
        // The button keeps the installer for most PCs; both links stay visible.
      });
  }

  // ---- Download count ----
  const downloads = document.getElementById("downloads");
  if (downloads) {
    fetch("/api/downloads")
      .then((response) => (response.ok ? response.json() : Promise.reject()))
      .then(({ total }) => {
        if (!Number.isSafeInteger(total) || total < 1) return;
        downloads.querySelector("span").textContent = total.toLocaleString("en-US");
        downloads.hidden = false;
      })
      .catch(() => {
        // Not shown; the page works without it.
      });
  }
})();
