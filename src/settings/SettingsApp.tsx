import { getVersion } from "@tauri-apps/api/app";
import {
  Bell,
  Clipboard,
  Info,
  Keyboard,
  LayoutGrid,
  Palette,
  Power,
  ShieldCheck,
  SlidersHorizontal,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useLoader, useSettings, useTauriEvent } from "../lib/hooks";
import { formatBytes } from "../lib/format";
import type { Settings } from "../lib/types";
import { Button, ErrorNote, Segmented, SettingRow, Toggle, cx, inputClass } from "../components/ui";
import { Section } from "./Section";
import { SlotsSection } from "./SlotsSection";
import { Welcome } from "./Welcome";
import { ShortcutField } from "./ShortcutField";

const SECTIONS: Array<{ id: string; label: string; icon: LucideIcon }> = [
  { id: "general", label: "General", icon: SlidersHorizontal },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "slots", label: "Dock slots", icon: LayoutGrid },
  { id: "clipboard", label: "Clipboard", icon: Clipboard },
  { id: "apps", label: "Apps", icon: Keyboard },
  { id: "notifications", label: "Notifications", icon: Bell },
  { id: "privacy", label: "Privacy & data", icon: ShieldCheck },
  { id: "about", label: "About", icon: Info },
];

function sectionFromHash(): string {
  const m = /#\/settings\/([a-z-]+)/.exec(window.location.hash);
  return m?.[1] ?? "general";
}

export function SettingsApp() {
  const { settings } = useSettings();
  const [section, setSection] = useState(sectionFromHash);
  useTauriEvent<string>("settings:navigate", (s) => setSection(s));
  useTauriEvent("data:reset", () => setSection("welcome"));

  useEffect(() => {
    window.location.hash = `#/settings/${section}`;
  }, [section]);

  if (section === "welcome" || !settings.onboardingCompleted) {
    return <Welcome onDone={() => setSection("general")} />;
  }

  return (
    <div className="flex h-full">
      <nav className="w-52 shrink-0 border-r border-line p-3" aria-label="Settings sections">
        <div className="mb-3 px-2 text-sm font-semibold">Brim</div>
        <ul className="relative space-y-0.5">
          {/* Selection highlight that springs to the chosen section (items are h-8 + 2px gap). */}
          <span
            aria-hidden
            className="slide-indicator pointer-events-none absolute inset-x-0 top-0 h-8 rounded-lg bg-accent/15"
            style={{ transform: `translateY(${Math.max(0, SECTIONS.findIndex((x) => x.id === section)) * 34}px)` }}
          />
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <li key={id}>
              <button
                type="button"
                aria-current={section === id ? "page" : undefined}
                onClick={() => setSection(id)}
                className={cx(
                  "press press-soft relative flex h-8 w-full items-center gap-2 rounded-lg px-2 text-left text-[13px]",
                  section === id ? "text-fg" : "text-muted hover:bg-hover hover:text-fg",
                )}
              >
                <Icon className={cx("size-4 transition-colors duration-200", section === id && "text-accent")} aria-hidden />
                {label}
              </button>
            </li>
          ))}
        </ul>
      </nav>
      <main className="min-w-0 flex-1 overflow-y-auto">
        <div key={section} className="fade-in mx-auto max-w-2xl px-8 py-6">
          {section === "general" && <GeneralSection />}
          {section === "appearance" && <AppearanceSection />}
          {section === "slots" && <SlotsSection />}
          {section === "clipboard" && <ClipboardSection />}
          {section === "apps" && <AppsSection />}
          {section === "notifications" && <NotificationsSection />}
          {section === "privacy" && <PrivacySection />}
          {section === "about" && <AboutSection />}
        </div>
      </main>
    </div>
  );
}

/** Settings update with inline error reporting. */
function usePatch() {
  const { settings, update } = useSettings();
  const action = useAction();
  const patch = (p: Partial<Settings>) => action.run(() => update(p));
  return { settings, patch, action };
}

function GeneralSection() {
  const { settings, patch, action } = usePatch();
  const startup = useLoader(api.settings.startupGet);
  const startupAction = useAction();
  return (
    <>
      <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />
      <Section title="Opening the dock">
        <SettingRow label="Keyboard shortcut" description="Opens or closes the dock from anywhere.">
          <ShortcutField />
        </SettingRow>
        <Toggle label="Expand when scrolling over the dock" checked={settings.expandOnScroll} onChange={(v) => patch({ expandOnScroll: v })} />
        <SettingRow label="Tray icon click" description="What a left-click on the tray icon does.">
          <Segmented
            label="Tray icon click"
            value={settings.trayClickAction}
            onChange={(v) => patch({ trayClickAction: v })}
            options={[
              { value: "toggle", label: "Open dock" },
              { value: "settings", label: "Settings" },
            ]}
          />
        </SettingRow>
      </Section>
      <Section title="Live activities" description="The dock grows from its centre for a moment to show what changed, then folds back. Never shown over full-screen apps.">
        <Toggle
          label="Show the song when music starts"
          checked={settings.showNowPlaying}
          onChange={(v) => patch({ showNowPlaying: v })}
        />
        <Toggle
          label="Show volume changes"
          description="Appears when the volume changes from the keyboard or another app."
          checked={settings.showVolumeHud}
          onChange={(v) => patch({ showVolumeHud: v })}
        />
        <Toggle
          label="Handle the volume keys"
          description="The keyboard volume keys change the volume through Brim, so Windows' own volume pop-up no longer appears."
          checked={settings.handleVolumeKeys}
          onChange={(v) => patch({ handleVolumeKeys: v })}
        />
      </Section>
      <Section title="Desktop music widget" description="A card on your desktop showing what's playing, with playback controls. Drag it anywhere; it remembers its spot.">
        <Toggle label="Show the music widget" checked={settings.musicWidget} onChange={(v) => patch({ musicWidget: v })} />
        {settings.musicWidget && (
          <>
            <SettingRow label="Style">
              <Segmented
                label="Widget style"
                value={settings.musicWidgetStyle}
                onChange={(v) => patch({ musicWidgetStyle: v })}
                options={[
                  { value: "card", label: "Card" },
                  { value: "compact", label: "Compact" },
                ]}
              />
            </SettingRow>
            <Toggle
              label="Hide when nothing is playing"
              checked={settings.musicWidgetHideIdle}
              onChange={(v) => patch({ musicWidgetHideIdle: v })}
            />
            <Toggle
              label="Keep above other windows"
              description="Off: it sits on the desktop behind your windows."
              checked={settings.musicWidgetOnTop}
              onChange={(v) => patch({ musicWidgetOnTop: v })}
            />
          </>
        )}
      </Section>
      <Section title="Behaviour">
        <Toggle
          label="Keep the dock above other windows"
          checked={settings.alwaysOnTop}
          onChange={(v) => patch({ alwaysOnTop: v })}
        />
        <Toggle
          label="Hide while a full-screen app is in front"
          description="Games, videos and presentations on the dock’s monitor won’t be covered. The shortcut still opens it."
          checked={settings.hideOnFullscreen}
          onChange={(v) => patch({ hideOnFullscreen: v })}
        />
        <Toggle label="Close when clicking outside" checked={settings.collapseOnBlur} onChange={(v) => patch({ collapseOnBlur: v })} />
        <Toggle
          label="Close after copying from clipboard or snippets"
          description="Focus returns to the app you were using so you can paste right away."
          checked={settings.collapseAfterCopy}
          onChange={(v) => patch({ collapseAfterCopy: v })}
        />
        <Toggle label="Close after launching an app" checked={settings.collapseAfterLaunch} onChange={(v) => patch({ collapseAfterLaunch: v })} />
        <Toggle
          label="Reopen the last used utility"
          checked={settings.restoreLastUtility}
          onChange={(v) => patch({ restoreLastUtility: v })}
        />
        <SettingRow label="When idle, the compact dock is" description={
            settings.compactVisibility === "vanish"
              ? "Collapses to a point and disappears. Hover where it was to bring it back."
              : settings.compactVisibility === "line"
                ? "Collapses into a thin line. Hover the line to bring the dock back."
                : undefined
          }>
          <Segmented
            label="Compact dock visibility"
            value={settings.compactVisibility}
            onChange={(v) => patch({ compactVisibility: v })}
            options={[
              { value: "vanish", label: "Vanish" },
              { value: "line", label: "Line" },
              { value: "always", label: "Visible" },
              { value: "fade", label: "Faded" },
              { value: "hidden", label: "Hidden" },
            ]}
          />
        </SettingRow>
        {settings.compactVisibility === "hidden" && (
          <p className="py-2 text-xs text-warn">The dock only appears when you press {settings.shortcut} or use the tray icon.</p>
        )}
      </Section>
      <Section title="Startup">
        <Toggle
          label="Launch Brim when I sign in to Windows"
          description={startup.data === undefined ? "Checking…" : startup.data ? "Enabled" : "Disabled"}
          checked={!!startup.data}
          disabled={startup.data === undefined || startupAction.busy}
          onChange={(v) => startupAction.run(() => api.settings.startupSet(v)).then((r) => r !== undefined && startup.setData(r))}
        />
        <ErrorNote error={startupAction.error ?? startup.error} />
        <SettingRow label="Quit Brim" description="Closes the dock and stops clipboard capture until you start it again.">
          <Button icon={Power} variant="danger" onClick={() => api.dock.quit()}>
            Quit
          </Button>
        </SettingRow>
      </Section>
    </>
  );
}

const ACCENTS = ["#7c8cff", "#a45cff", "#ff6b9a", "#ff8a4c", "#f5c542", "#3ecf8e", "#38bdf8", "#94a3b8"];

function AppearanceSection() {
  const { settings, patch, action } = usePatch();
  return (
    <>
      <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />
      <Section title="Theme">
        <SettingRow label="Colour mode">
          <Segmented
            label="Colour mode"
            value={settings.theme}
            onChange={(v) => patch({ theme: v })}
            options={[
              { value: "dark", label: "Dark" },
              { value: "light", label: "Light" },
              { value: "aqua", label: "Aqua" },
              { value: "midnight", label: "Midnight" },
              { value: "system", label: "System" },
            ]}
          />
        </SettingRow>
        <SettingRow label="Accent colour">
          <div className="flex items-center gap-1.5">
            {ACCENTS.map((c) => (
              <button
                key={c}
                type="button"
                aria-label={`Accent ${c}`}
                aria-pressed={settings.accentColor === c}
                onClick={() => patch({ accentColor: c })}
                className={cx("size-6 rounded-full ring-offset-2 ring-offset-[var(--surface-solid)]", settings.accentColor === c && "ring-2 ring-fg")}
                style={{ background: c }}
              />
            ))}
            <input
              type="color"
              aria-label="Custom accent colour"
              value={settings.accentColor}
              onChange={(e) => patch({ accentColor: e.target.value })}
              className="size-7 cursor-default rounded border-0 bg-transparent p-0"
            />
          </div>
        </SettingRow>
      </Section>
      <Section title="Dock">
        <SettingRow label="Layout" description="Wide shows text tabs across a wider panel; Compact keeps the four slot icons.">
          <Segmented
            label="Layout"
            value={settings.panelStyle}
            onChange={(v) => patch({ panelStyle: v })}
            options={[
              { value: "wide", label: "Wide" },
              { value: "compact", label: "Compact" },
            ]}
          />
        </SettingRow>
        <SettingRow label="Size">
          <Segmented
            label="Dock size"
            value={settings.dockScale}
            onChange={(v) => patch({ dockScale: v })}
            options={[
              { value: 0.9, label: "Small" },
              { value: 1, label: "Default" },
              { value: 1.15, label: "Large" },
            ]}
          />
        </SettingRow>
        <SettingRow label="Opacity" description={`${Math.round(settings.surfaceOpacity * 100)}%`}>
          <input
            type="range"
            aria-label="Opacity"
            min={0.6}
            max={1}
            step={0.02}
            value={settings.surfaceOpacity}
            onChange={(e) => patch({ surfaceOpacity: Number(e.target.value) })}
            className="w-44 accent-[var(--accent)]"
          />
        </SettingRow>
        <SettingRow label="Corner radius" description={`${settings.cornerRadius}px`}>
          <input
            type="range"
            aria-label="Corner radius"
            min={8}
            max={28}
            step={1}
            value={settings.cornerRadius}
            onChange={(e) => patch({ cornerRadius: Number(e.target.value) })}
            className="w-44 accent-[var(--accent)]"
          />
        </SettingRow>
        <SettingRow label="Animations" description="Reduced motion is also respected from Windows settings.">
          <Segmented
            label="Animations"
            value={settings.animationLevel}
            onChange={(v) => patch({ animationLevel: v })}
            options={[
              { value: "full", label: "Full" },
              { value: "reduced", label: "Reduced" },
              { value: "none", label: "Off" },
            ]}
          />
        </SettingRow>
      </Section>
    </>
  );
}

function ClipboardSection() {
  const { settings, patch, action } = usePatch();
  const [ignored, setIgnored] = useState(settings.clipboardIgnoredApps.join("\n"));
  const [confirm, setConfirm] = useState<null | "unpinned" | "all">(null);
  const clearAction = useAction();
  useEffect(() => setIgnored(settings.clipboardIgnoredApps.join("\n")), [settings.clipboardIgnoredApps]);

  return (
    <>
      <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />
      <Section
        title="Clipboard history"
        description="Text you copy is stored only on this PC. It is never uploaded or synced by Brim."
      >
        <SettingRow label="Capture">
          <Segmented
            label="Capture"
            value={settings.clipboardCapture}
            onChange={(v) => patch({ clipboardCapture: v, clipboardPausedUntil: null })}
            options={[
              { value: "on", label: "On" },
              { value: "paused", label: "Paused" },
              { value: "off", label: "Off" },
            ]}
          />
        </SettingRow>
        <SettingRow label="Keep items for" description="Pinned items are never removed automatically.">
          <select
            aria-label="Keep items for"
            className={cx(inputClass, "w-36")}
            value={settings.clipboardRetentionDays}
            onChange={(e) => patch({ clipboardRetentionDays: Number(e.target.value) })}
          >
            <option value={1}>1 day</option>
            <option value={7}>7 days</option>
            <option value={30}>30 days</option>
            <option value={90}>90 days</option>
            <option value={0}>No time limit</option>
          </select>
        </SettingRow>
        <SettingRow label="Maximum items" description="Oldest unpinned items are removed first. Total history is also capped at 25 MB.">
          <select
            aria-label="Maximum items"
            className={cx(inputClass, "w-36")}
            value={settings.clipboardMaxItems}
            onChange={(e) => patch({ clipboardMaxItems: Number(e.target.value) })}
          >
            {[50, 200, 500, 1000].map((n) => (
              <option key={n} value={n}>
                {n} items
              </option>
            ))}
          </select>
        </SettingRow>
        <SettingRow label="Largest item to keep">
          <select
            aria-label="Largest item to keep"
            className={cx(inputClass, "w-36")}
            value={settings.clipboardMaxEntryKb}
            onChange={(e) => patch({ clipboardMaxEntryKb: Number(e.target.value) })}
          >
            <option value={64}>64 KB</option>
            <option value={256}>256 KB</option>
            <option value={1024}>1 MB</option>
            <option value={2048}>2 MB</option>
          </select>
        </SettingRow>
      </Section>
      <Section title="Sensitive content">
        <Toggle
          label="Skip text that looks like a password, key or token"
          description="Best effort only — detection is not perfect. Pause capture before copying anything sensitive."
          checked={settings.clipboardSkipSecrets}
          onChange={(v) => patch({ clipboardSkipSecrets: v })}
        />
        <div className="py-3">
          <label htmlFor="ignored-apps" className="text-[13px] font-medium">
            Never save copies from these programs
          </label>
          <p className="mb-2 mt-0.5 text-xs text-muted">
            One program file name per line (for example keepass.exe). Copies that apps mark as private are always skipped.
          </p>
          <textarea
            id="ignored-apps"
            rows={5}
            value={ignored}
            onChange={(e) => setIgnored(e.target.value)}
            className={cx(inputClass, "h-auto py-2 font-mono text-xs")}
          />
          <div className="mt-2 flex justify-end">
            <Button
              small
              variant="primary"
              disabled={ignored === settings.clipboardIgnoredApps.join("\n")}
              onClick={() => patch({ clipboardIgnoredApps: ignored.split(/\r?\n/) })}
            >
              Save list
            </Button>
          </div>
        </div>
      </Section>
      <Section title="Clear history">
        <ErrorNote error={clearAction.error} />
        <SettingRow label="Delete clipboard history" description="This cannot be undone.">
          {confirm ? (
            <div className="flex gap-1.5">
              <Button
                variant="danger"
                busy={clearAction.busy}
                onClick={() => clearAction.run(() => api.clipboard.clear(confirm === "all")).finally(() => setConfirm(null))}
              >
                {confirm === "all" ? "Delete everything" : "Delete unpinned"}
              </Button>
              <Button variant="ghost" onClick={() => setConfirm(null)}>
                Cancel
              </Button>
            </div>
          ) : (
            <div className="flex gap-1.5">
              <Button onClick={() => setConfirm("unpinned")}>Unpinned items</Button>
              <Button variant="danger" onClick={() => setConfirm("all")}>
                Everything
              </Button>
            </div>
          )}
        </SettingRow>
      </Section>
    </>
  );
}

function AppsSection() {
  const { settings, patch, action } = usePatch();
  const [done, setDone] = useState<string | null>(null);
  const run = (label: string, fn: () => Promise<void>) => action.run(fn).then(() => setDone(label));
  return (
    <>
      <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />
      <Section title="App launcher" description="Apps are found from your Start menu. Launch history stays on this PC.">
        <Toggle
          label="Show recently launched apps"
          description="Only launches made from the dock are recorded."
          checked={settings.trackRecentApps}
          onChange={(v) => patch({ trackRecentApps: v })}
        />
        <SettingRow label="Rescan installed apps">
          <Button onClick={() => run("Scanning…", () => api.apps.refresh())}>Rescan</Button>
        </SettingRow>
        <SettingRow label="Clear recent apps">
          <Button onClick={() => run("Recent apps cleared.", () => api.apps.clearRecent())}>Clear</Button>
        </SettingRow>
        <SettingRow label="Reset launcher" description="Removes pins, hidden apps, recents and apps you added.">
          <Button variant="danger" onClick={() => run("Launcher reset.", () => api.apps.reset())}>
            Reset
          </Button>
        </SettingRow>
      </Section>
      {done && <p className="text-xs text-ok">{done}</p>}
    </>
  );
}

function NotificationsSection() {
  const { settings, patch, action } = usePatch();
  const status = useLoader(api.settings.notificationStatus);
  const test = useAction();
  const blocked = status.data && status.data.state !== "enabled" && status.data.state !== "unknown";
  return (
    <>
      <ErrorNote error={action.error ?? test.error} onDismiss={() => action.setError(null)} />
      <Section title="Reminders">
        <Toggle
          label="Show Windows notifications for reminders"
          description="Each reminder notifies once. Dismissed reminders never notify again."
          checked={settings.notificationsEnabled}
          onChange={(v) => patch({ notificationsEnabled: v })}
        />
        {blocked && (
          <p className="py-2 text-xs text-warn">
            Windows is blocking notifications for this app. Reminders still appear in the Tasks panel; turn notifications on in
            Windows Settings › System › Notifications to see pop-ups.
          </p>
        )}
        <SettingRow label="Send a test notification">
          <Button busy={test.busy} onClick={() => test.run(() => api.settings.notificationTest())}>
            Test
          </Button>
        </SettingRow>
        <p className="py-2 text-xs text-muted">Reminders are delivered while Brim is running. Ones missed while it was closed appear once when it starts (if less than a day late).</p>
      </Section>
    </>
  );
}

function PrivacySection() {
  const info = useLoader(api.settings.storageInfo, ["clipboard:changed", "wallpapers:changed"]);
  const action = useAction();
  const [typed, setTyped] = useState("");
  return (
    <>
      <Section title="Your data stays on this PC">
        <ul className="list-disc space-y-1 py-3 pl-5 text-xs text-muted">
          <li>No account, cloud service or AI provider is used.</li>
          <li>Clipboard history, tasks, snippets and app history are stored in a local database.</li>
          <li>Brim collects no telemetry.</li>
        </ul>
      </Section>
      <Section title="Storage">
        {info.data ? (
          <>
            <SettingRow label="Location" description={<span className="break-all font-mono">{info.data.dataDir}</span>}>
              <Button onClick={() => action.run(() => api.settings.openStorageFolder())}>Open folder</Button>
            </SettingRow>
            <SettingRow label="Database" description={`Clipboard: ${info.data.clipboard.count} items`}>
              <span className="text-xs text-muted">{formatBytes(info.data.databaseBytes)}</span>
            </SettingRow>
            <SettingRow label="Wallpapers and thumbnails">
              <span className="text-xs text-muted">{formatBytes(info.data.wallpaperBytes)}</span>
            </SettingRow>
            <SettingRow label="Icon cache">
              <span className="text-xs text-muted">{formatBytes(info.data.cacheBytes)}</span>
            </SettingRow>
          </>
        ) : (
          <ErrorNote error={info.error} />
        )}
      </Section>
      <Section title="Delete all local data" description="Removes clipboard history, tasks, reminders, events, snippets, wallpapers, app history and settings, then restarts setup. This cannot be undone.">
        <div className="py-3">
          <label htmlFor="confirm-delete" className="text-xs text-muted">
            Type DELETE to confirm
          </label>
          <div className="mt-1.5 flex gap-2">
            <input id="confirm-delete" value={typed} onChange={(e) => setTyped(e.target.value)} className={cx(inputClass, "w-40")} />
            <Button
              variant="danger"
              disabled={typed !== "DELETE"}
              busy={action.busy}
              onClick={() => action.run(() => api.settings.deleteAllData()).then(() => setTyped(""))}
            >
              Delete all data
            </Button>
          </div>
          <div className="mt-2">
            <ErrorNote error={action.error} />
          </div>
        </div>
      </Section>
    </>
  );
}

function AboutSection() {
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);
  return (
    <Section title="About Brim">
      <div className="space-y-2 py-3 text-xs text-muted">
        <p>
          <span className="font-medium text-fg">Brim {version}</span> — a local-first floating utility dock for Windows.
        </p>
        <p>An independent project; not affiliated with any other product.</p>
        <p>To uninstall, use Windows Settings › Apps. The uninstaller asks whether to delete your local data.</p>
      </div>
    </Section>
  );
}
