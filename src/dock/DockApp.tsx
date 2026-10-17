import { getCurrentWindow } from "@tauri-apps/api/window";
import { ChevronDown, ChevronUp, GripVertical, LayoutGrid, Settings as SettingsIcon } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../lib/ipc";
import { useLoader, useSettings, useTauriEvent } from "../lib/hooks";
import { useMediaState } from "../lib/media";
import { PanelSwap, closeDelayMs, usePresence } from "../lib/motion";
import type { AppError, Direction, DockState, IdleForm, SlotView, Utility } from "../lib/types";
import { UTILITIES, WIDE_TABS } from "../lib/types";
import { UTILITY_ICON, UTILITY_LABEL } from "../components/icons";
import { ErrorBoundary, ErrorNote, IconButton, cx } from "../components/ui";
import { DockContext, useDock, type DockControls } from "./context";
import { SlotButton } from "./SlotButton";
import { ActivityPill, type PillAnchor } from "./ActivityPill";
import { MusicPanel } from "../panels/MusicPanel";
import { ClipboardPanel } from "../panels/ClipboardPanel";
import { AppsPanel } from "../panels/AppsPanel";
import { TasksPanel } from "../panels/TasksPanel";
import { CalendarPanel } from "../panels/CalendarPanel";
import { SnippetsPanel } from "../panels/SnippetsPanel";
import { WallpapersPanel } from "../panels/WallpapersPanel";
import { TodayPanel } from "../panels/TodayPanel";
import { StatsPanel } from "../panels/StatsPanel";
import { PowerPanel } from "../panels/PowerPanel";
import { SearchPanel } from "../panels/SearchPanel";
import { WideTabs } from "./WideTabs";

// Must match COMPACT_W / COMPACT_H in src-tauri/src/dock.rs.
const PILL_W = 196;
const PILL_H = 52;
const DRAG_THRESHOLD = 4;
/** Total length of the startup intro (bar + staggered icon drops); see .island-intro in styles.css. */
const INTRO_MS = 1700;
/** The idle line's size and inset from the island's top edge (matches .idle-line in styles.css). */
const LINE_W = 100;
const LINE_H = 4;
const LINE_TOP = 3;

type Phase = "compact" | "opening" | "open" | "closing";

const PANELS: Record<Utility, () => ReactNode> = {
  music: () => <MusicPanel />,
  clipboard: () => <ClipboardPanel />,
  apps: () => <AppsPanel />,
  tasks: () => <TasksPanel />,
  calendar: () => <CalendarPanel />,
  snippets: () => <SnippetsPanel />,
  wallpapers: () => <WallpapersPanel />,
  controls: () => <MusicPanel />,
  today: () => <TodayPanel />,
  stats: () => <StatsPanel />,
  power: () => <PowerPanel />,
  search: () => <SearchPanel />,
};

/** In the wide layout, slots for utilities that live inside a tab open that tab. */
function wideTab(u: Utility): Utility {
  if (u === "music") return "controls";
  if (u === "tasks" || u === "calendar") return "today";
  return u;
}

export function DockApp() {
  const { settings } = useSettings();
  const [dock, setDock] = useState<DockState | null>(null);
  const [direction, setDirection] = useState<Direction>("down");
  const [phase, setPhase] = useState<Phase>("compact");
  const [error, setError] = useState<AppError | null>(null);
  const phaseRef = useRef<Phase>("compact");
  phaseRef.current = phase;
  // Collapsing modes: whether the island just grew out of its idle form / is shrinking into it.
  const wasIdle = useRef(false);
  const idleFormRef = useRef<IdleForm>("vanish");
  const [grewFromIdle, setGrewFromIdle] = useState(false);
  const [openedFromIdle, setOpenedFromIdle] = useState(false);
  const [shrinking, setShrinking] = useState(false);
  // Live activity: what it grew out of, and what it is folding back into.
  const [activityFrom, setActivityFrom] = useState<PillAnchor>("island");
  const [activityEnding, setActivityEnding] = useState<PillAnchor | null>(null);
  const hadActivity = useRef(false);

  const slots = useLoader(api.slots.list, ["slots:changed", "apps:changed"]);
  const apps = useLoader(api.apps.list, ["apps:changed"]);
  const media = useMediaState();
  const playing = media?.active?.status === "playing";

  const applyState = useCallback((s: DockState) => {
    setDock(s);
    setDirection(s.direction);
    setShrinking(false);
    if (s.activity && !hadActivity.current) setActivityFrom(wasIdle.current ? idleFormRef.current : "island");
    if (!s.activity) setActivityEnding(null);
    // Returning from an activity lands exactly on the island/idle shape: no extra animation.
    if (!s.expanded && !s.idle && !s.activity) setGrewFromIdle(hadActivity.current ? false : wasIdle.current);
    hadActivity.current = !!s.activity;
    if (s.expanded && phaseRef.current === "compact") setOpenedFromIdle(wasIdle.current);
    wasIdle.current = !s.expanded && s.idle;
    if (s.expanded) {
      if (phaseRef.current === "compact" || phaseRef.current === "closing") {
        setPhase("opening");
        // Two frames: paint the clipped (island-shaped) panel first, then animate it open.
        requestAnimationFrame(() => requestAnimationFrame(() => setPhase("open")));
      }
    } else {
      setPhase("compact");
    }
  }, []);

  // Startup intro (once per app start): point → line → bar, icons drop in, then the dock
  // collapses again in the vanish/line modes.
  const [intro, setIntro] = useState(false);
  const introStarted = useRef(false);

  useEffect(() => {
    api.dock
      .state()
      .then((s) => {
        applyState(s);
        if (introStarted.current) return;
        introStarted.current = true;
        if (settings.animationLevel === "none" || settings.compactVisibility === "hidden" || s.expanded) return;
        setIntro(true);
        if (s.idle) api.dock.intro().catch(() => {});
        window.setTimeout(() => setIntro(false), INTRO_MS);
      })
      .catch(() => {});
    api.dock.ready().catch(() => {});
    // Runs once on mount; settings are read as they were at startup.
  }, [applyState]);

  useTauriEvent<DockState>("dock:state", applyState);
  useTauriEvent<Direction>("dock:layout", setDirection);

  const collapse = useCallback(
    (restoreFocus: boolean) => {
      const p = phaseRef.current;
      if (p !== "open" && p !== "opening") return;
      setPhase("closing");
      // Resize the window only once the panel has shrunk back into the island's shape.
      window.setTimeout(() => {
        api.dock.collapse(restoreFocus).catch(() => setPhase("compact"));
      }, closeDelayMs(settings.animationLevel));
    },
    [settings.animationLevel],
  );

  useTauriEvent<{ restoreFocus: boolean }>("dock:collapse-request", (p) => collapse(p.restoreFocus));

  // An activity's time is up: fold it back into the island or the idle form, then resize.
  useTauriEvent<{ toIdle: boolean }>("dock:activity-end-request", ({ toIdle }) => {
    setActivityEnding(toIdle ? idleFormRef.current : "island");
    window.setTimeout(() => {
      api.dock.activityDone(toIdle).catch(() => setActivityEnding(null));
    }, closeDelayMs(settings.animationLevel));
  });

  // The cursor left the island in a collapsing mode: shrink it into its idle form, then resize.
  useTauriEvent("dock:unpeek-request", () => {
    if (phaseRef.current !== "compact") return;
    setShrinking(true);
    window.setTimeout(() => {
      api.dock.peek(false).catch(() => setShrinking(false));
    }, closeDelayMs(settings.animationLevel));
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented && phaseRef.current !== "compact") {
        e.preventDefault();
        collapse(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [collapse]);

  const utility = dock?.utility ?? settings.lastUtility;

  const wide = settings.panelStyle === "wide";
  const expandTo = useCallback(
    (requested: Utility) => {
      const u = wide ? wideTab(requested) : requested;
      setDock((d) => (d ? { ...d, utility: u } : d));
      api.dock.expand(u).catch((e) => setError(e));
    },
    [wide],
  );

  const activateSlot = useCallback(
    (slot: SlotView) => {
      setError(null);
      if (slot.status !== "ok") {
        api.dock.openSettings("slots").catch((e) => setError(e));
        return;
      }
      if (slot.kind === "app" && slot.target) {
        api.apps
          .launch(slot.target)
          .then(() => {
            if (phaseRef.current !== "compact" && settings.collapseAfterLaunch) collapse(false);
          })
          .catch((e) => {
            setError(e);
            if (phaseRef.current === "compact") api.dock.expand().catch(() => {});
          });
        return;
      }
      expandTo(slot.kind as Utility);
    },
    [collapse, expandTo, settings.collapseAfterLaunch],
  );

  const controls: DockControls = useMemo(
    () => ({
      utility,
      setUtility: expandTo,
      collapse,
      openSettings: (section?: string) => {
        api.dock.openSettings(section).catch((e) => setError(e));
      },
    }),
    [utility, expandTo, collapse],
  );

  const slotList = slots.data ?? [];
  const appById = useMemo(() => new Map((apps.data ?? []).map((a) => [a.id, a])), [apps.data]);
  const activeSlot = slotList.findIndex((s) => s.kind === utility);
  const idleForm: IdleForm = settings.compactVisibility === "line" ? "line" : "vanish";
  idleFormRef.current = idleForm;
  const renderSlots = (expanded: boolean) =>
    slotList.map((slot) => (
      <SlotButton
        key={slot.slotNumber}
        slot={slot}
        app={slot.target ? appById.get(slot.target) : undefined}
        active={expanded && phase === "open" && slot.slotNumber === activeSlot}
        playing={playing && slot.kind === "music"}
        onActivate={activateSlot}
      />
    ));

  return (
    <DockContext.Provider value={controls}>
      <div className="dock-stage">
        {phase === "compact" && dock?.activity ? (
          <ActivityPill
            kind={dock.activity}
            from={activityFrom}
            endingTo={activityEnding}
            onOpenMusic={() => expandTo("music")}
          />
        ) : phase === "compact" && dock?.idle ? (
          <IdleMark
            form={idleForm}
            onHover={() => api.dock.peek(true).catch(() => {})}
            onClick={() => expandTo(utility)}
          />
        ) : phase === "compact" ? (
          <CompactIsland
            idleForm={idleForm}
            intro={intro}
            grow={grewFromIdle && !intro}
            shrinking={shrinking}
            direction={direction}
            fade={settings.compactVisibility === "fade"}
            onWheelExpand={settings.expandOnScroll ? () => expandTo(utility) : undefined}
            onBackgroundClick={() => expandTo(utility)}
          >
            {renderSlots(false)}
          </CompactIsland>
        ) : (
          <ExpandedPanel
            phase={phase}
            dock={dock}
            fromIdle={openedFromIdle ? idleForm : null}
            scale={settings.dockScale}
            slots={renderSlots(true)}
            activeSlot={activeSlot}
            error={error}
            onDismissError={() => setError(null)}
          >
            <PanelSwap id={utility} order={(wide ? WIDE_TABS : UTILITIES).indexOf(utility)}>
              <ErrorBoundary key={utility} name={UTILITY_LABEL[utility]}>
                {PANELS[utility]()}
              </ErrorBoundary>
            </PanelSwap>
          </ExpandedPanel>
        )}
      </div>
    </DockContext.Provider>
  );
}

// The island's entrance plays once per app start, not after every collapse.
let islandHasAppeared = false;

/**
 * The idle dock. "vanish" paints nothing (an invisible hover strip where the island was);
 * "line" is a thin line on the island's top edge. Hovering either grows the island back.
 */
function IdleMark({ form, onHover, onClick }: { form: IdleForm; onHover: () => void; onClick: () => void }) {
  useEffect(() => {
    islandHasAppeared = true;
  }, []);
  return (
    <div
      role="button"
      tabIndex={-1}
      aria-label="Show Brim"
      className="relative h-full w-full"
      onPointerEnter={onHover}
      onWheel={onHover}
      onClick={onClick}
    >
      {form === "line" && <span className="idle-line" aria-hidden />}
    </div>
  );
}

function CompactIsland({
  intro,
  idleForm,
  grow,
  shrinking,
  children,
  direction,
  fade,
  onWheelExpand,
  onBackgroundClick,
}: {
  intro: boolean;
  idleForm: IdleForm;
  grow: boolean;
  shrinking: boolean;
  children: ReactNode;
  direction: Direction;
  fade: boolean;
  onWheelExpand?: () => void;
  onBackgroundClick: () => void;
}) {
  const drag = useRef<{ x: number; y: number; dragging: boolean } | null>(null);
  const suppressClick = useRef(false);
  const wheelLock = useRef(0);
  const [idle, setIdle] = useState(false);
  const idleTimer = useRef<number | undefined>(undefined);
  const [entrance] = useState(() => !islandHasAppeared);
  useEffect(() => {
    islandHasAppeared = true;
  }, []);

  useEffect(() => {
    if (!fade) {
      setIdle(false);
      return;
    }
    idleTimer.current = window.setTimeout(() => setIdle(true), 2500);
    return () => window.clearTimeout(idleTimer.current);
  }, [fade]);

  return (
    <div
      role="toolbar"
      aria-label="Brim"
      data-direction={direction}
      className={cx(
        "surface mx-auto flex h-[52px] w-[196px] items-center justify-between rounded-[min(var(--radius),26px)] px-1.5 transition-opacity duration-700 ease-[var(--ease-out)]",
        intro ? "island-intro" : entrance && !grow && "island-in",
        grow && !shrinking && `island-grow-${idleForm}`,
        shrinking && `island-shrink-${idleForm}`,
        idle ? "opacity-35" : "opacity-100",
      )}
      onPointerEnter={() => {
        window.clearTimeout(idleTimer.current);
        setIdle(false);
      }}
      onPointerLeave={() => {
        if (fade) idleTimer.current = window.setTimeout(() => setIdle(true), 2500);
      }}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        drag.current = { x: e.screenX, y: e.screenY, dragging: false };
        suppressClick.current = false;
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || d.dragging || !(e.buttons & 1)) return;
        if (Math.hypot(e.screenX - d.x, e.screenY - d.y) > DRAG_THRESHOLD) {
          // Past the threshold this is a move, never a click (FR-DCK-006).
          d.dragging = true;
          suppressClick.current = true;
          getCurrentWindow()
            .startDragging()
            .catch(() => {})
            .finally(() => {
              drag.current = null;
            });
        }
      }}
      onPointerUp={() => {
        drag.current = null;
      }}
      onClickCapture={(e) => {
        if (suppressClick.current) {
          e.preventDefault();
          e.stopPropagation();
          suppressClick.current = false;
        }
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onBackgroundClick();
      }}
      onWheel={(e) => {
        if (!onWheelExpand || Math.abs(e.deltaY) < 1) return;
        const now = Date.now();
        if (now - wheelLock.current < 600) return;
        wheelLock.current = now;
        onWheelExpand();
      }}
    >
      {children}
    </div>
  );
}

function ExpandedPanel({
  phase,
  dock,
  fromIdle,
  scale,
  slots,
  activeSlot,
  children,
  error,
  onDismissError,
}: {
  phase: Phase;
  dock: DockState | null;
  fromIdle: IdleForm | null;
  scale: number;
  slots: ReactNode;
  activeSlot: number;
  children: ReactNode;
  error: AppError | null;
  onDismissError: () => void;
}) {
  const { utility, setUtility, collapse, openSettings } = useDock();
  const { settings } = useSettings();
  const wide = settings.panelStyle === "wide";
  const [moreOpen, setMoreOpen] = useState(false);
  const menu = usePresence(moreOpen, 140);
  const stageW = window.innerWidth / scale;
  const stageH = window.innerHeight / scale;
  const offX = (dock?.pillOffsetX ?? 0) / scale;
  const offY = (dock?.pillOffsetY ?? 0) / scale;
  const up = dock?.direction === "up";
  const r = "min(var(--radius), 26px)";
  const islandShaped = phase === "opening" || phase === "closing";
  // Opening straight from the idle form (e.g. via the shortcut) grows from that shape.
  const pillCx = offX + PILL_W / 2;
  const clip = phase === "opening" && fromIdle === "vanish"
    ? `inset(${offY + PILL_H / 2}px ${stageW - pillCx}px ${stageH - offY - PILL_H / 2}px ${pillCx}px round 0px)`
    : phase === "opening" && fromIdle === "line"
    ? `inset(${offY + LINE_TOP}px ${stageW - pillCx - LINE_W / 2}px ${stageH - offY - LINE_TOP - LINE_H}px ${pillCx - LINE_W / 2}px round ${LINE_H / 2}px)`
    : islandShaped
    ? `inset(${offY}px ${Math.max(0, stageW - offX - PILL_W)}px ${Math.max(0, stageH - offY - PILL_H)}px ${offX}px round ${r})`
    : `inset(0px 0px 0px 0px round var(--radius))`;
  const rightSpace = stageW - offX - PILL_W;
  const controlsOnRight = rightSpace >= 92 || offX < 92;

  useEffect(() => setMoreOpen(false), [utility]);

  const controlsGroup = (
    <div className="flex items-center gap-1">
      <IconButton
        size="sm"
        label="All utilities"
        icon={LayoutGrid}
        active={moreOpen}
        aria-expanded={moreOpen}
        onClick={() => setMoreOpen((v) => !v)}
      />
      <IconButton size="sm" label="Settings" icon={SettingsIcon} onClick={() => openSettings()} />
      <IconButton size="sm" label="Collapse dock" icon={up ? ChevronDown : ChevronUp} onClick={() => collapse(true)} />
    </div>
  );
  const grip = (
    <div
      className="press flex h-8 w-7 items-center justify-center rounded-md text-subtle hover:bg-hover"
      title="Drag to move"
      aria-hidden
      onPointerDown={(e) => {
        if (e.button === 0) getCurrentWindow().startDragging().catch(() => {});
      }}
    >
      <GripVertical className="size-4" />
    </div>
  );

  return (
    <div
      data-phase={phase}
      className={cx("morph absolute inset-0 flex overflow-hidden rounded-[var(--radius)]", up ? "flex-col-reverse" : "flex-col")}
      style={{ clipPath: clip, "--enter-y": up ? "10px" : "-10px" } as React.CSSProperties}
    >
      {wide ? (
        <header data-tauri-drag-region className="morph-content relative flex h-[52px] shrink-0 items-center justify-center px-20">
          <WideTabs active={utility} onSelect={setUtility} />
          <div className="absolute right-3 top-0 flex h-[52px] items-center gap-0.5">
            <IconButton
              size="sm"
              label="More utilities"
              icon={LayoutGrid}
              active={moreOpen}
              aria-expanded={moreOpen}
              onClick={() => setMoreOpen((v) => !v)}
              className="text-subtle"
            />
            <IconButton size="sm" label="Settings" icon={SettingsIcon} onClick={() => openSettings()} className="text-subtle" />
          </div>
        </header>
      ) : (
      <header className="relative h-[52px] shrink-0">
        <div className="absolute top-0 flex h-[52px] items-center justify-between px-1.5" style={{ left: offX, width: PILL_W }}>
          {/* Selection pill that springs between slots (shared-element style). */}
          <span
            aria-hidden
            className="slide-indicator pointer-events-none absolute left-1.5 top-1.5 size-10 rounded-[min(calc(var(--radius)-6px),14px)] bg-accent"
            style={{
              transform: `translateX(${Math.max(0, activeSlot) * 48}px) scale(${phase === "open" && activeSlot >= 0 ? 1 : 0.6})`,
              opacity: phase === "open" && activeSlot >= 0 ? 1 : 0,
            }}
          />
          {slots}
        </div>
        <div
          className={cx("morph-content absolute top-0 flex h-[52px] items-center px-2", controlsOnRight ? "justify-start" : "justify-end")}
          style={controlsOnRight ? { left: 0, width: offX } : { left: offX + PILL_W, right: 0 }}
        >
          {grip}
        </div>
        <div
          className={cx("morph-content absolute top-0 flex h-[52px] items-center px-2", controlsOnRight ? "justify-end" : "justify-start")}
          style={controlsOnRight ? { left: offX + PILL_W, right: 0 } : { left: 0, width: offX }}
        >
          {controlsGroup}
        </div>
      </header>
      )}
      {!wide && <div className="morph-content h-px shrink-0 bg-line" />}
      <main className="morph-content relative min-h-0 flex-1">
        {error && (
          <div className="fade-in absolute inset-x-3 top-2 z-20">
            <ErrorNote error={error} onDismiss={onDismissError} />
          </div>
        )}
        {menu.mounted && (
          <div
            data-state={menu.state}
            className="pop-menu absolute inset-x-3 top-2 z-10 grid grid-cols-4 gap-1 rounded-xl border border-line bg-[var(--surface-solid)] p-2 shadow-xl"
            style={{ "--origin": controlsOnRight ? "top right" : "top left" } as React.CSSProperties}
          >
            {UTILITIES.map((u) => {
              const Icon = UTILITY_ICON[u];
              return (
                <button
                  key={u}
                  type="button"
                  onClick={() => setUtility(u)}
                  className={cx(
                    "press flex flex-col items-center gap-1 rounded-lg px-1 py-2 text-[11px]",
                    u === utility ? "bg-accent/15 text-accent" : "text-muted hover:bg-hover hover:text-fg",
                  )}
                >
                  <Icon className="size-[18px]" aria-hidden />
                  {UTILITY_LABEL[u]}
                </button>
              );
            })}
          </div>
        )}
        <div className="h-full">{children}</div>
      </main>
    </div>
  );
}
