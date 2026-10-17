import { AppWindow, Bell, CalendarDays, Check, CircleCheck, Flag, ListTodo, Plus, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { api, toAppError } from "../lib/ipc";
import { useAction, useLoader, useNow } from "../lib/hooks";
import {
  addDays,
  formatDayMonth,
  formatTime,
  fromDateInput,
  fromLocalInput,
  isSameDay,
  nextQuarterHour,
  relativeTime,
  startOfDay,
  toDateInput,
  toLocalInput,
} from "../lib/format";
import type { AppEntry, AppError, Reminder, Task } from "../lib/types";
import { Button, EmptyState, ErrorNote, IconButton, Segmented, Spinner, cx, inputClass } from "../components/ui";

type Tab = "today" | "all" | "reminders";

const PRIORITY_LABEL = ["None", "Low", "Medium", "High"];
const PRIORITY_COLOR = ["", "text-subtle", "text-warn", "text-danger"];

/** Task due dates are stored as the start of the local day they are due. */
function taskDue(dueAt: number, now: number): { text: string; overdue: boolean; today: boolean } {
  const today = startOfDay(now);
  const day = startOfDay(dueAt);
  if (day === today) return { text: "Today", overdue: false, today: true };
  if (day === addDays(today, 1)) return { text: "Tomorrow", overdue: false, today: false };
  if (day < today) return { text: day === addDays(today, -1) ? "Yesterday" : formatDayMonth(day), overdue: true, today: false };
  return { text: formatDayMonth(day), overdue: false, today: false };
}

export function TasksPanel() {
  const [tab, setTab] = useState<Tab>("today");
  const tasks = useLoader(api.tasks.list, ["productivity:changed"]);
  const reminders = useLoader(api.reminders.list, ["productivity:changed"]);
  const action = useAction();
  const now = useNow(30_000);
  const dayStart = startOfDay(now);
  const events = useLoader(() => api.calendar.list(dayStart, addDays(dayStart, 1)), ["productivity:changed"], [dayStart]);
  const apps = useLoader(api.apps.list, ["apps:changed"]);

  const open = (tasks.data ?? []).filter((t) => !t.completed);
  const done = (tasks.data ?? []).filter((t) => t.completed);
  const todayTasks = useMemo(
    () => open.filter((t) => (t.dueAt != null && startOfDay(t.dueAt) <= startOfDay(now)) || (t.dueAt == null && t.priority === 3)),
    [open, now],
  );
  const activeReminders = (reminders.data ?? []).filter((r) => r.notificationState !== "dismissed");
  const todayReminders = activeReminders.filter(
    (r) => r.notificationState !== "scheduled" || isSameDay(r.scheduledAt, now) || r.scheduledAt < now,
  );
  const dueCount = activeReminders.filter((r) => r.notificationState === "notified" || r.notificationState === "missed").length;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between px-3 pt-3">
        <Segmented
          label="Tasks view"
          value={tab}
          onChange={setTab}
          options={[
            { value: "today", label: "Today" },
            { value: "all", label: "All tasks" },
            { value: "reminders", label: dueCount ? `Reminders (${dueCount})` : "Reminders" },
          ]}
        />
      </div>
      <div className="px-3 pt-2">
        <ErrorNote error={action.error ?? tasks.error ?? reminders.error} onDismiss={() => action.setError(null)} />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3 pt-1">
        {tasks.loading && !tasks.data ? (
          <Spinner />
        ) : tab === "today" ? (
          <>
            <AddTask defaultDue={startOfDay(now)} placeholder="Add a task for today" onError={action.setError} />
            {(events.data ?? []).length > 0 && (
              <ul className="mb-1" aria-label="Today's events">
                {(events.data ?? []).map((e) => (
                  <li key={e.id} className="row-in flex items-center gap-2 rounded-lg px-2 py-1.5">
                    <CalendarDays className="size-4 shrink-0 text-accent" aria-hidden />
                    <span className="min-w-0 flex-1 truncate text-[13px]">{e.title}</span>
                    <span className="shrink-0 text-[11px] text-subtle">
                      {e.allDay ? "All day" : `${formatTime(e.startAt)} – ${formatTime(e.endAt)}`}
                    </span>
                  </li>
                ))}
              </ul>
            )}
            {todayReminders.length > 0 && (
              <ul className="mb-1">
                {todayReminders.map((r) => (
                  <ReminderRow key={r.id} r={r} now={now} run={action.run} />
                ))}
              </ul>
            )}
            {todayTasks.length ? (
              <ul>
                {todayTasks.map((t) => (
                  <TaskRow key={t.id} task={t} now={now} run={action.run} />
                ))}
              </ul>
            ) : todayReminders.length === 0 && (events.data ?? []).length === 0 ? (
              <EmptyState icon={CircleCheck} title="Nothing due today">
                Add a task above, or plan ahead in All tasks.
              </EmptyState>
            ) : null}
          </>
        ) : tab === "all" ? (
          <>
            <AddTask defaultDue={null} placeholder="Add a task" onError={action.setError} withDate />
            {open.length ? (
              <ul>
                {open.map((t) => (
                  <TaskRow key={t.id} task={t} now={now} run={action.run} />
                ))}
              </ul>
            ) : (
              <EmptyState icon={ListTodo} title="No open tasks">
                Add a task to get started.
              </EmptyState>
            )}
            {done.length > 0 && (
              <details className="mt-2">
                <summary className="cursor-default px-2 py-1 text-xs text-subtle">
                  Completed ({done.length})
                  <button
                    type="button"
                    className="ml-2 text-subtle underline hover:text-fg"
                    onClick={(e) => {
                      e.preventDefault();
                      action.run(() => api.tasks.clearCompleted());
                    }}
                  >
                    Clear
                  </button>
                </summary>
                <ul>
                  {done.map((t) => (
                    <TaskRow key={t.id} task={t} now={now} run={action.run} />
                  ))}
                </ul>
              </details>
            )}
          </>
        ) : (
          <>
            <AddReminder now={now} apps={apps.data ?? []} onError={action.setError} />
            {activeReminders.length ? (
              <ul>
                {activeReminders.map((r) => (
                  <ReminderRow key={r.id} r={r} now={now} run={action.run} />
                ))}
              </ul>
            ) : (
              <EmptyState icon={Bell} title="No reminders">
                Reminders use Windows notifications and appear once at the chosen time.
              </EmptyState>
            )}
          </>
        )}
      </div>
    </div>
  );
}

type Run = <T>(fn: () => Promise<T>) => Promise<T | undefined>;

function TaskRow({ task, now, run }: { task: Task; now: number; run: Run }) {
  const due = task.dueAt != null ? taskDue(task.dueAt, now) : null;
  const [editing, setEditing] = useState<string | null>(null);
  const saveTitle = () => {
    const title = editing?.trim();
    setEditing(null);
    if (title && title !== task.title) run(() => api.tasks.update(task.id, { title }));
  };
  return (
    <li className="row-in group flex items-center gap-2 rounded-lg px-2 py-1.5 transition-colors duration-150 hover:bg-hover">
      <button
        type="button"
        role="checkbox"
        aria-checked={task.completed}
        aria-label={task.completed ? `Mark “${task.title}” as not done` : `Complete “${task.title}”`}
        onClick={() => run(() => api.tasks.update(task.id, { completed: !task.completed }))}
        className={cx(
          "press flex size-[18px] shrink-0 items-center justify-center rounded-full border",
          task.completed ? "border-accent bg-accent text-accent-contrast" : "border-subtle hover:border-accent",
        )}
      >
        {task.completed && <Check className="pop size-3" aria-hidden />}
      </button>
      <div className="min-w-0 flex-1">
        {editing != null ? (
          <input
            autoFocus
            aria-label="Task title"
            value={editing}
            maxLength={200}
            onChange={(e) => setEditing(e.target.value)}
            onBlur={saveTitle}
            onKeyDown={(e) => {
              if (e.key === "Enter") saveTitle();
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setEditing(null);
              }
            }}
            className="h-6 w-full rounded-md border border-line bg-field px-1.5 text-[13px] outline-none focus:border-accent"
          />
        ) : (
          <div
            title={task.completed ? undefined : "Double-click to edit"}
            onDoubleClick={() => !task.completed && setEditing(task.title)}
            className={cx("truncate text-[13px] transition-colors duration-200", task.completed && "text-subtle line-through")}
          >
            {task.title}
          </div>
        )}
        {(due || task.groupName) && !task.completed && (
          <div className="flex gap-1.5 text-[11px]">
            {due && <span className={due.overdue ? "text-danger" : due.today ? "text-accent" : "text-subtle"}>{due.text}</span>}
            {task.groupName && <span className="text-subtle">· {task.groupName}</span>}
          </div>
        )}
      </div>
      {!task.completed && (
        <button
          type="button"
          title={`Priority: ${PRIORITY_LABEL[task.priority]} (click to change)`}
          aria-label={`Priority ${PRIORITY_LABEL[task.priority]}. Change priority`}
          onClick={() => run(() => api.tasks.update(task.id, { priority: (task.priority + 1) % 4 }))}
          className={cx(
            "press flex size-6 items-center justify-center rounded-md hover:bg-hover",
            task.priority === 0 && "opacity-0 group-hover:opacity-60 focus-visible:opacity-100",
          )}
        >
          <Flag className={cx("size-3.5", task.priority ? PRIORITY_COLOR[task.priority] : "text-subtle")} aria-hidden />
        </button>
      )}
      <IconButton
        size="sm"
        label={`Delete “${task.title}”`}
        icon={Trash2}
        className="opacity-50 group-hover:opacity-100 focus-visible:opacity-100"
        onClick={() => run(() => api.tasks.remove(task.id))}
      />
    </li>
  );
}

function AddTask({
  defaultDue,
  placeholder,
  onError,
  withDate,
}: {
  defaultDue: number | null;
  placeholder: string;
  onError: (e: AppError) => void;
  withDate?: boolean;
}) {
  const [title, setTitle] = useState("");
  const [due, setDue] = useState("");
  const [priority, setPriority] = useState(0);
  const [group, setGroup] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!title.trim() || busy) return;
    setBusy(true);
    try {
      const dueAt = withDate ? (due ? fromDateInput(due) : null) : defaultDue;
      await api.tasks.create({ title, dueAt, priority, groupName: group.trim() || null });
      setTitle("");
      setDue("");
      setPriority(0);
    } catch (e) {
      onError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="mb-1 flex flex-wrap items-center gap-1.5 px-1 py-1"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <input
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        maxLength={200}
        className={cx(inputClass, "min-w-0 flex-1 basis-40")}
      />
      {withDate && (
        <>
          <input
            aria-label="Group"
            placeholder="Group"
            value={group}
            maxLength={40}
            onChange={(e) => setGroup(e.target.value)}
            className={cx(inputClass, "w-[84px] shrink-0 text-xs")}
          />
          <input
            type="date"
            aria-label="Due date"
            value={due}
            min={toDateInput(Date.now())}
            onChange={(e) => setDue(e.target.value)}
            className={cx(inputClass, "w-[118px] shrink-0 px-1.5 text-xs")}
          />
          <select
            aria-label="Priority"
            value={priority}
            onChange={(e) => setPriority(Number(e.target.value))}
            className={cx(inputClass, "w-[74px] shrink-0 px-1.5 text-xs")}
          >
            {PRIORITY_LABEL.map((l, i) => (
              <option key={l} value={i}>
                {i === 0 ? "Priority" : l}
              </option>
            ))}
          </select>
        </>
      )}
      <IconButton label="Add task" icon={Plus} type="submit" disabled={!title.trim() || busy} />
    </form>
  );
}

function ReminderRow({ r, now, run }: { r: Reminder; now: number; run: Run }) {
  const due = r.notificationState === "notified" || r.notificationState === "missed";
  return (
    <li className="row-in group flex items-center gap-2 rounded-lg px-2 py-1.5 transition-colors duration-150 hover:bg-hover">
      <Bell className={cx("size-4 shrink-0", due ? "text-warn" : "text-accent")} aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px]">{r.title}</div>
        <div className={cx("text-[11px]", due ? "text-warn" : "text-subtle")}>
          {r.notificationState === "missed"
            ? `Missed · ${formatDayMonth(r.scheduledAt)} ${formatTime(r.scheduledAt)}`
            : due
              ? `Due ${relativeTime(r.scheduledAt, now)}`
              : `${isSameDay(r.scheduledAt, now) ? "Today" : formatDayMonth(r.scheduledAt)} at ${formatTime(r.scheduledAt)}`}
        </div>
      </div>
      {due ? (
        <div className="flex shrink-0 items-center gap-1">
          {[
            { label: "+10m", ms: 10 * 60_000 },
            { label: "+1h", ms: 60 * 60_000 },
          ].map((o) => (
            <Button
              key={o.label}
              small
              variant="ghost"
              title={`Snooze ${o.label.slice(1)}`}
              onClick={() =>
                run(() => api.reminders.update(r.id, { title: r.title, scheduledAt: Date.now() + o.ms, launchAppId: r.launchAppId }))
              }
            >
              {o.label}
            </Button>
          ))}
          <Button small onClick={() => run(() => api.reminders.dismiss(r.id))}>
            Dismiss
          </Button>
        </div>
      ) : (
        <IconButton
          size="sm"
          label={`Delete reminder “${r.title}”`}
          icon={Trash2}
          className="opacity-50 group-hover:opacity-100 focus-visible:opacity-100"
          onClick={() => run(() => api.reminders.remove(r.id))}
        />
      )}
    </li>
  );
}

function AddReminder({ now, apps, onError }: { now: number; apps: AppEntry[]; onError: (e: AppError) => void }) {
  const [title, setTitle] = useState("");
  const [appId, setAppId] = useState("");
  const [when, setWhen] = useState(() => toLocalInput(nextQuarterHour(now)));
  const [busy, setBusy] = useState(false);
  const scheduledAt = fromLocalInput(when);
  const valid = title.trim().length > 0 && scheduledAt != null && scheduledAt > Date.now();

  return (
    <form
      className="mb-1 space-y-1.5 px-1 py-1"
      onSubmit={async (e) => {
        e.preventDefault();
        if (!valid || busy || scheduledAt == null) return;
        setBusy(true);
        try {
          await api.reminders.create({ title, scheduledAt, launchAppId: appId || null });
          setTitle("");
          setAppId("");
          setWhen(toLocalInput(nextQuarterHour(Date.now())));
        } catch (err) {
          onError(toAppError(err));
        } finally {
          setBusy(false);
        }
      }}
    >
      <input
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Remind me to…"
        aria-label="Reminder"
        maxLength={200}
        className={inputClass}
      />
      <div className="flex items-center gap-1.5">
        <input
          type="datetime-local"
          aria-label="Reminder time"
          value={when}
          onChange={(e) => setWhen(e.target.value)}
          className={cx(inputClass, "text-xs")}
        />
        <Button type="submit" variant="primary" icon={Bell} disabled={!valid} busy={busy}>
          Remind
        </Button>
      </div>
      <label className="flex items-center gap-1.5 text-[11px] text-muted">
        <AppWindow className="size-3.5 shrink-0" aria-hidden />
        Also open
        <select
          aria-label="App to open with the reminder"
          value={appId}
          onChange={(e) => setAppId(e.target.value)}
          className={cx(inputClass, "h-7 flex-1 text-xs")}
        >
          <option value="">No app</option>
          {apps
            .filter((a) => a.available)
            .map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
        </select>
      </label>
      {scheduledAt != null && scheduledAt <= Date.now() && <p className="text-[11px] text-danger">Choose a time in the future.</p>}
    </form>
  );
}
