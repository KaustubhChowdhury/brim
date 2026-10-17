import { Bell, CalendarDays, Check, CircleCheck, Plus } from "lucide-react";
import { useState } from "react";
import { api, toAppError } from "../lib/ipc";
import { useAction, useLoader, useNow } from "../lib/hooks";
import { addDays, formatTime, startOfDay } from "../lib/format";
import { EmptyState, ErrorNote, IconButton, cx, inputClass } from "../components/ui";

const dateFmt = new Intl.DateTimeFormat(undefined, { weekday: "long", day: "numeric", month: "long" });

function greeting(now: number): string {
  const h = new Date(now).getHours();
  return h < 5 ? "Good night" : h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
}

/** A glance at today: agenda (events + reminders) beside the tasks that are due. */
export function TodayPanel() {
  const now = useNow(30_000);
  const day = startOfDay(now);
  const events = useLoader(() => api.calendar.list(day, addDays(day, 1)), ["productivity:changed"], [day]);
  const reminders = useLoader(api.reminders.list, ["productivity:changed"]);
  const tasks = useLoader(api.tasks.list, ["productivity:changed"]);
  const action = useAction();
  const [title, setTitle] = useState("");

  const agenda = [
    ...(events.data ?? []).map((e) => ({ key: `e${e.id}`, at: e.startAt, title: e.title, kind: "event" as const, allDay: e.allDay, end: e.endAt })),
    ...(reminders.data ?? [])
      .filter((r) => r.notificationState !== "dismissed" && (r.scheduledAt < addDays(day, 1)))
      .map((r) => ({ key: `r${r.id}`, at: r.scheduledAt, title: r.title, kind: "reminder" as const, allDay: false, end: r.scheduledAt })),
  ].sort((a, b) => a.at - b.at);
  const due = (tasks.data ?? []).filter(
    (t) => !t.completed && ((t.dueAt != null && startOfDay(t.dueAt) <= day) || (t.dueAt == null && t.priority === 3)),
  );

  const add = async () => {
    if (!title.trim()) return;
    try {
      await api.tasks.create({ title, dueAt: day });
      setTitle("");
    } catch (e) {
      action.setError(toAppError(e));
    }
  };

  return (
    <div className="flex h-full flex-col px-5 pb-4 pt-3">
      <div className="fade-in mb-3">
        <div className="text-[15px] font-semibold">{greeting(now)}</div>
        <div className="text-xs text-muted">{dateFmt.format(now)}</div>
      </div>
      <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />
      <div className="grid min-h-0 flex-1 grid-cols-2 gap-4">
        <section className="flex min-h-0 flex-col">
          <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-subtle">Agenda</h3>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {agenda.length === 0 ? (
              <EmptyState icon={CalendarDays} title="Nothing scheduled">
                Events and reminders for today appear here.
              </EmptyState>
            ) : (
              <ul className="space-y-1">
                {agenda.map((a) => (
                  <li key={a.key} className="row-in flex items-center gap-2.5 rounded-lg bg-field px-2.5 py-2">
                    {a.kind === "event" ? (
                      <CalendarDays className="size-4 shrink-0 text-accent" aria-hidden />
                    ) : (
                      <Bell className={cx("size-4 shrink-0", a.at <= now ? "text-warn" : "text-accent")} aria-hidden />
                    )}
                    <span className="min-w-0 flex-1 truncate text-[13px]">{a.title}</span>
                    <span className="shrink-0 text-[11px] tabular-nums text-muted">
                      {a.allDay ? "All day" : a.kind === "event" ? `${formatTime(a.at)}–${formatTime(a.end)}` : formatTime(a.at)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </section>
        <section className="flex min-h-0 flex-col">
          <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-subtle">Due today</h3>
          <form
            className="mb-1.5 flex gap-1.5"
            onSubmit={(e) => {
              e.preventDefault();
              add();
            }}
          >
            <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Add a task for today" aria-label="Add a task for today" maxLength={200} className={inputClass} />
            <IconButton label="Add task" icon={Plus} type="submit" disabled={!title.trim()} />
          </form>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {due.length === 0 ? (
              <EmptyState icon={CircleCheck} title="All clear" />
            ) : (
              <ul className="space-y-1">
                {due.map((t) => (
                  <li key={t.id} className="row-in flex items-center gap-2.5 rounded-lg px-2 py-1.5 transition-colors hover:bg-hover">
                    <button
                      type="button"
                      role="checkbox"
                      aria-checked={false}
                      aria-label={`Complete “${t.title}”`}
                      onClick={() => action.run(() => api.tasks.update(t.id, { completed: true }))}
                      className="press flex size-[18px] shrink-0 items-center justify-center rounded-full border border-subtle hover:border-accent"
                    >
                      <Check className="size-3 opacity-0" aria-hidden />
                    </button>
                    <span className="min-w-0 flex-1 truncate text-[13px]">{t.title}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
