import { Music, Pause, Play, SkipForward, Volume1, Volume2, VolumeX } from "lucide-react";
import { useRef } from "react";
import { api } from "../lib/ipc";
import { useMediaState, useVolumeState } from "../lib/media";
import type { ActivityKind, IdleForm } from "../lib/types";
import { cx } from "../components/ui";
import { fillStyle } from "../lib/motion";

/** Where the pill grows from / shrinks back into. */
export type PillAnchor = "island" | IdleForm;

/**
 * A live activity, like the Dynamic Island: the island grows from its centre into a wider pill
 * showing what just happened (a song started, the volume changed), then folds back.
 */
export function ActivityPill({
  kind,
  from,
  endingTo,
  onOpenMusic,
}: {
  kind: ActivityKind;
  from: PillAnchor;
  endingTo: PillAnchor | null;
  onOpenMusic: () => void;
}) {
  return (
    <div
      className={cx(
        "activity-pill surface flex h-full w-full items-center overflow-hidden",
        endingTo ? `activity-shrink-${endingTo}` : `activity-grow-${from}`,
      )}
    >
      <div className="activity-content flex h-full w-full items-center">
        {kind === "nowPlaying" ? <NowPlayingContent onOpen={onOpenMusic} /> : <VolumeContent />}
      </div>
    </div>
  );
}

function NowPlayingContent({ onOpen }: { onOpen: () => void }) {
  const media = useMediaState();
  const a = media?.active;
  const playing = a?.status === "playing";
  return (
    <div className="flex h-full w-full items-center gap-3 px-2.5">
      <button
        type="button"
        onClick={onOpen}
        aria-label="Open music"
        className="press press-soft flex min-w-0 flex-1 items-center gap-3 text-left"
      >
        <div className="size-11 shrink-0 overflow-hidden rounded-xl bg-field shadow-[0_4px_14px_rgb(0_0_0/0.3)]">
          {a?.artwork ? (
            <img key={a.title} src={a.artwork} alt="" className="fade-in size-full object-cover" draggable={false} />
          ) : (
            <div className="flex size-full items-center justify-center text-subtle">
              <Music className="size-5" aria-hidden />
            </div>
          )}
        </div>
        <div key={`${a?.title}|${a?.artist}`} className="fade-in min-w-0 flex-1">
          <div className="truncate text-[13px] font-semibold leading-tight">{a?.title || "Now playing"}</div>
          <div className="truncate text-[11.5px] text-muted">{a ? a.artist || a.appName : ""}</div>
        </div>
      </button>
      {playing && (
        <span className="flex h-4 shrink-0 items-end gap-[2px]" aria-hidden>
          {[0, -300, -600, -150].map((d) => (
            <span key={d} className="eq-bar h-4 w-[3px] rounded-full bg-accent" style={{ animationDelay: `${d}ms` }} />
          ))}
        </span>
      )}
      <div className="flex shrink-0 items-center">
        <button
          type="button"
          aria-label={playing ? "Pause" : "Play"}
          disabled={!a?.canPlayPause}
          onClick={() => api.media.command("playPause").catch(() => {})}
          className="press flex size-9 items-center justify-center rounded-full hover:bg-hover disabled:opacity-40"
        >
          {playing ? <Pause key="p" className="pop size-[18px]" aria-hidden /> : <Play key="l" className="pop ml-0.5 size-[18px]" aria-hidden />}
        </button>
        <button
          type="button"
          aria-label="Next track"
          disabled={!a?.canNext}
          onClick={() => api.media.command("next").catch(() => {})}
          className="press flex size-9 items-center justify-center rounded-full hover:bg-hover disabled:opacity-40"
        >
          <SkipForward className="size-[18px]" aria-hidden />
        </button>
      </div>
    </div>
  );
}

export function volumeIcon(level: number, muted: boolean) {
  if (muted || level <= 0.001) return VolumeX;
  return level < 0.5 ? Volume1 : Volume2;
}

function VolumeContent() {
  const v = useVolumeState();
  const level = v?.level ?? 0;
  const muted = v?.muted ?? false;
  const Icon = volumeIcon(level, muted);
  const wheelLock = useRef(0);
  return (
    <div
      className="flex h-full w-full items-center gap-3 px-3"
      onWheel={(e) => {
        const now = Date.now();
        if (now - wheelLock.current < 40 || !v?.available) return;
        wheelLock.current = now;
        api.volume.set(level + (e.deltaY < 0 ? 0.04 : -0.04)).catch(() => {});
      }}
    >
      <button
        type="button"
        aria-label={muted ? "Unmute" : "Mute"}
        onClick={() => api.volume.mute(!muted).catch(() => {})}
        className="press flex size-9 shrink-0 items-center justify-center rounded-full hover:bg-hover"
      >
        <Icon key={muted || level <= 0.001 ? "off" : level < 0.5 ? "low" : "high"} className="pop size-[18px]" aria-hidden />
      </button>
      <div className="relative h-1.5 flex-1 overflow-hidden rounded-full bg-line" role="meter" aria-label="Volume" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(level * 100)}>
        <div className={cx("volume-fill h-full w-full rounded-full", muted ? "bg-subtle" : "bg-accent")} style={fillStyle(level)} />
      </div>
      <span className="w-8 shrink-0 text-right text-xs font-medium tabular-nums">{muted ? "Off" : Math.round(level * 100)}</span>
    </div>
  );
}
