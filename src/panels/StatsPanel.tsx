import { BatteryCharging, Battery, Clock, Cpu, HardDrive, MemoryStick, type LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { formatBytes } from "../lib/format";
import type { SystemStats } from "../lib/types";
import { ErrorNote, Spinner } from "../components/ui";

/** Live system readings, sampled once a second only while this panel is open. */
export function StatsPanel() {
  const [stats, setStats] = useState<SystemStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    const tick = () =>
      api.system
        .stats()
        .then((s) => alive && setStats(s))
        .catch((e) => alive && setError(e.message));
    tick();
    const id = window.setInterval(tick, 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  if (error) return <div className="p-4"><ErrorNote error={{ code: "stats", message: error }} /></div>;
  if (!stats) return <Spinner label="Reading system stats" />;

  const mem = stats.memoryTotal ? stats.memoryUsed / stats.memoryTotal : 0;
  const disk = stats.diskTotal ? stats.diskUsed / stats.diskTotal : 0;
  const b = stats.battery;
  return (
    <div className="flex h-full flex-col justify-center gap-5 px-6 py-4">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(140px,1fr))] gap-3">
        <Gauge icon={Cpu} label="CPU" value={(stats.cpuPercent ?? 0) / 100} detail={stats.cpuPercent == null ? "Measuring…" : `${Math.round(stats.cpuPercent)}%`} />
        <Gauge icon={MemoryStick} label="Memory" value={mem} detail={`${formatBytes(stats.memoryUsed)} of ${formatBytes(stats.memoryTotal)}`} />
        <Gauge icon={HardDrive} label={`Disk ${stats.diskLabel}`} value={disk} detail={`${formatBytes(stats.diskTotal - stats.diskUsed)} free`} />
        {b && (
          <Gauge
            icon={b.charging ? BatteryCharging : Battery}
            label="Battery"
            value={b.percent / 100}
            detail={b.charging ? "Charging" : b.pluggedIn ? "Plugged in" : "On battery"}
            invert
          />
        )}
      </div>
      <div className="flex items-center justify-center gap-2 text-xs text-muted">
        <Clock className="size-3.5" aria-hidden />
        Up for {formatUptime(stats.uptimeSecs)}
      </div>
    </div>
  );
}

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86_400);
  const h = Math.floor((secs % 86_400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return d > 0 ? `${d}d ${h}h ${m}m` : h > 0 ? `${h}h ${m}m` : `${m}m`;
}

/** Ring gauge. High usage turns warm; `invert` is for levels where low is bad (battery). */
function Gauge({ icon: Icon, label, value, detail, invert }: { icon: LucideIcon; label: string; value: number; detail: string; invert?: boolean }) {
  const v = Math.min(1, Math.max(0, value));
  const danger = invert ? v < 0.15 : v > 0.9;
  const warn = invert ? v < 0.3 : v > 0.75;
  const r = 30;
  const c = 2 * Math.PI * r;
  return (
    <div className="fade-in flex flex-col items-center gap-2 rounded-2xl bg-field px-3 py-4" role="meter" aria-label={label} aria-valuenow={Math.round(v * 100)} aria-valuemin={0} aria-valuemax={100}>
      <div className="relative size-20">
        <svg viewBox="0 0 72 72" className="size-full -rotate-90">
          <circle cx="36" cy="36" r={r} fill="none" stroke="var(--line)" strokeWidth="6" />
          <circle
            cx="36"
            cy="36"
            r={r}
            fill="none"
            stroke={danger ? "var(--danger)" : warn ? "var(--warn)" : "var(--accent)"}
            strokeWidth="6"
            strokeLinecap="round"
            strokeDasharray={c}
            strokeDashoffset={c * (1 - v)}
            className="gauge-arc"
          />
        </svg>
        <div className="absolute inset-0 flex items-center justify-center">
          <Icon className="size-5 text-muted" aria-hidden />
        </div>
      </div>
      <div className="text-center">
        <div className="text-[13px] font-semibold">{label}</div>
        <div className="text-[11px] tabular-nums text-muted">{detail}</div>
      </div>
    </div>
  );
}
