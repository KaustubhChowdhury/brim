import { CalendarDays, ChevronLeft, ChevronRight, Pencil, Plus, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useLoader, useNow } from "../lib/hooks";
import { addDays, formatTime, fromLocalInput, monthGrid, startOfDay, toLocalInput } from "../lib/format";
import type { CalendarEvent } from "../lib/types";
import { Button, EmptyState, ErrorNote, IconButton, Toggle, cx, inputClass } from "../components/ui";

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
const monthFmt = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" });
const dayFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long" });

type Draft = { id: number | null; title: string; start: string; end: string; allDay: boolean; description: string };

export function CalendarPanel() {
  const now = useNow(60_000);
  const [cursor, setCursor] = useState(() => {
    const d = new Date();
    return { year: d.getFullYear(), month: d.getMonth() };
  });
  const [selected, setSelected] = useState(() => startOfDay(Date.now()));
  const [draft, setDraft] = useState<Draft | null>(null);
  const action = useAction();

  const grid = useMemo(() => monthGrid(cursor.year, cursor.month), [cursor]);
  const from = grid[0]!;
  const to = addDays(grid[grid.length - 1]!, 1);
  const events = useLoader(() => api.calendar.list(from, to), ["productivity:changed"], [from, to]);

  const byDay = useMemo(() => {
    const map = new Map<number, CalendarEvent[]>();
    for (const e of events.data ?? []) {
      // List multi-day events on every day they touch.
      for (let d = startOfDay(e.startAt); d <= e.endAt; d = addDays(d, 1)) {
        if (d < from || d >= to) continue;
        map.set(d, [...(map.get(d) ?? []), e]);
      }
    }
    return map;
  }, [events.data, from, to]);

  const dayEvents = byDay.get(selected) ?? [];
  const shift = (delta: number) =>
    setCursor(({ year, month }) => {
      const d = new Date(year, month + delta, 1);
      return { year: d.getFullYear(), month: d.getMonth() };
    });

  const newDraft = (day: number): Draft => {
    const base = new Date(day);
    base.setHours(day === startOfDay(Date.now()) ? Math.min(new Date().getHours() + 1, 23) : 9, 0, 0, 0);
    return {
      id: null,
      title: "",
      start: toLocalInput(base.getTime()),
      end: toLocalInput(base.getTime() + 3_600_000),
      allDay: false,
      description: "",
    };
  };

  if (draft) return <EventEditor draft={draft} onClose={() => setDraft(null)} />;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-3 pt-3">
        <IconButton label="Previous month" icon={ChevronLeft} onClick={() => shift(-1)} />
        <button
          type="button"
          className="rounded-md px-2 py-1 text-[13px] font-semibold hover:bg-hover"
          title="Go to today"
          onClick={() => {
            const d = new Date();
            setCursor({ year: d.getFullYear(), month: d.getMonth() });
            setSelected(startOfDay(Date.now()));
          }}
        >
          {monthFmt.format(new Date(cursor.year, cursor.month, 1))}
        </button>
        <IconButton label="Next month" icon={ChevronRight} onClick={() => shift(1)} />
      </div>
      <div className="grid grid-cols-7 px-3 pt-1 text-center text-[10px] font-medium uppercase text-subtle" aria-hidden>
        {WEEKDAYS.map((d) => (
          <div key={d} className="py-1">
            {d}
          </div>
        ))}
      </div>
      <div className="grid grid-cols-7 gap-0.5 px-3" aria-label="Days of the month">
        {grid.map((day) => {
          const inMonth = new Date(day).getMonth() === cursor.month;
          const isToday = day === startOfDay(now);
          const count = byDay.get(day)?.length ?? 0;
          return (
            <button
              key={day}
              type="button"
              aria-pressed={day === selected}
              aria-label={`${dayFmt.format(day)}${count ? `, ${count} event${count > 1 ? "s" : ""}` : ""}`}
              onClick={() => setSelected(day)}
              onDoubleClick={() => {
                setSelected(day);
                setDraft(newDraft(day));
              }}
              className={cx(
                "press relative flex h-8 items-center justify-center rounded-lg text-xs tabular-nums",
                day === selected
                  ? "bg-accent text-accent-contrast"
                  : isToday
                    ? "font-semibold text-accent hover:bg-hover"
                    : "hover:bg-hover",
                !inMonth && day !== selected && "text-subtle",
              )}
            >
              {new Date(day).getDate()}
              {count > 0 && (
                <span
                  className={cx("absolute bottom-1 size-1 rounded-full", day === selected ? "bg-accent-contrast" : "bg-accent")}
                  aria-hidden
                />
              )}
            </button>
          );
        })}
      </div>
      <div className="mt-2 flex items-center justify-between border-t border-line px-3 pt-2">
        <div className="text-xs font-semibold">{dayFmt.format(selected)}</div>
        <IconButton size="sm" label="Add event" icon={Plus} onClick={() => setDraft(newDraft(selected))} />
      </div>
      <div className="px-3">
        <ErrorNote error={action.error ?? events.error} onDismiss={() => action.setError(null)} />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {dayEvents.length === 0 ? (
          <EmptyState icon={CalendarDays} title="No events">
            Double-click a day or use + to add one.
          </EmptyState>
        ) : (
          <ul aria-label="Events">
            {dayEvents.map((e) => (
              <li key={e.id} className="row-in group flex items-start gap-2 rounded-lg px-2 py-1.5 transition-colors duration-150 hover:bg-hover">
                <span className="mt-1.5 size-2 shrink-0 rounded-full bg-accent" aria-hidden />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px]">{e.title}</div>
                  <div className="text-[11px] text-subtle">
                    {e.allDay ? "All day" : `${formatTime(e.startAt)} – ${formatTime(e.endAt)}`}
                  </div>
                  {e.description && <div className="mt-0.5 line-clamp-2 text-[11px] text-muted">{e.description}</div>}
                </div>
                <IconButton
                  size="sm"
                  label={`Edit ${e.title}`}
                  icon={Pencil}
                  className="opacity-50 group-hover:opacity-100 focus-visible:opacity-100"
                  onClick={() =>
                    setDraft({
                      id: e.id,
                      title: e.title,
                      start: toLocalInput(e.startAt),
                      end: toLocalInput(e.endAt),
                      allDay: e.allDay,
                      description: e.description ?? "",
                    })
                  }
                />
                <IconButton
                  size="sm"
                  label={`Delete ${e.title}`}
                  icon={Trash2}
                  className="opacity-50 group-hover:opacity-100 focus-visible:opacity-100"
                  onClick={() => action.run(() => api.calendar.remove(e.id))}
                />
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function EventEditor({ draft, onClose }: { draft: Draft; onClose: () => void }) {
  const [d, setD] = useState(draft);
  const action = useAction();
  let startAt = fromLocalInput(d.start);
  let endAt = fromLocalInput(d.end);
  if (d.allDay && startAt != null) {
    startAt = startOfDay(startAt);
    endAt = addDays(startAt, 1) - 1;
  }
  const rangeOk = startAt != null && endAt != null && endAt >= startAt;
  const valid = d.title.trim().length > 0 && rangeOk;

  const save = async () => {
    if (!valid || startAt == null || endAt == null) return;
    const input = { title: d.title, startAt, endAt, allDay: d.allDay, description: d.description.trim() || null };
    const r = await action.run(() => (d.id == null ? api.calendar.create(input) : api.calendar.update(d.id, input)));
    if (r) onClose();
  };

  return (
    <form
      className="flex h-full flex-col gap-2 p-3"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onClose();
        }
      }}
    >
      <div className="text-[13px] font-semibold">{d.id == null ? "New event" : "Edit event"}</div>
      <input
        autoFocus
        aria-label="Title"
        placeholder="Title"
        maxLength={200}
        value={d.title}
        onChange={(e) => setD({ ...d, title: e.target.value })}
        className={inputClass}
      />
      <Toggle label="All day" checked={d.allDay} onChange={(v) => setD({ ...d, allDay: v })} />
      <label className="text-[11px] text-muted">
        {d.allDay ? "Day" : "Starts"}
        <input
          type="datetime-local"
          value={d.start}
          onChange={(e) => setD({ ...d, start: e.target.value })}
          className={cx(inputClass, "mt-1 text-xs")}
        />
      </label>
      {!d.allDay && (
        <label className="text-[11px] text-muted">
          Ends
          <input
            type="datetime-local"
            value={d.end}
            min={d.start}
            onChange={(e) => setD({ ...d, end: e.target.value })}
            className={cx(inputClass, "mt-1 text-xs")}
          />
        </label>
      )}
      {!rangeOk && <p className="text-[11px] text-danger">The end time must be after the start time.</p>}
      <textarea
        aria-label="Description"
        placeholder="Description (optional)"
        maxLength={2000}
        value={d.description}
        onChange={(e) => setD({ ...d, description: e.target.value })}
        className={cx(inputClass, "h-auto min-h-0 flex-1 resize-none py-2 text-xs")}
      />
      <ErrorNote error={action.error} />
      <div className="flex justify-end gap-1.5">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" variant="primary" disabled={!valid} busy={action.busy}>
          Save
        </Button>
      </div>
    </form>
  );
}
