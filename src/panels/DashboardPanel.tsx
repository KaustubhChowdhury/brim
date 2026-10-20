import {
  ChevronLeft,
  ChevronRight,
  Cpu,
  HardDrive,
  Laptop,
  MemoryStick,
  Monitor,
  Moon,
  Music,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Sun,
  Sunrise,
  Sunset,
  Timer,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useLoader, useNow } from "../lib/hooks";
import { useMediaState } from "../lib/media";
import { useDock } from "../dock/context";
import { cx } from "../components/ui";
import type { MediaCommand, SystemInfo, SystemStats } from "../lib/types";

/**
 * The wide layout's home tab: a calm overview with one job per card. Everything shown is
 * read locally; live usage is sampled only while this tab is open.
 */
export function DashboardPanel() {
  const now = useNow(1000);
  const stats = useStats();
  const [info, setInfo] = useState<SystemInfo | null>(null);
  useEffect(() => {
    api.system.info().then(setInfo).catch(() => {});
  }, []);

  return (
    <div className="dash-grid h-full px-4 pb-4 pt-3">
      <Greeting now={now} name={info?.userName ?? ""} />
      <SystemCard info={info} uptime={stats?.uptimeSecs ?? null} />
      <ClockCard now={now} />
      <CalendarCard now={now} />
      <UsageBars stats={stats} />
      <NowPlayingCard />
    </div>
  );
}

function useStats(): SystemStats | null {
  const [stats, setStats] = useState<SystemStats | null>(null);
  useEffect(() => {
    let alive = true;
    const tick = () => api.system.stats().then((s) => alive && setStats(s)).catch(() => {});
    tick();
    const id = window.setInterval(tick, 2000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);
  return stats;
}

// ---------------- Greeting ----------------

function dayPart(hour: number): { greeting: string; icon: LucideIcon } {
  if (hour < 5) return { greeting: "Good night", icon: Moon };
  if (hour < 8) return { greeting: "Good morning", icon: Sunrise };
  if (hour < 12) return { greeting: "Good morning", icon: Sun };
  if (hour < 17) return { greeting: "Good afternoon", icon: Sun };
  if (hour < 20) return { greeting: "Good evening", icon: Sunset };
  return { greeting: "Good evening", icon: Moon };
}

function Greeting({ now, name }: { now: number; name: string }) {
  const date = new Date(now);
  const { greeting, icon: Icon } = dayPart(date.getHours());
  const first = name.split(/\s+/)[0] ?? "";
  return (
    <section className="dash-card [grid-area:greet] flex items-center gap-3 px-4">
      <Icon className="size-8 shrink-0 text-accent" strokeWidth={1.7} aria-hidden />
      <div className="min-w-0">
        <div className="truncate text-[17px] font-semibold leading-tight text-accent">
          {greeting}
          {first && <span className="capitalize">, {first}</span>}
        </div>
        <div className="truncate text-[12px] text-muted">
          {date.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}
        </div>
      </div>
    </section>
  );
}

// ---------------- System ----------------

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86_400);
  const h = Math.floor((secs % 86_400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `up ${d}d ${h}h`;
  if (h > 0) return `up ${h}h ${m}m`;
  return `up ${m}m`;
}

function SystemCard({ info, uptime }: { info: SystemInfo | null; uptime: number | null }) {
  const initial = (info?.userName.trim()[0] ?? "").toUpperCase();
  const rows: [LucideIcon, string][] = [
    [Monitor, info ? `${info.osName}${info.osVersion ? ` ${info.osVersion}` : ""}` : "…"],
    [Laptop, info ? info.computerName || "This PC" : "…"],
    [Timer, uptime == null ? "…" : formatUptime(uptime)],
  ];
  return (
    <section className="dash-card [grid-area:sys] flex items-center gap-3.5 px-4">
      <div className="dash-avatar flex size-14 shrink-0 items-center justify-center rounded-full text-[22px] font-semibold" title={info?.userName}>
        {initial || "·"}
      </div>
      <ul className="min-w-0 space-y-1 text-[12px]">
        {rows.map(([Icon, text], i) => (
          <li key={i} className="flex min-w-0 items-center gap-2">
            <Icon className="size-3.5 shrink-0 text-accent" aria-hidden />
            <span className="truncate text-fg/90">{text}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

// ---------------- Clock ----------------

function ClockCard({ now }: { now: number }) {
  const d = new Date(now);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return (
    <section
      className="dash-card [grid-area:clock] flex flex-col items-center justify-center gap-1"
      aria-label={d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}
    >
      <div className="text-[34px] font-semibold leading-none tabular-nums tracking-tight">{hh}</div>
      <div className="flex gap-1 py-1" aria-hidden>
        {[0, 1, 2].map((i) => (
          <span key={i} className={cx("dash-dot size-1.5 rounded-full bg-accent", d.getSeconds() % 3 === i && "dash-dot-on")} />
        ))}
      </div>
      <div className="text-[34px] font-semibold leading-none tabular-nums tracking-tight">{mm}</div>
      <div className="mt-2 text-[11px] text-muted">{d.toLocaleDateString(undefined, { weekday: "short", day: "numeric" })}</div>
    </section>
  );
}

// ---------------- Calendar ----------------

const DAY_MS = 86_400_000;

function CalendarCard({ now }: { now: number }) {
  const today = new Date(now);
  const [offset, setOffset] = useState(0);
  const month = new Date(today.getFullYear(), today.getMonth() + offset, 1);
  // Six Monday-first weeks covering the month.
  const start = new Date(month);
  start.setDate(1 - ((month.getDay() + 6) % 7));
  const from = start.getTime();
  const to = from + 42 * DAY_MS + DAY_MS;
  const events = useLoader(() => api.calendar.list(from, to), ["productivity:changed"], [from, to]);
  const busy = useMemo(() => {
    const days = new Set<string>();
    for (const e of events.data ?? []) days.add(new Date(e.startAt).toDateString());
    return days;
  }, [events.data]);

  const cells = Array.from({ length: 42 }, (_, i) => {
    const d = new Date(start);
    d.setDate(start.getDate() + i);
    return d;
  });
  const todayKey = today.toDateString();

  return (
    <section className="dash-card [grid-area:cal] flex flex-col px-3 pb-2 pt-2">
      <div className="flex items-center justify-between px-1">
        <button
          type="button"
          onClick={() => setOffset(0)}
          className="press text-[12px] font-semibold"
          title={offset ? "Back to this month" : undefined}
        >
          {month.toLocaleDateString(undefined, { month: "long", year: "numeric" })}
        </button>
        <div className="flex">
          <button type="button" aria-label="Previous month" onClick={() => setOffset((o) => o - 1)} className="press flex size-6 items-center justify-center rounded-full text-muted hover:bg-hover hover:text-fg">
            <ChevronLeft className="size-3.5" aria-hidden />
          </button>
          <button type="button" aria-label="Next month" onClick={() => setOffset((o) => o + 1)} className="press flex size-6 items-center justify-center rounded-full text-muted hover:bg-hover hover:text-fg">
            <ChevronRight className="size-3.5" aria-hidden />
          </button>
        </div>
      </div>
      <div className="mt-1 grid flex-1 grid-cols-7 grid-rows-[auto_repeat(6,1fr)] text-center text-[11px]">
        {["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].map((w) => (
          <div key={w} className="pb-0.5 font-semibold text-muted">
            {w}
          </div>
        ))}
        {cells.map((d) => {
          const inMonth = d.getMonth() === month.getMonth();
          const isToday = d.toDateString() === todayKey;
          return (
            <div key={d.getTime()} className="flex items-center justify-center">
              <span
                className={cx(
                  "relative flex size-[22px] items-center justify-center rounded-full tabular-nums",
                  isToday ? "bg-accent font-semibold text-accent-contrast" : inMonth ? "text-fg/90" : "text-subtle/60",
                )}
              >
                {d.getDate()}
                {busy.has(d.toDateString()) && !isToday && (
                  <span className="absolute -bottom-0.5 size-1 rounded-full bg-accent" aria-label="Has events" />
                )}
              </span>
            </div>
          );
        })}
      </div>
    </section>
  );
}

// ---------------- Usage ----------------

function UsageBars({ stats }: { stats: SystemStats | null }) {
  const bars: { icon: LucideIcon; label: string; value: number | null }[] = [
    { icon: Cpu, label: "CPU", value: stats?.cpuPercent != null ? stats.cpuPercent / 100 : null },
    { icon: MemoryStick, label: "Memory", value: stats && stats.memoryTotal ? stats.memoryUsed / stats.memoryTotal : null },
    { icon: HardDrive, label: "Disk", value: stats && stats.diskTotal ? stats.diskUsed / stats.diskTotal : null },
  ];
  return (
    <section className="dash-card [grid-area:bars] flex justify-center gap-3 px-2 pb-2.5 pt-3">
      {bars.map(({ icon: Icon, label, value }) => (
        <div
          key={label}
          className="flex flex-col items-center gap-2"
          title={`${label}: ${value == null ? "measuring…" : `${Math.round(value * 100)}%`}`}
          role="meter"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={value == null ? undefined : Math.round(value * 100)}
        >
          <div className="relative w-2.5 flex-1 overflow-hidden rounded-full bg-line">
            <div className="dash-bar absolute inset-0 rounded-full" style={{ transform: `scaleY(${value ?? 0})` }} />
          </div>
          <Icon className="size-3.5 text-muted" aria-hidden />
        </div>
      ))}
    </section>
  );
}

// ---------------- Now playing ----------------

function NowPlayingCard() {
  const media = useMediaState();
  const action = useAction();
  const { setUtility } = useDock();
  const active = media?.active ?? null;
  const playing = active?.status === "playing";
  const send = (cmd: MediaCommand) => action.run(() => api.media.command(cmd));

  return (
    <section className="dash-card [grid-area:music] flex min-w-0 flex-col items-center justify-center gap-3 px-4 py-4 text-center">
      <button
        type="button"
        onClick={() => setUtility("controls")}
        title="Open Controls"
        className="press-soft press relative"
      >
        {playing && <span className="absolute -left-3 top-1/2 h-4 w-1.5 -translate-y-1/2 rounded-full bg-accent" aria-hidden />}
        <div className={cx("dash-disc size-[118px] overflow-hidden rounded-full", playing && "dash-disc-spin")}>
          {active?.artwork ? (
            <img key={active.artwork.length + active.title} src={active.artwork} alt="" className="fade-in size-full object-cover" draggable={false} />
          ) : (
            <div className="flex size-full items-center justify-center bg-field text-subtle">
              <Music className="size-9" aria-hidden />
            </div>
          )}
        </div>
      </button>
      {active ? (
        <>
          <div key={`${active.sessionId}|${active.title}`} className="fade-in w-full min-w-0 space-y-0.5">
            <div className="truncate text-[13px] font-semibold" title={active.title}>
              {active.title || "Unknown title"}
            </div>
            <div className="truncate text-[11px] uppercase tracking-wide text-muted" title={active.album || active.appName}>
              {active.album || active.appName}
            </div>
            <div className="truncate text-[12px] text-fg/80" title={active.artist}>
              {active.artist}
            </div>
          </div>
          <div className="flex items-center gap-1">
            <DiscButton label="Previous track" disabled={!active.canPrevious || action.busy} onClick={() => send("previous")}>
              <SkipBack className="size-4 fill-current" aria-hidden />
            </DiscButton>
            <DiscButton label={playing ? "Pause" : "Play"} disabled={!active.canPlayPause || action.busy} onClick={() => send("playPause")}>
              {playing ? <Pause key="pause" className="pop size-4 fill-current" aria-hidden /> : <Play key="play" className="pop size-4 fill-current" aria-hidden />}
            </DiscButton>
            <DiscButton label="Next track" disabled={!active.canNext || action.busy} onClick={() => send("next")}>
              <SkipForward className="size-4 fill-current" aria-hidden />
            </DiscButton>
          </div>
        </>
      ) : (
        <div className="space-y-0.5">
          <div className="text-[13px] font-semibold">Nothing playing</div>
          <div className="text-[11px] text-muted">Music you play shows up here.</div>
        </div>
      )}
    </section>
  );
}

function DiscButton({ label, disabled, onClick, children }: { label: string; disabled: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="press flex size-8 items-center justify-center rounded-full text-fg hover:bg-hover disabled:opacity-30"
    >
      {children}
    </button>
  );
}
