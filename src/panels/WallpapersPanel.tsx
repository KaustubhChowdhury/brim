import { Check, FolderOpen, Heart, Image, Pause, Play, Trash2, TriangleAlert, Volume2, VolumeX } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { api, toAppError } from "../lib/ipc";
import { useAction, useLoader, useSettings } from "../lib/hooks";
import type { WallpaperFit } from "../lib/types";
import { Button, EmptyState, cx } from "../components/ui";

const FITS: Array<{ value: WallpaperFit; label: string }> = [
  { value: "fill", label: "Fill" },
  { value: "fit", label: "Fit" },
  { value: "stretch", label: "Stretch" },
  { value: "center", label: "Centre" },
  { value: "tile", label: "Tile" },
  { value: "span", label: "Span" },
];

// Carousel geometry (logical px).
const CENTER_W = 330;
const CENTER_H = 186;
const SIDE_W = 54;
const SIDE_H = 150;
const GAP = 10;
const VISIBLE_SIDES = 3;

type Filter = "recent" | "favourites" | "engine";

/** One card in the carousel, from the local gallery or from Wallpaper Engine. */
interface Card {
  key: string;
  title: string;
  thumb: string | null;
  sharp: string | null;
  missing?: boolean;
  favourite?: boolean;
  galleryId?: number;
  engineId?: string;
}

/** Horizontal x-offset of a card `o` steps away from the selected one. */
function offsetX(o: number): number {
  if (o === 0) return 0;
  const steps = Math.abs(o);
  return Math.sign(o) * (CENTER_W / 2 + GAP + (steps - 1) * (SIDE_W + GAP) + SIDE_W / 2);
}

/** Wallpaper gallery (and Wallpaper Engine, when installed) as a slanted carousel. */
export function WallpapersPanel() {
  const { settings, update } = useSettings();
  const gallery = useLoader(api.wallpapers.list, ["wallpapers:changed"]);
  const engineStatus = useLoader(api.wallpaperEngine.status);
  const [engineList, setEngineList] = useState<Awaited<ReturnType<typeof api.wallpaperEngine.list>> | null>(null);
  const action = useAction();
  const [filter, setFilter] = useState<Filter>("recent");
  const [selected, setSelected] = useState(0);
  const [previews, setPreviews] = useState<Record<string, string | null>>({});
  const [applied, setApplied] = useState<string | null>(null);
  const [engineState, setEngineState] = useState({ playing: true, muted: false });
  const wheelLock = useRef(0);
  const engineInstalled = !!engineStatus.data?.installed;

  useEffect(() => {
    if (filter !== "engine" || engineList) return;
    api.wallpaperEngine.list().then(setEngineList).catch((e) => action.setError(toAppError(e)));
  }, [filter, engineList]);

  const cards: Card[] =
    filter === "engine"
      ? (engineList ?? []).map((w) => ({
          key: `we:${w.id}`,
          title: w.title,
          thumb: previews[`we:${w.id}`] ?? null,
          sharp: previews[`we:${w.id}`] ?? null,
          engineId: w.id,
        }))
      : (gallery.data ?? [])
          .filter((w) => filter === "recent" || w.favourite)
          .slice()
          .sort((a, b) => b.createdAt - a.createdAt)
          .map((w) => ({
            key: `g:${w.id}`,
            title: w.title,
            thumb: w.thumbnail,
            sharp: previews[`g:${w.id}`] ?? null,
            missing: w.missing,
            favourite: w.favourite,
            galleryId: w.id,
          }));
  const index = Math.min(selected, Math.max(0, cards.length - 1));
  const current = cards[index];

  useEffect(() => setSelected(0), [filter]);

  // Load images lazily: Wallpaper Engine previews for the visible cards, and a sharp preview
  // for the large middle gallery card (thumbnails are small).
  const visibleKeys = cards
    .slice(Math.max(0, index - VISIBLE_SIDES), index + VISIBLE_SIDES + 1)
    .map((c) => c.key)
    .join("|");
  useEffect(() => {
    let alive = true;
    for (const c of cards.slice(Math.max(0, index - VISIBLE_SIDES), index + VISIBLE_SIDES + 1)) {
      if (c.key in previews) continue;
      const isCenter = c.key === current?.key;
      if (c.engineId) {
        api.wallpaperEngine
          .preview(c.engineId)
          .then((p) => alive && setPreviews((m) => ({ ...m, [c.key]: p })))
          .catch(() => alive && setPreviews((m) => ({ ...m, [c.key]: null })));
      } else if (isCenter && c.galleryId != null && !c.missing) {
        const id = c.galleryId;
        api.wallpapers
          .preview(id)
          .then((p) => alive && setPreviews((m) => ({ ...m, [c.key]: p })))
          .catch(() => {});
      }
    }
    return () => {
      alive = false;
    };
  }, [visibleKeys, current?.key]);

  const step = (d: number) => setSelected((i) => Math.min(cards.length - 1, Math.max(0, Math.min(i, cards.length - 1) + d)));

  const apply = async (c: Card) => {
    if (c.missing) return;
    try {
      if (c.engineId) await api.wallpaperEngine.apply(c.engineId);
      else if (c.galleryId != null) await api.wallpapers.apply(c.galleryId, settings.wallpaperFit);
      setApplied(c.key);
      window.setTimeout(() => setApplied((a) => (a === c.key ? null : a)), 1400);
    } catch (e) {
      action.setError(toAppError(e));
    }
  };

  const engineControl = (control: "play" | "pause" | "mute" | "unmute") =>
    action.run(() => api.wallpaperEngine.control(control)).then(() =>
      setEngineState((s) => ({
        playing: control === "play" ? true : control === "pause" ? false : s.playing,
        muted: control === "mute" ? true : control === "unmute" ? false : s.muted,
      })),
    );

  const filters: Filter[] = engineInstalled ? ["recent", "favourites", "engine"] : ["recent", "favourites"];

  return (
    <div className="flex h-full flex-col px-4 pb-3 pt-3">
      <div className="flex items-center gap-2">
        {filters.map((f) => (
          <button
            key={f}
            type="button"
            aria-pressed={filter === f}
            onClick={() => setFilter(f)}
            className={cx(
              "press h-7 rounded-full px-3.5 text-[11.5px] font-semibold",
              filter === f ? "bg-accent text-accent-contrast" : "bg-field text-muted hover:text-fg",
            )}
          >
            {f === "recent" ? "Recent" : f === "favourites" ? "Favourites" : "Wallpaper Engine"}
          </button>
        ))}
        <span className="ml-1 text-[11px] text-subtle">
          {cards.length} {filter === "engine" ? (cards.length === 1 ? "wallpaper" : "wallpapers") : cards.length === 1 ? "picture" : "pictures"}
        </span>
        <div className="ml-auto flex items-center gap-1.5">
          {filter === "engine" ? (
            <>
              <button
                type="button"
                aria-label={engineState.playing ? "Pause Wallpaper Engine" : "Play Wallpaper Engine"}
                title={engineState.playing ? "Pause" : "Play"}
                onClick={() => engineControl(engineState.playing ? "pause" : "play")}
                className="press flex size-7 items-center justify-center rounded-full bg-field text-muted hover:text-fg"
              >
                {engineState.playing ? <Pause className="size-3.5" aria-hidden /> : <Play className="size-3.5" aria-hidden />}
              </button>
              <button
                type="button"
                aria-label={engineState.muted ? "Unmute Wallpaper Engine" : "Mute Wallpaper Engine"}
                title={engineState.muted ? "Unmute" : "Mute"}
                onClick={() => engineControl(engineState.muted ? "unmute" : "mute")}
                className="press flex size-7 items-center justify-center rounded-full bg-field text-muted hover:text-fg"
              >
                {engineState.muted ? <VolumeX className="size-3.5" aria-hidden /> : <Volume2 className="size-3.5" aria-hidden />}
              </button>
            </>
          ) : (
            <>
              <select
                aria-label="How the wallpaper fills the screen"
                value={settings.wallpaperFit}
                onChange={(e) => update({ wallpaperFit: e.target.value as WallpaperFit }).catch(() => {})}
                className="h-7 rounded-full bg-field px-2.5 text-[11.5px] text-muted outline-none hover:text-fg"
              >
                {FITS.map((f) => (
                  <option key={f.value} value={f.value}>
                    {f.label}
                  </option>
                ))}
              </select>
              <button
                type="button"
                aria-label="Add pictures"
                title="Add pictures"
                onClick={() => action.run(() => api.wallpapers.import())}
                className="press flex size-7 items-center justify-center rounded-full bg-field text-muted hover:text-fg"
              >
                <FolderOpen className="size-3.5" aria-hidden />
              </button>
            </>
          )}
        </div>
      </div>

      {action.error && <p className="mt-2 text-xs text-danger">{action.error.message}</p>}

      {cards.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <EmptyState
            icon={Image}
            title={filter === "favourites" ? "No favourites yet" : filter === "engine" ? (engineList ? "No Wallpaper Engine wallpapers" : "Loading…") : "No pictures yet"}
          >
            {filter === "recent" && (
              <Button small variant="primary" icon={FolderOpen} onClick={() => action.run(() => api.wallpapers.import())}>
                Add pictures
              </Button>
            )}
          </EmptyState>
        </div>
      ) : (
        <div
          className="carousel relative mt-2 flex-1 outline-none"
          tabIndex={0}
          aria-label="Wallpapers. Use left and right arrows to browse, Enter to set."
          onWheel={(e) => {
            const now = Date.now();
            if (now - wheelLock.current < 140) return;
            const d = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
            if (Math.abs(d) < 2) return;
            wheelLock.current = now;
            step(d > 0 ? 1 : -1);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowRight") step(1);
            else if (e.key === "ArrowLeft") step(-1);
            else if (e.key === "Enter" && current) apply(current);
            else return;
            e.preventDefault();
          }}
        >
          {cards.map((c, i) => {
            const o = i - index;
            if (Math.abs(o) > VISIBLE_SIDES + 1) return null;
            const center = o === 0;
            const hidden = Math.abs(o) > VISIBLE_SIDES;
            const src = center ? (c.sharp ?? c.thumb) : c.thumb;
            const style: CSSProperties = {
              width: center ? CENTER_W : SIDE_W,
              height: center ? CENTER_H : SIDE_H,
              transform: `translate(-50%, -50%) translateX(${offsetX(o)}px) skewX(${center ? 0 : -9}deg)`,
              opacity: hidden ? 0 : 1 - Math.max(0, Math.abs(o) - 1) * 0.18,
              zIndex: 10 - Math.abs(o),
            };
            return (
              <div key={c.key} className="carousel-card group" style={style}>
                <button
                  type="button"
                  tabIndex={-1}
                  aria-label={center ? `Set ${c.title} as wallpaper` : `Show ${c.title}`}
                  onClick={() => (center ? apply(c) : setSelected(i))}
                  className="block size-full"
                >
                  {src ? (
                    <img src={src} alt="" draggable={false} className={cx("size-full object-cover", c.missing && "opacity-30 grayscale")} />
                  ) : (
                    <span className="flex size-full items-center justify-center text-subtle">
                      <Image className="size-5" aria-hidden />
                    </span>
                  )}
                </button>
                {center && (
                  <>
                    {c.missing && (
                      <span className="absolute inset-0 flex items-center justify-center gap-1.5 text-xs text-warn">
                        <TriangleAlert className="size-4" aria-hidden /> File missing
                      </span>
                    )}
                    <div className="carousel-actions absolute inset-x-0 bottom-0 flex items-center gap-1.5 px-3 pb-2.5 pt-8">
                      <span className="min-w-0 flex-1 truncate text-[12px] font-medium text-white">{c.title}</span>
                      {c.galleryId != null && (
                        <>
                          <button
                            type="button"
                            aria-label={c.favourite ? "Remove from favourites" : "Add to favourites"}
                            onClick={() => action.run(() => api.wallpapers.favourite(c.galleryId!, !c.favourite))}
                            className="press flex size-7 items-center justify-center rounded-full bg-black/35 text-white hover:bg-black/55"
                          >
                            <Heart className={cx("size-3.5", c.favourite && "fill-current")} aria-hidden />
                          </button>
                          <button
                            type="button"
                            aria-label="Remove from gallery"
                            onClick={() => action.run(() => api.wallpapers.remove(c.galleryId!))}
                            className="press flex size-7 items-center justify-center rounded-full bg-black/35 text-white hover:bg-black/55"
                          >
                            <Trash2 className="size-3.5" aria-hidden />
                          </button>
                        </>
                      )}
                    </div>
                    {applied === c.key && (
                      <span className="pop absolute right-2.5 top-2.5 flex items-center gap-1 rounded-full bg-black/55 px-2.5 py-1 text-[11px] font-medium text-white">
                        <Check className="size-3" aria-hidden /> Set
                      </span>
                    )}
                  </>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
