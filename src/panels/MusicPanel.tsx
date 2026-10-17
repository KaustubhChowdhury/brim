import { Music, Pause, Play, SkipBack, SkipForward, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useNow } from "../lib/hooks";
import { useMediaState, useVolumeState } from "../lib/media";
import { volumeIcon } from "../dock/ActivityPill";
import { fillStyle } from "../lib/motion";
import { formatDuration, livePosition } from "../lib/format";
import { EmptyState, ErrorNote, Spinner, cx } from "../components/ui";
import type { MediaCommand } from "../lib/types";

const STATUS_LABEL: Record<string, string> = {
  playing: "Playing",
  paused: "Paused",
  stopped: "Stopped",
  changing: "Loading…",
  opened: "Ready",
  closed: "Player closed",
};

export function MusicPanel() {
  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1">
        <NowPlaying />
      </div>
      <VolumeRow />
    </div>
  );
}

function NowPlaying() {
  const media = useMediaState();
  const action = useAction();
  const active = media?.active ?? null;
  const isPlaying = active?.status === "playing";
  const now = useNow(500, isPlaying);
  const [scrub, setScrub] = useState<number | null>(null);

  if (!media) return <Spinner label="Loading media" />;
  if (!media.available) {
    return (
      <EmptyState icon={TriangleAlert} title="Media controls are unavailable">
        Windows did not provide media session information on this PC.
      </EmptyState>
    );
  }
  if (!active) {
    return (
      <EmptyState icon={Music} title="No supported media is currently available.">
        Start playing in Spotify, a browser or another app that shows Windows media controls.
      </EmptyState>
    );
  }

  const position = livePosition(active.positionMs, active.durationMs, active.positionUpdatedAt, isPlaying, now);
  const send = (cmd: MediaCommand) => action.run(() => api.media.command(cmd));
  const closed = active.status === "closed" || active.status === "stopped";

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <div className="flex items-center gap-4">
        <div
          className={cx(
            "size-28 shrink-0 overflow-hidden rounded-2xl bg-field shadow-[0_8px_24px_rgb(0_0_0/0.35)] transition-transform duration-500 ease-[var(--ease-out)]",
            isPlaying ? "scale-100" : "scale-[0.94]",
          )}
        >
          {active.artwork ? (
            <img key={active.artwork.length + active.title} src={active.artwork} alt="" className="fade-in size-full object-cover" draggable={false} />
          ) : (
            <div className="flex size-full items-center justify-center text-subtle">
              <Music className="size-10" aria-hidden />
            </div>
          )}
        </div>
        <div key={`${active.sessionId}|${active.title}|${active.artist}`} className="fade-in min-w-0 flex-1">
          <div className="mb-1 inline-flex items-center gap-1.5 rounded-full bg-field px-2 py-0.5 text-[11px] text-muted">
            <span className={cx("size-1.5 rounded-full", isPlaying ? "bg-ok" : "bg-subtle")} aria-hidden />
            {active.appName} · {STATUS_LABEL[active.status] ?? active.status}
          </div>
          <div className="line-clamp-2 text-[15px] font-semibold leading-snug" title={active.title}>
            {active.title || "Unknown title"}
          </div>
          <div className="truncate text-[13px] text-muted" title={active.artist}>
            {active.artist || (active.title ? "Unknown artist" : "Metadata unavailable")}
          </div>
          {active.album && <div className="truncate text-xs text-subtle">{active.album}</div>}
        </div>
      </div>

      {position != null && active.durationMs != null && (
        <div>
          <div className="group relative h-4 rounded-full has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent">
            <div className="absolute inset-x-0 top-1/2 h-1 -translate-y-1/2 overflow-hidden rounded-full bg-line transition-[height] duration-200 group-hover:h-1.5">
              <div
                className={cx("h-full w-full rounded-full bg-fg/80", scrub == null && isPlaying && "progress-fill")}
                style={fillStyle((scrub ?? position) / Math.max(1, active.durationMs))}
              />
            </div>
            <input
              type="range"
              aria-label="Playback position"
              aria-valuetext={formatDuration(scrub ?? position)}
              min={0}
              max={active.durationMs}
              step={1000}
              value={scrub ?? position}
              disabled={!active.canSeek}
              onChange={(e) => setScrub(Number(e.target.value))}
              onPointerUp={() => {
                if (scrub != null) action.run(() => api.media.seek(scrub)).finally(() => setScrub(null));
              }}
              onKeyUp={() => {
                if (scrub != null) action.run(() => api.media.seek(scrub)).finally(() => setScrub(null));
              }}
              className="absolute inset-0 h-full w-full cursor-default opacity-0 disabled:cursor-not-allowed"
            />
          </div>
          <div className="flex justify-between text-[11px] tabular-nums text-subtle">
            <span>{formatDuration(scrub ?? position)}</span>
            <span>{formatDuration(active.durationMs)}</span>
          </div>
        </div>
      )}

      <div className="flex items-center justify-center gap-3">
        <ControlButton label="Previous track" disabled={!active.canPrevious || action.busy} onClick={() => send("previous")}>
          <SkipBack className="size-5" aria-hidden />
        </ControlButton>
        <button
          type="button"
          aria-label={isPlaying ? "Pause" : "Play"}
          title={active.canPlayPause ? (isPlaying ? "Pause" : "Play") : "Not supported by this player"}
          disabled={!active.canPlayPause || action.busy}
          onClick={() => send("playPause")}
          className="press flex size-14 items-center justify-center rounded-full bg-accent text-accent-contrast shadow-[0_6px_18px_color-mix(in_srgb,var(--accent)_40%,transparent)] hover:brightness-110 disabled:opacity-40"
        >
          {isPlaying ? (
            <Pause key="pause" className="pop size-6" aria-hidden />
          ) : (
            <Play key="play" className="pop ml-0.5 size-6" aria-hidden />
          )}
        </button>
        <ControlButton label="Next track" disabled={!active.canNext || action.busy} onClick={() => send("next")}>
          <SkipForward className="size-5" aria-hidden />
        </ControlButton>
      </div>

      {closed && <p className="text-center text-xs text-muted">The player closed or stopped sharing its session.</p>}
      <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />

      {media.sessions.length > 1 && (
        <label className="mt-auto flex items-center gap-2 text-xs text-muted">
          Playing from
          <select
            className="h-7 flex-1 rounded-md border border-line bg-field px-2 text-xs"
            value={media.manualSelection ? active.sessionId : ""}
            onChange={(e) => action.run(() => api.media.select(e.target.value || null))}
          >
            <option value="">Automatic ({active.appName})</option>
            {media.sessions.map((s) => (
              <option key={s.id} value={s.id}>
                {s.appName} — {STATUS_LABEL[s.status] ?? s.status}
              </option>
            ))}
          </select>
        </label>
      )}
    </div>
  );
}

function ControlButton({
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
      className="press flex size-11 items-center justify-center rounded-full text-fg hover:bg-hover disabled:opacity-30"
    >
      {children}
    </button>
  );
}

/** System output volume: the dock doubles as the sound control. */
function VolumeRow() {
  const v = useVolumeState();
  const [drag, setDrag] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  if (!v) return null;
  if (!v.available) {
    return <div className="border-t border-line px-4 py-3 text-xs text-muted">No audio output device is available.</div>;
  }
  const level = drag ?? v.level;
  const Icon = volumeIcon(level, v.muted);
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
    <div className="flex items-center gap-2.5 border-t border-line px-3 py-2.5">
      <button
        type="button"
        aria-label={v.muted ? "Unmute" : "Mute"}
        title={v.muted ? "Unmute" : "Mute"}
        onClick={() => api.volume.mute(!v.muted).catch((e) => setError(e.message))}
        className="press flex size-8 shrink-0 items-center justify-center rounded-full text-fg hover:bg-hover"
      >
        <Icon key={v.muted || level <= 0.001 ? "off" : level < 0.5 ? "low" : "high"} className="pop size-[18px]" aria-hidden />
      </button>
      <div className="group relative h-5 flex-1 rounded-full has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-accent">
        <div className="absolute inset-x-0 top-1/2 h-1.5 -translate-y-1/2 overflow-hidden rounded-full bg-line transition-[height] duration-200 group-hover:h-2">
          <div
            className={cx("h-full w-full rounded-full", drag == null && "volume-fill", v.muted ? "bg-subtle" : "bg-accent")}
            style={fillStyle(level)}
          />
        </div>
        <input
          type="range"
          aria-label="Volume"
          aria-valuetext={v.muted ? "Muted" : `${Math.round(level * 100)}%`}
          min={0}
          max={100}
          step={1}
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
      </div>
      <span className="w-9 shrink-0 text-right text-xs tabular-nums text-muted">{v.muted ? "Muted" : `${Math.round(level * 100)}%`}</span>
      {error && <span className="sr-only" role="alert">{error}</span>}
    </div>
  );
}
