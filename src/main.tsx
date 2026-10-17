import { getCurrentWindow } from "@tauri-apps/api/window";
import { error as logError, warn as logWarn } from "@tauri-apps/plugin-log";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { SettingsProvider } from "./lib/hooks";
import { DockApp } from "./dock/DockApp";
import { SettingsApp } from "./settings/SettingsApp";
import { MusicWidget } from "./widget/MusicWidget";

const label = getCurrentWindow().label;
const isDock = label === "dock";
const isWidget = label === "widget";
// Dock and widget windows are transparent; only their content paints.
document.documentElement.classList.add(isDock || isWidget ? "is-dock" : "is-settings");

// Forward frontend problems to the local log file (never sent anywhere).
const where = label;
window.addEventListener("error", (e) => void logError(`[${where}] ${e.message} at ${e.filename}:${e.lineno}`).catch(() => {}));
window.addEventListener("unhandledrejection", (e) => {
  const reason = e.reason && typeof e.reason === "object" && "message" in e.reason ? String(e.reason.message) : String(e.reason);
  void logWarn(`[${where}] unhandled rejection: ${reason}`).catch(() => {});
});
const consoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  consoleError(...args);
  void logError(`[${where}] ${args.map((a) => (a instanceof Error ? a.message : String(a))).join(" ")}`).catch(() => {});
};

// The webview's own context menu (Reload, Inspect…) has no place in a desktop utility.
if (!import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => e.preventDefault());
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <SettingsProvider fallback={null}>{isDock ? <DockApp /> : isWidget ? <MusicWidget /> : <SettingsApp />}</SettingsProvider>
  </StrictMode>,
);
