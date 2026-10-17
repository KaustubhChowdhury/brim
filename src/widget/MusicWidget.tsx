import { Music, Pause, Play, SkipBack, SkipForward } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { useNow, useSettings } from "../lib/hooks";
import { useMediaState } from "../lib/media";
import { formatDuration, livePosition } from "../lib/format";
import type { MediaCommand, NowPlaying } from "../lib/types";
import { cx } from "../components/ui";
import { fillStyle } from "../lib/motion";

/** Desktop music widget (its own window). Drag anywhere on the card to move it. */
export function MusicWidget() {
  const { settings } = useSettings();
  const media = useMediaState();
  const active = media?.active ?? null;
  const hideIdle = settings.musicWidgetHideIdle;
  const show = !!active || !hideIdle;

  useEffect(() => {
    if (media === null) return; // wait for the first state
    api.widget.setVisible(show).catch(() => {});
  }, [show, media === null]);

  const compact = settings.musicWidgetStyle === "compact";
  return (
    <div className="h-full w-full p-1.5">
      <div
        data-tauri-drag-region
        className={cx("music-widget fade-in relative flex h-full w-full overflow-hidden rounded-[20px]", compact ? "flex-col px-5 pb-4 pt-5" : "flex-col p-5")}
      >
        {active ? (
          compact ? <CompactCard np={active} /> : <TallCard np={active} />
        ) : (
          <div data-tauri-drag-region className="flex flex-1 flex-col items-center justify-center gap-2 text-[13px] text-white/60">
            <Music className="size-7" aria-hidden />
            Nothing playing
          </div>
        )}
      </div>
    </div>
  );
}

function Artwork({ np, className }: { np: NowPlaying; className: string }) {
  return (
    <div className={cx("overflow-hidden rounded-xl bg-white/5 shadow-[0_10px_30px_rgb(0_0_0/0.35)] ring-1 ring-white/10", className)}>
      {np.artwork ? (
        <img key={np.artwork.length + np.title} src={np.artwork} alt="" className="fade-in size-full object-cover" draggable={false} />
      ) : (
        <div className="flex size-full items-center justify-center text-white/40">
          <Music className="size-10" aria-hidden />
        </div>
      )}
    </div>
  );
}

function TallCard({ np }: { np: NowPlaying }) {
  return (
    <>
      <Artwork np={np} className="aspect-square w-full" />
      <TrackText np={np} className="mt-4" big />
      <Progress np={np} className="mt-3" />
      <Controls np={np} className="mt-2" />
    </>
  );
}

function CompactCard({ np }: { np: NowPlaying }) {
  return (
    <>
      <div data-tauri-drag-region className="flex items-center gap-4">
        <Artwork np={np} className="size-[104px] shrink-0" />
        <TrackText np={np} className="min-w-0 flex-1" big />
      </div>
      <Progress np={np} className="mt-auto" />
      <Controls np={np} className="mt-1" />
    </>
  );
}

function TrackText({ np, className, big }: { np: NowPlaying; className?: string; big?: boolean }) {
  const sub = [np.artist, np.album].filter(Boolean).join(" – ") || np.appName;
  return (
    <div key={`${np.title}|${np.artist}`} data-tauri-drag-region className={cx("fade-in min-w-0", className)}>
      <div data-tauri-drag-region className={cx("truncate font-semibold text-white", big ? "text-[19px] leading-tight" : "text-sm")}>
        {np.title || "Unknown title"}
      </div>
      <div data-tauri-drag-region className="mt-0.5 truncate text-[14px] text-white/65">{sub}</div>
    </div>
  );
}

function Progress({ np, className }: { np: NowPlaying; className?: string }) {
  const playing = np.status === "playing";
  const now = useNow(500, playing);
  const position = livePosition(np.positionMs, np.durationMs, np.positionUpdatedAt, playing, now);
  const [scrub, setScrub] = useState<number | null>(null);
  if (position == null || np.durationMs == null) return <div className={cx("h-6", className)} />;
  const shown = scrub ?? position;
  const commit = () => {
    if (scrub != null) api.media.seek(scrub).catch(() => {}).finally(() => setScrub(null));
  };
  return (
    <div className={className}>
      <div className="group relative h-3 rounded-full has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-white/50">
        <div className="absolute inset-x-0 top-1/2 h-[3px] -translate-y-1/2 overflow-hidden rounded-full bg-white/15 transition-[height] duration-200 group-hover:h-[5px]">
          <div
            className={cx("h-full w-full rounded-full bg-white/80", scrub == null && playing && "progress-fill")}
            style={fillStyle(shown / Math.max(1, np.durationMs))}
          />
        </div>
        <input
          type="range"
          aria-label="Playback position"
          aria-valuetext={formatDuration(shown)}
          min={0}
          max={np.durationMs}
          step={1000}
          value={shown}
          disabled={!np.canSeek}
          onChange={(e) => setScrub(Number(e.target.value))}
          onPointerUp={commit}
          onKeyUp={commit}
          className="absolute inset-0 h-full w-full cursor-default opacity-0"
        />
      </div>
      <div className="mt-1 flex justify-between text-[12px] tabular-nums text-white/55">
        <span>{formatDuration(shown)}</span>
        <span>{formatDuration(np.durationMs)}</span>
      </div>
    </div>
  );
}

function Controls({ np, className }: { np: NowPlaying; className?: string }) {
  const playing = np.status === "playing";
  const send = (c: MediaCommand) => api.media.command(c).catch(() => {});
  return (
    <div className={cx("flex items-center justify-center gap-5", className)}>
      <button
        type="button"
        aria-label="Previous track"
        disabled={!np.canPrevious}
        onClick={() => send("previous")}
        className="press flex size-10 items-center justify-center rounded-full text-white hover:bg-white/10 disabled:opacity-30"
      >
        <SkipBack className="size-5 fill-current" aria-hidden />
      </button>
      <button
        type="button"
        aria-label={playing ? "Pause" : "Play"}
        disabled={!np.canPlayPause}
        onClick={() => send("playPause")}
        className="press flex size-14 items-center justify-center rounded-full bg-white/12 text-white ring-1 ring-white/10 hover:bg-white/20 disabled:opacity-30"
      >
        {playing ? (
          <Pause key="pause" className="pop size-6 fill-current" aria-hidden />
        ) : (
          <Play key="play" className="pop ml-0.5 size-6 fill-current" aria-hidden />
        )}
      </button>
      <button
        type="button"
        aria-label="Next track"
        disabled={!np.canNext}
        onClick={() => send("next")}
        className="press flex size-10 items-center justify-center rounded-full text-white hover:bg-white/10 disabled:opacity-30"
      >
        <SkipForward className="size-5 fill-current" aria-hidden />
      </button>
    </div>
  );
}
