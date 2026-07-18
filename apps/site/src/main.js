const root = document.documentElement;
const darkMedia = window.matchMedia("(prefers-color-scheme: dark)");
const themeColor = document.querySelector('meta[name="theme-color"]');
const themeButtons = Array.from(document.querySelectorAll(".theme-toggle"));

function effectiveTheme() {
  if (root.dataset.theme === "light" || root.dataset.theme === "dark")
    return root.dataset.theme;
  return darkMedia.matches ? "dark" : "light";
}

function renderThemeControl() {
  const current = effectiveTheme();
  const next = current === "dark" ? "light" : "dark";

  for (const button of themeButtons) {
    button.textContent = next === "dark" ? "Dark" : "Light";
    button.setAttribute("aria-label", `Switch to ${next} theme`);
  }

  themeColor?.setAttribute(
    "content",
    current === "dark" ? "#0c131e" : "#f7f9fc",
  );
}

function saveTheme(theme) {
  root.dataset.theme = theme;
  try {
    localStorage.setItem("oore-site-theme", theme);
  } catch {}
  renderThemeControl();
}

for (const button of themeButtons) {
  button.addEventListener("click", () => {
    saveTheme(effectiveTheme() === "dark" ? "light" : "dark");
  });
}

darkMedia.addEventListener("change", renderThemeControl);
renderThemeControl();

const menuButton = document.querySelector(".menu-toggle");
const navigation = document.querySelector("#site-nav");
const desktopMedia = window.matchMedia("(min-width: 821px)");

function setMenu(open) {
  if (!menuButton || !navigation) return;
  menuButton.setAttribute("aria-expanded", open ? "true" : "false");
  menuButton.textContent = open ? "Close" : "Menu";
  navigation.dataset.open = open ? "true" : "false";
}

menuButton?.addEventListener("click", () => {
  setMenu(menuButton.getAttribute("aria-expanded") !== "true");
});

navigation?.addEventListener("click", (event) => {
  if (event.target instanceof HTMLAnchorElement) setMenu(false);
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") setMenu(false);
});

desktopMedia.addEventListener("change", (event) => {
  if (event.matches) setMenu(false);
});

async function copyToClipboard(text) {
  if (navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(text);
    return;
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.readOnly = true;
  textarea.style.position = "fixed";
  textarea.style.inset = "-1000px auto auto -1000px";
  document.body.appendChild(textarea);
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  if (!copied) throw new Error("Clipboard copy failed");
}

document.addEventListener("click", async (event) => {
  const button =
    event.target instanceof Element
      ? event.target.closest("[data-copy-target]")
      : null;
  if (!(button instanceof HTMLButtonElement)) return;

  const targetId = button.dataset.copyTarget;
  const target = targetId ? document.getElementById(targetId) : null;
  const text = target?.textContent?.trim();
  if (!text) return;

  const original = button.textContent;
  button.disabled = true;

  try {
    await copyToClipboard(text);
    button.textContent = "Copied";
    button.dataset.state = "success";
  } catch {
    button.textContent = "Copy failed";
    button.dataset.state = "error";
  }

  window.setTimeout(() => {
    button.textContent = original;
    button.disabled = false;
    delete button.dataset.state;
  }, 1500);
});

function loadAnalytics() {
  const token = import.meta.env.VITE_CF_WEB_ANALYTICS_TOKEN;
  if (!token || navigator.doNotTrack === "1") return;

  const script = document.createElement("script");
  script.defer = true;
  script.src = "https://static.cloudflareinsights.com/beacon.min.js";
  script.dataset.cfBeacon = JSON.stringify({ token });
  document.head.appendChild(script);
}

window.addEventListener(
  "load",
  () => {
    if ("requestIdleCallback" in window) {
      window.requestIdleCallback(loadAnalytics, { timeout: 3000 });
      return;
    }
    window.setTimeout(loadAnalytics, 1200);
  },
  { once: true },
);
