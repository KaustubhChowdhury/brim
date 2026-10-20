import {
  Bluetooth,
  BluetoothOff,
  Check,
  ChevronDown,
  Contrast,
  Headphones,
  Moon,
  Music,
  Pause,
  Play,
  Settings2,
  SkipBack,
  SkipForward,
  Speaker,
  Volume2,
  Wifi,
  WifiOff,
  type LucideIcon,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useNow } from "../lib/hooks";
import { useMediaState, useVolumeState } from "../lib/media";
import { usePresence } from "../lib/motion";
import { formatDuration, livePosition } from "../lib/format";
import { volumeIcon } from "../dock/ActivityPill";
import { ErrorNote, cx } from "../components/ui";
import type { AppError, AudioOutput, MediaCommand, QuickPage, QuickSetting, QuickState } from "../lib/types";

/** Re-reads Wi-Fi, Bluetooth, outputs etc. this often while the tab is visible. */
const POLL_MS = 4000;

/**
 * The Controls tab: Android-style quick settings tiles, a big volume slider and a
 * now-playing card tinted with the album art.
 */
export function ControlsPanel() {
  const quick = useQuickState();
  const [menuOpen, setMenuOpen] = useState(false);
  const output = quick.state?.outputs.find((o) => o.isDefault) ?? null;

  return (
    <div className="relative flex h-full flex-col gap-2.5 px-4 pb-4 pt-1.5">
      <div className="relative grid shrink-0 grid-cols-6 gap-2.5">
        <WifiTile quick={quick} />
        <BluetoothTile quick={quick} />
        <Tile
          className="col-span-2"
          icon={output && /head(phone|set)/i.test(output.name) ? Headphones : Speaker}
          label="Audio"
          detail={output?.name ?? (quick.state ? "No output device" : "…")}
          on={!!output}
          disabled={!quick.state?.outputs.length}
          onMain={() => setMenuOpen((v) => !v)}
          onMore={() => setMenuOpen((v) => !v)}
          moreLabel="Choose output device"
          menu
          expanded={menuOpen}
        />
        <OutputMenu
          open={menuOpen}
          outputs={quick.state?.outputs ?? []}
          onClose={() => setMenuOpen(false)}
          onPick={(id) => {
            setMenuOpen(false);
            quick.setOutput(id);
          }}
          onSettings={() => quick.open("sound")}
        />
        <NightLightTile quick={quick} />
        <Tile
          className="col-span-3"
          icon={Contrast}
          label="Dark mode"
          detail={quick.state ? (quick.state.darkMode ? "On" : "Off") : "…"}
          on={!!quick.state?.darkMode}
          disabled={!quick.state}
          onMain={() => quick.state && quick.toggle("darkMode", !quick.state.darkMode)}
        />
      </div>

      <VolumeSlider />
      <MediaCard outputName={output?.name ?? null} onOutputClick={() => setMenuOpen(true)} />

      {quick.error && (
        <div className="absolute inset-x-4 bottom-3 z-20">
          <ErrorNote error={quick.error} onDismiss={() => quick.setError(null)} />
        </div>
      )}
    </div>
  );
}

// ---------------- Quick settings state ----------------

type Quick = ReturnType<typeof useQuickState>;

function useQuickState() {
  const [state, setState] = useState<QuickState | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  // Polls skip while a change is in flight so they can't undo an optimistic update.
  const pending = useRef(0);

  const refresh = useCallback(() => {
    if (pending.current) return;
    api.quick
      .state()
      .then((s) => !pending.current && setState(s))
      .catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, POLL_MS);
    return () => window.clearInterval(id);
  }, [refresh]);

  const apply = useCallback(async (optimistic: (s: QuickState) => QuickState, call: () => Promise<QuickState>) => {
    pending.current++;
    setError(null);
    setState((s) => (s ? optimistic(s) : s));
    try {
      const next = await call();
      pending.current--;
      setState(next);
    } catch (e) {
      pending.current--;
      setError(e as AppError);
      api.quick.state().then(setState).catch(() => {});
    }
  }, []);

  const toggle = useCallback(
    (setting: QuickSetting, on: boolean) =>
      apply(
        (s) => {
          switch (setting) {
            case "wifi":
              return s.wifi ? { ...s, wifi: { ...s.wifi, on, connected: on && s.wifi.connected } } : s;
            case "bluetooth":
              return s.bluetooth ? { ...s, bluetooth: { ...s.bluetooth, on } } : s;
            case "nightLight":
              return { ...s, nightLight: on };
            case "darkMode":
              return { ...s, darkMode: on };
          }
        },
        () => api.quick.set(setting, on),
      ),
    [apply],
  );

  const setOutput = useCallback(
    (id: string) =>
      apply(
        (s) => ({ ...s, outputs: s.outputs.map((o) => ({ ...o, isDefault: o.id === id })) }),
        () => api.quick.setOutput(id),
      ),
    [apply],
  );

  const open = useCallback((page: QuickPage) => {
    api.quick.open(page).catch((e) => setError(e as AppError));
  }, []);

  return { state, error, setError, toggle, setOutput, open };
}

// ---------------- Tiles ----------------

function WifiTile({ quick }: { quick: Quick }) {
  const wifi = quick.state?.wifi;
  const detail = !quick.state
    ? "…"
    : !wifi
      ? "Unavailable"
      : !wifi.on
        ? "Off"
        : wifi.connected
          ? (wifi.network ?? "Connected")
          : "Not connected";
  return (
    <Tile
      className="col-span-2"
      icon={wifi?.on ? Wifi : WifiOff}
      label="Wi-Fi"
      detail={detail}
      on={!!wifi?.on}
      disabled={!wifi}
      onMain={() => wifi && quick.toggle("wifi", !wifi.on)}
      onMore={() => quick.open("wifi")}
      moreLabel="Wi-Fi settings"
    />
  );
}

function BluetoothTile({ quick }: { quick: Quick }) {
  const bt = quick.state?.bluetooth;
  return (
    <Tile
      className="col-span-2"
      icon={bt?.on ? Bluetooth : BluetoothOff}
      label="Bluetooth"
      detail={!quick.state ? "…" : !bt ? "Unavailable" : bt.on ? "On" : "Off"}
      on={!!bt?.on}
      disabled={!bt}
      onMain={() => bt && quick.toggle("bluetooth", !bt.on)}
      onMore={() => quick.open("bluetooth")}
      moreLabel="Bluetooth settings"
    />
  );
}

function NightLightTile({ quick }: { quick: Quick }) {
  const s = quick.state;
  const on = !!s?.nightLight;
  return (
    <Tile
      className="col-span-3"
      icon={Moon}
      label="Night light"
      detail={!s ? "…" : s.nightLight == null ? "Open settings" : on ? "On" : "Off"}
      on={on}
      disabled={!s}
      // Until Windows has stored Night light's full state once, only Settings can turn it on.
      onMain={() => s && (s.nightLightSwitchable ? quick.toggle("nightLight", !on) : quick.open("nightLight"))}
      onMore={() => quick.open("nightLight")}
      moreLabel="Night light settings"
    />
  );
}

function Tile({
  icon: Icon,
  label,
  detail,
  on,
  disabled,
  onMain,
  onMore,
  moreLabel,
  menu,
  expanded,
  className,
}: {
  icon: LucideIcon;
  label: string;
  detail: string;
  on: boolean;
  disabled?: boolean;
  onMain: () => void;
  onMore?: () => void;
  moreLabel?: string;
  /** The tile opens a menu instead of switching something on and off. */
  menu?: boolean;
  expanded?: boolean;
  className?: string;
}) {
  return (
    <div data-on={on} className={cx("qs-tile flex h-14 min-w-0 items-center rounded-[22px]", disabled && "opacity-45", className)}>
      <button
        type="button"
        aria-pressed={menu ? undefined : on}
        aria-haspopup={menu ? "menu" : undefined}
        aria-expanded={menu ? expanded : undefined}
        disabled={disabled}
        onClick={onMain}
        className="press-soft press flex h-full min-w-0 flex-1 items-center gap-3 rounded-[22px] pl-4 pr-1 text-left"
      >
        <Icon className="size-[18px] shrink-0" aria-hidden />
        <span className="min-w-0">
          <span className="block truncate text-[13px] font-semibold leading-tight">{label}</span>
          <span className="qs-detail block truncate text-[11px] leading-tight" title={detail}>
            {detail}
          </span>
        </span>
      </button>
      {onMore && (
        <button
          type="button"
          aria-label={moreLabel}
          title={moreLabel}
          aria-haspopup={menu ? "menu" : undefined}
          aria-expanded={menu ? expanded : undefined}
          disabled={disabled}
          onClick={onMore}
          className="qs-more press mr-2 flex size-8 shrink-0 items-center justify-center rounded-full"
        >
          <ChevronDown className={cx("size-4 transition-transform duration-300", expanded && "rotate-180")} aria-hidden />
        </button>
      )}
    </div>
  );
}

function OutputMenu({
  open,
  outputs,
  onClose,
  onPick,
  onSettings,
}: {
  open: boolean;
  outputs: AudioOutput[];
  onClose: () => void;
  onPick: (id: string) => void;
  onSettings: () => void;
}) {
  const { mounted, state } = usePresence(open, 140);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      // Clicks on the Audio tile toggle the menu themselves.
      const target = e.target as HTMLElement;
      if (!ref.current?.contains(target) && !target.closest("[aria-expanded]")) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, onClose]);
  if (!mounted) return null;
  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Output device"
      data-state={state}
      className="pop-menu absolute right-0 top-[60px] z-30 w-[min(320px,100%)] rounded-2xl border border-line bg-[var(--surface-solid)] p-1.5 shadow-2xl"
      style={{ "--origin": "top right" } as React.CSSProperties}
    >
      {outputs.map((o) => (
        <button
          key={o.id}
          type="button"
          role="menuitemradio"
          aria-checked={o.isDefault}
          onClick={() => onPick(o.id)}
          className="press-soft press flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-left text-[13px] hover:bg-hover"
        >
          {/head(phone|set)/i.test(o.name) ? (
            <Headphones className="size-4 shrink-0 text-muted" aria-hidden />
          ) : (
            <Speaker className="size-4 shrink-0 text-muted" aria-hidden />
          )}
          <span className="min-w-0 flex-1 truncate">{o.name}</span>
          {o.isDefault && <Check className="size-4 shrink-0 text-accent" aria-hidden />}
        </button>
      ))}
      <div className="my-1 h-px bg-line" />
      <button
        type="button"
        role="menuitem"
        onClick={onSettings}
        className="press-soft press flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-left text-[13px] text-muted hover:bg-hover"
      >
        <Settings2 className="size-4 shrink-0" aria-hidden />
        Sound settings
      </button>
    </div>
  );
}

// ---------------- Volume ----------------

function VolumeSlider() {
  const v = useVolumeState();
  const [drag, setDrag] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const available = !!v?.available;
  const level = drag ?? v?.level ?? 0;
  const muted = !!v?.muted;
  const Icon = available ? volumeIcon(level, muted) : Volume2;
  // Send at most ~20 updates a second while dragging; the last value always lands.
  const push = (value: number) => {
    pending.current = value;
    if (timer.current !== undefined) return;
    timer.current = window.setTimeout(() => {
      timer.current = undefined;
      if (pending.current != null) api.volume.set(pending.current).catch((e) => setError(e.message));
    }, 50);
  };

  return (
    <div className={cx("flex shrink-0 items-center gap-3", !available && "opacity-45")}>
      <div className="qs-volume group relative h-10 flex-1 overflow-hidden rounded-full has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent">
        <div
          className={cx("qs-volume-fill absolute inset-0 rounded-full", drag == null && "volume-fill", muted && "qs-muted")}
          style={{ "--level": muted ? 0 : level } as React.CSSProperties}
        />
        <input
          type="range"
          aria-label="Volume"
          aria-valuetext={muted ? "Muted" : `${Math.round(level * 100)}%`}
          min={0}
          max={100}
          step={1}
          disabled={!available}
          value={Math.round(level * 100)}
          onChange={(e) => {
            const next = Number(e.target.value) / 100;
            setDrag(next);
            setError(null);
            push(next);
          }}
          onPointerUp={() => setDrag(null)}
          onKeyUp={() => setDrag(null)}
          onBlur={() => setDrag(null)}
          className="absolute inset-0 h-full w-full cursor-default opacity-0"
        />
        <button
          type="button"
          aria-label={muted ? "Unmute" : "Mute"}
          title={muted ? "Unmute" : "Mute"}
          disabled={!available}
          onClick={() => v && api.volume.mute(!v.muted).catch((e) => setError(e.message))}
          className="qs-volume-icon press absolute left-1 top-1 z-10 flex size-8 items-center justify-center rounded-full"
        >
          <Icon key={muted || level <= 0.001 ? "off" : level < 0.5 ? "low" : "high"} className="pop size-[18px]" aria-hidden />
        </button>
      </div>
      <span className="w-10 shrink-0 text-right text-[13px] font-semibold tabular-nums text-muted">
        {!available ? "—" : muted ? "Muted" : `${Math.round(level * 100)}%`}
      </span>
      {error && (
        <span className="sr-only" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}

// ---------------- Now playing ----------------

function MediaCard({ outputName, onOutputClick }: { outputName: string | null; onOutputClick: () => void }) {
  const media = useMediaState();
  const action = useAction();
  const active = media?.active ?? null;
  const isPlaying = active?.status === "playing";
  const now = useNow(500, isPlaying);
  const [scrub, setScrub] = useState<number | null>(null);
  const tint = useArtworkTint(active?.artwork ?? null);

  if (!active) {
    return (
      <div className="qs-media qs-media-empty flex min-h-0 flex-1 flex-col items-center justify-center gap-1 rounded-[24px] text-center">
        <Music className="size-5 opacity-60" aria-hidden />
        <div className="text-[13px] font-semibold">Nothing playing</div>
        <div className="max-w-[340px] text-[11px] opacity-70">
          {media && !media.available
            ? "Windows didn't provide media controls on this PC."
            : "Play something in Spotify, YouTube Music or any app that shows Windows media controls."}
        </div>
      </div>
    );
  }

  const position = livePosition(active.positionMs, active.durationMs, active.positionUpdatedAt, isPlaying, now);
  const send = (cmd: MediaCommand) => action.run(() => api.media.command(cmd));
  const shown = scrub ?? position;
  const fraction = shown != null && active.durationMs ? Math.min(1, Math.max(0, shown / active.durationMs)) : 0;
  const seek = () => {
    if (scrub != null) action.run(() => api.media.seek(scrub)).finally(() => setScrub(null));
  };

  return (
    <div
      className="qs-media relative flex min-h-0 flex-1 flex-col justify-between gap-2 overflow-hidden rounded-[24px] px-4 py-3"
      style={{ "--tint": tint } as React.CSSProperties}
    >
      <button
        type="button"
        onClick={onOutputClick}
        className="press-soft press flex max-w-full items-center gap-1.5 self-start rounded-full text-[11px] font-medium opacity-80 hover:opacity-100"
        title="Choose output device"
      >
        <Volume2 className="size-3.5 shrink-0" aria-hidden />
        <span className="truncate">{outputName ?? active.appName}</span>
      </button>

      <div className="flex min-w-0 items-center gap-3">
        <div className="size-14 shrink-0 overflow-hidden rounded-[14px] bg-black/25 shadow-[0_6px_16px_rgb(0_0_0/0.35)]">
          {active.artwork ? (
            <img key={active.artwork.length + active.title} src={active.artwork} alt="" className="fade-in size-full object-cover" draggable={false} />
          ) : (
            <div className="flex size-full items-center justify-center">
              <Music className="size-6 opacity-60" aria-hidden />
            </div>
          )}
        </div>
        <div key={`${active.sessionId}|${active.title}|${active.artist}`} className="fade-in min-w-0 flex-1">
          <div className="truncate text-[17px] font-bold uppercase leading-tight tracking-wide" title={active.title}>
            {active.title || "Unknown title"}
          </div>
          <div className="truncate text-[12px] opacity-75" title={active.artist}>
            {active.artist || active.appName}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <MediaButton label="Previous track" disabled={!active.canPrevious || action.busy} onClick={() => send("previous")}>
            <SkipBack className="size-4 fill-current" aria-hidden />
          </MediaButton>
          <button
            type="button"
            aria-label={isPlaying ? "Pause" : "Play"}
            title={active.canPlayPause ? (isPlaying ? "Pause" : "Play") : "Not supported by this player"}
            disabled={!active.canPlayPause || action.busy}
            onClick={() => send("playPause")}
            className={cx(
              "qs-play press flex h-11 w-14 items-center justify-center disabled:opacity-40",
              isPlaying ? "rounded-[16px]" : "rounded-[22px]",
            )}
          >
            {isPlaying ? (
              <Pause key="pause" className="pop size-5 fill-current" aria-hidden />
            ) : (
              <Play key="play" className="pop ml-0.5 size-5 fill-current" aria-hidden />
            )}
          </button>
          <MediaButton label="Next track" disabled={!active.canNext || action.busy} onClick={() => send("next")}>
            <SkipForward className="size-4 fill-current" aria-hidden />
          </MediaButton>
        </div>
      </div>

      {position != null && active.durationMs != null ? (
        <div className="flex items-center gap-3 text-[11px] tabular-nums">
          <span className="w-9 opacity-80">{formatDuration(shown ?? 0)}</span>
          <div className="relative h-5 flex-1 has-[:focus-visible]:rounded-full has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-white/70">
            <WaveProgress fraction={fraction} playing={isPlaying && scrub == null} smooth={scrub == null && isPlaying} />
            <input
              type="range"
              aria-label="Playback position"
              aria-valuetext={formatDuration(shown ?? 0)}
              min={0}
              max={active.durationMs}
              step={1000}
              value={shown ?? 0}
              disabled={!active.canSeek}
              onChange={(e) => setScrub(Number(e.target.value))}
              onPointerUp={seek}
              onKeyUp={seek}
              className="absolute inset-0 h-full w-full cursor-default opacity-0 disabled:cursor-not-allowed"
            />
          </div>
          <span className="w-9 text-right opacity-80">{formatDuration(active.durationMs)}</span>
        </div>
      ) : (
        <div className="text-[11px] opacity-70">{active.appName}</div>
      )}

      {action.error && (
        <div className="absolute inset-x-3 bottom-2 z-10 text-fg">
          <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />
        </div>
      )}
    </div>
  );
}

function MediaButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={disabled ? `${label} (not available)` : label}
      disabled={disabled}
      onClick={onClick}
      className="press flex size-9 items-center justify-center rounded-full hover:bg-white/15 disabled:opacity-30"
    >
      {children}
    </button>
  );
}

/**
 * Android-style progress: a gently flowing wave up to the playhead (flattening while paused),
 * then a plain line. Both halves are clipped from full-width layers, so moving the playhead
 * only animates clip-path and transform.
 */
function WaveProgress({ fraction, playing, smooth }: { fraction: number; playing: boolean; smooth: boolean }) {
  const pct = `${fraction * 100}%`;
  return (
    <div className={cx("qs-wave-track pointer-events-none absolute inset-0", smooth && "qs-wave-smooth")} aria-hidden>
      <div className="qs-wave-rest" style={{ clipPath: `inset(0 0 0 ${pct})` }} />
      <div className="qs-wave-played" style={{ clipPath: `inset(-4px calc(100% - ${pct}) -4px 0)` }}>
        <div className={cx("qs-wave", !playing && "qs-wave-paused")} />
        <div className={cx("qs-wave-flat", playing && "opacity-0")} />
      </div>
      <div className="qs-wave-head-track">
        <div className="qs-wave-head" style={{ transform: `translateX(${pct})` }}>
          <span />
        </div>
      </div>
    </div>
  );
}

// ---------------- Album art tint ----------------

const FALLBACK_TINT = "color-mix(in srgb, var(--accent) 45%, #121212)";

/** A deep, saturated colour sampled from the artwork (data URL), like Android's media card. */
function useArtworkTint(src: string | null): string {
  const [tint, setTint] = useState(FALLBACK_TINT);
  useEffect(() => {
    if (!src) {
      setTint(FALLBACK_TINT);
      return;
    }
    let alive = true;
    const img = new Image();
    img.onload = () => {
      if (!alive) return;
      try {
        const size = 24;
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = size;
        const ctx = canvas.getContext("2d", { willReadFrequently: true });
        if (!ctx) return;
        ctx.drawImage(img, 0, 0, size, size);
        const px = ctx.getImageData(0, 0, size, size).data;
        // Weight colourful pixels more so a vivid accent wins over grey backgrounds.
        let r = 0, g = 0, b = 0, w = 0;
        for (let i = 0; i + 2 < px.length; i += 4) {
          const [pr, pg, pb] = [px[i] ?? 0, px[i + 1] ?? 0, px[i + 2] ?? 0];
          const weight = 0.15 + (Math.max(pr, pg, pb) - Math.min(pr, pg, pb)) / 255;
          r += pr * weight;
          g += pg * weight;
          b += pb * weight;
          w += weight;
        }
        setTint(deepen(r / w, g / w, b / w));
      } catch {
        setTint(FALLBACK_TINT);
      }
    };
    img.onerror = () => alive && setTint(FALLBACK_TINT);
    img.src = src;
    return () => {
      alive = false;
    };
  }, [src]);
  return tint;
}

/** Same hue, kept dark enough for white text and saturated enough to feel like the art. */
function deepen(r: number, g: number, b: number): string {
  const [h, s, l] = rgbToHsl(r, g, b);
  return `hsl(${Math.round(h)} ${Math.round(Math.min(70, Math.max(s, 18)))}% ${Math.round(Math.min(26, Math.max(l * 0.55, 14)))}%)`;
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l * 100];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s * 100, l * 100];
}
