import type { Settings } from "./types";

const DEFAULT_ACCENT = "#7c8cff";
const AQUA_ACCENT = "#0891b2";
const MIDNIGHT_ACCENT = "#e8b04a";

export function resolvedTheme(theme: Settings["theme"]): "dark" | "light" | "aqua" | "midnight" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  return theme;
}

/** Applies appearance settings as CSS custom properties / data attributes on <html>. */
export function applyTheme(s: Settings) {
  const root = document.documentElement;
  root.dataset.theme = resolvedTheme(s.theme);
  const prefersReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  root.dataset.motion = s.animationLevel === "full" && prefersReduced ? "reduced" : s.animationLevel;
  // Aqua and Midnight each pair with their own accent unless the user picked one.
  const themeAccent = s.theme === "aqua" ? AQUA_ACCENT : s.theme === "midnight" ? MIDNIGHT_ACCENT : null;
  const accent = themeAccent && s.accentColor === DEFAULT_ACCENT ? themeAccent : s.accentColor;
  root.style.setProperty("--accent", accent);
  root.style.setProperty("--accent-contrast", contrastText(accent));
  root.style.setProperty("--surface-alpha", String(s.surfaceOpacity));
  // No desktop blur exists behind the webview, so the expanded panel stays near-opaque for legibility.
  root.style.setProperty("--panel-alpha", String(Math.min(1, s.surfaceOpacity + 0.12)));
  root.style.setProperty("--radius", `${s.cornerRadius}px`);
  root.style.setProperty("--dock-scale", String(s.dockScale));
}

/** Black or white, whichever reads better on `hex`. */
export function contrastText(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex);
  if (!m) return "#ffffff";
  const n = parseInt(m[1]!, 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.45 ? "#0b0b10" : "#ffffff";
}

export function motionMs(level: Settings["animationLevel"], full: number): number {
  if (level === "none") return 0;
  if (level === "reduced" || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return Math.round(full * 0.5);
  return full;
}
