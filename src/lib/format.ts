// Pure formatting and date helpers. All timestamps are Unix milliseconds (UTC);
// everything shown to the user is in the local time zone.

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function addDays(ms: number, days: number): number {
  const d = new Date(ms);
  d.setDate(d.getDate() + days);
  return d.getTime();
}

export function endOfDay(ms: number): number {
  return addDays(startOfDay(ms), 1);
}

export function isSameDay(a: number, b: number): boolean {
  return startOfDay(a) === startOfDay(b);
}

const timeFmt = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const dayMonthFmt = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const weekdayFmt = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const fullFmt = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export const formatTime = (ms: number) => timeFmt.format(ms);
export const formatDayMonth = (ms: number) => dayMonthFmt.format(ms);
export const formatDateTime = (ms: number) => fullFmt.format(ms);

/** Short recency label: "just now", "5 min ago", "3 h ago", "Yesterday", "Mon", "12 Oct". */
export function relativeTime(ms: number, now: number = Date.now()): string {
  const diff = now - ms;
  if (diff < 45_000) return "just now";
  if (diff < HOUR) return `${Math.max(1, Math.round(diff / MINUTE))} min ago`;
  if (isSameDay(ms, now)) return `${Math.floor(diff / HOUR)} h ago`;
  if (isSameDay(ms, addDays(now, -1))) return "Yesterday";
  if (diff < 6 * DAY) return weekdayFmt.format(ms);
  return dayMonthFmt.format(ms);
}

/** Label for a due date relative to today. */
export function dueLabel(dueAt: number, now: number = Date.now()): { text: string; overdue: boolean; today: boolean } {
  const today = isSameDay(dueAt, now);
  const overdue = dueAt < now && !today;
  if (today) return { text: `Today ${timeFmt.format(dueAt)}`, overdue: false, today: true };
  if (isSameDay(dueAt, addDays(now, 1))) return { text: `Tomorrow ${timeFmt.format(dueAt)}`, overdue: false, today: false };
  if (isSameDay(dueAt, addDays(now, -1))) return { text: "Yesterday", overdue, today: false };
  return { text: dayMonthFmt.format(dueAt), overdue, today: false };
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "0 B";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 10 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

/** 0:05, 3:07, 1:02:03 */
export function formatDuration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** Value for <input type="datetime-local"> in local time. */
export function toLocalInput(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** Parses a datetime-local value as local time. Returns null for empty or invalid input. */
export function fromLocalInput(value: string): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(value.trim());
  if (!m) return null;
  const [, y, mo, d, h, mi] = m.map(Number) as [number, number, number, number, number, number];
  const date = new Date(y, mo - 1, d, h, mi, 0, 0);
  if (date.getMonth() !== mo - 1 || date.getDate() !== d) return null; // e.g. 31 Feb
  return date.getTime();
}

/** Value for <input type="date"> in local time. */
export function toDateInput(ms: number): string {
  return toLocalInput(ms).slice(0, 10);
}

export function fromDateInput(value: string): number | null {
  return fromLocalInput(`${value}T00:00`);
}

/** 6x7 grid of day starts covering `year`/`month` (0-based), weeks starting Monday. */
export function monthGrid(year: number, month: number): number[] {
  const first = new Date(year, month, 1);
  const offset = (first.getDay() + 6) % 7; // Monday = 0
  const start = new Date(year, month, 1 - offset).getTime();
  return Array.from({ length: 42 }, (_, i) => addDays(start, i));
}

/** Rounds up to the next quarter hour, at least `minAheadMin` minutes ahead. */
export function nextQuarterHour(now: number = Date.now(), minAheadMin = 5): number {
  const d = new Date(now + minAheadMin * MINUTE);
  const mins = d.getMinutes();
  d.setMinutes(Math.ceil(mins / 15) * 15, 0, 0);
  return d.getTime();
}

/** Media position, interpolated while playing, clamped to the duration. */
export function livePosition(
  positionMs: number | null,
  durationMs: number | null,
  updatedAt: number | null,
  playing: boolean,
  now: number = Date.now(),
): number | null {
  if (positionMs == null || durationMs == null) return null;
  const elapsed = playing && updatedAt != null ? Math.max(0, now - updatedAt) : 0;
  return Math.min(durationMs, Math.max(0, positionMs + elapsed));
}

export function pluralize(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}
