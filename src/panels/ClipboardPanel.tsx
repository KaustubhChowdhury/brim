import { Check, ChevronDown, Clipboard, Pin, PinOff, Search, ShieldCheck, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, toAppError } from "../lib/ipc";
import { useAction, useLoader, useNow, useSettings, useTauriEvent } from "../lib/hooks";
import { formatBytes, formatTime, pluralize, relativeTime } from "../lib/format";
import type { ClipItem, ClipNotice, ClipboardStatus } from "../lib/types";
import { Button, EmptyState, ErrorNote, IconButton, Spinner, cx, inputClass } from "../components/ui";
import { useDock } from "../dock/context";

function noticeText(n: ClipNotice): string {
  switch (n.kind) {
    case "tooLarge":
      return `Your last copy (${formatBytes(n.sizeBytes ?? 0)}) was too large to keep.`;
    case "secret":
      return "A copy that looked like a password or key was not saved.";
    case "private":
      return "An app marked its copy as private, so it was not saved.";
    case "ignoredApp":
      return `Copies from ${n.app ?? "an ignored app"} are not saved.`;
  }
}

export function ClipboardPanel() {
  const { settings, update } = useSettings();
  const { collapse, openSettings } = useDock();
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [filter, setFilter] = useState<"all" | "pinned">("all");
  const [selected, setSelected] = useState(0);
  const [copiedId, setCopiedId] = useState<number | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  // Expanded entry: shows its full text without copying it.
  const [expanded, setExpanded] = useState<{ id: number; text: string | null } | null>(null);
  const toggleExpand = (item: ClipItem) => {
    if (expanded?.id === item.id) return setExpanded(null);
    setExpanded({ id: item.id, text: null });
    api.clipboard
      .get(item.id)
      .then((text) => setExpanded((e) => (e?.id === item.id ? { id: item.id, text } : e)))
      .catch((e) => action.setError(toAppError(e)));
  };
  const action = useAction();
  const listRef = useRef<HTMLUListElement>(null);
  const now = useNow(30_000);

  useEffect(() => {
    const t = window.setTimeout(() => setDebounced(query), 120);
    return () => window.clearTimeout(t);
  }, [query]);

  const status = useLoader<ClipboardStatus>(api.clipboard.status, ["clipboard:changed", "settings:changed"]);
  const page = useLoader(() => api.clipboard.list(debounced, filter, 200, 0), ["clipboard:changed"], [debounced, filter]);
  const [notice, setNotice] = useState<ClipNotice | null>(null);
  useTauriEvent<ClipNotice>("clipboard:notice", setNotice);
  useEffect(() => {
    if (status.data?.lastNotice) setNotice(status.data.lastNotice);
  }, [status.data?.lastNotice]);

  const items = page.data?.items ?? [];
  useEffect(() => setSelected(0), [debounced, filter]);

  const copy = async (item: ClipItem) => {
    try {
      await api.clipboard.copy(item.id);
    } catch (e) {
      action.setError(toAppError(e));
      return;
    }
    setCopiedId(item.id);
    window.setTimeout(() => setCopiedId(null), 900);
    if (settings.collapseAfterCopy) window.setTimeout(() => collapse(true), 250);
  };

  const onListKey = (e: React.KeyboardEvent) => {
    if (!items.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.min(items.length - 1, Math.max(0, selected + (e.key === "ArrowDown" ? 1 : -1)));
      setSelected(next);
      listRef.current?.querySelectorAll<HTMLElement>("[data-row]")[next]?.focus();
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = items[selected];
      if (item) copy(item);
    } else if (e.key === "Delete") {
      const item = items[selected];
      if (item) action.run(() => api.clipboard.remove(item.id));
    }
  };

  const capture = status.data?.capture ?? settings.clipboardCapture;
  const recentNotice = notice && now - notice.at < 5 * 60_000 ? notice : null;

  return (
    <div className="flex h-full flex-col">
      <div className="space-y-2 px-3 pt-3">
        <CaptureBanner
          status={status.data}
          capture={capture}
          onPause={(m) => action.run(() => api.clipboard.pause(m))}
          onResume={() => action.run(() => api.clipboard.resume())}
          onTurnOn={() => action.run(() => update({ clipboardCapture: "on" }))}
          onSettings={() => openSettings("clipboard")}
        />
        {recentNotice && (
          <p className="flex items-center gap-2 px-1 text-[11.5px] text-subtle">
            <ShieldCheck className="size-3 shrink-0" aria-hidden />
            <span className="flex-1">{noticeText(recentNotice)}</span>
            <button type="button" className="hover:text-fg" onClick={() => setNotice(null)}>
              Hide
            </button>
          </p>
        )}
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-2 size-4 text-subtle" aria-hidden />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape" && query) {
                  e.preventDefault();
                  e.stopPropagation();
                  setQuery("");
                } else if (e.key === "ArrowDown") {
                  e.preventDefault();
                  listRef.current?.querySelector<HTMLElement>("[data-row]")?.focus();
                } else if (e.key === "Enter" && items[0]) {
                  e.preventDefault();
                  copy(items[0]);
                }
              }}
              placeholder="Search clipboard history"
              aria-label="Search clipboard history"
              className={cx(inputClass, "pl-8")}
            />
          </div>
          <div className="flex shrink-0 gap-0.5" role="radiogroup" aria-label="Filter">
            {(["all", "pinned"] as const).map((f) => (
              <button
                key={f}
                type="button"
                role="radio"
                aria-checked={filter === f}
                onClick={() => setFilter(f)}
                className={cx(
                  "press h-7 rounded-full px-3 text-[11.5px] font-semibold",
                  filter === f ? "bg-accent/15 text-accent" : "text-muted hover:text-fg",
                )}
              >
                {f === "all" ? "All" : "Pinned"}
              </button>
            ))}
          </div>
        </div>
        <ErrorNote error={action.error ?? page.error} onDismiss={() => action.setError(null)} />
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        {page.loading && !page.data ? (
          <Spinner />
        ) : items.length === 0 ? (
          debounced ? (
            <EmptyState icon={Search} title="No matches">
              Nothing in your history contains “{debounced}”.
            </EmptyState>
          ) : filter === "pinned" ? (
            <EmptyState icon={Pin} title="No pinned items">
              Pin text you reuse often and it will stay here.
            </EmptyState>
          ) : (
            <EmptyState icon={Clipboard} title={capture === "on" ? "Nothing copied yet" : "Clipboard history is empty"}>
              {capture === "on"
                ? "Copy some text in any app and it will appear here."
                : "Turn on clipboard history to keep text you copy. It stays on this PC."}
            </EmptyState>
          )
        ) : (
          <ul ref={listRef} className="space-y-1" onKeyDown={onListKey} aria-label="Clipboard history">
            {items.map((item, i) => (
              <li key={item.id} className="row-in group relative">
                <button
                  type="button"
                  data-row
                  tabIndex={i === selected ? 0 : -1}
                  onFocus={() => setSelected(i)}
                  onClick={() => copy(item)}
                  title="Copy to clipboard"
                  className={cx(
                    "press press-soft w-full rounded-lg px-2.5 py-2 pr-24 text-left",
                    i === selected ? "bg-hover" : "hover:bg-hover",
                  )}
                >
                  <div className={cx("line-clamp-2 break-all text-[13px] leading-snug", item.lineCount > 1 && "font-mono text-[12px]")}>
                    {item.preview}
                    {item.truncated && "…"}
                  </div>
                  <div className="mt-0.5 flex items-center gap-1.5 text-[11px] text-subtle">
                    {copiedId === item.id ? (
                      <span className="pop flex items-center gap-1 text-ok">
                        <Check className="size-3" aria-hidden /> Copied
                      </span>
                    ) : (
                      <span title={formatTime(item.lastUsedAt)}>{relativeTime(item.lastUsedAt, now)}</span>
                    )}
                    {item.pinned && <Pin className="size-3 text-accent" aria-label="Pinned" />}
                  </div>
                </button>
                {expanded?.id === item.id && (
                  <pre className="fade-in mx-1 mb-1 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-field p-2.5 font-mono text-[11.5px] leading-relaxed select-text">
                    {expanded.text ?? "Loading…"}
                  </pre>
                )}
                <div className="absolute right-1.5 top-1.5 flex gap-0.5">
                  {(item.truncated || item.lineCount > 2) && (
                    <IconButton
                      size="sm"
                      label={expanded?.id === item.id ? "Collapse" : "Show full text"}
                      icon={ChevronDown}
                      tabIndex={-1}
                      aria-expanded={expanded?.id === item.id}
                      className={cx(
                        "opacity-0 transition-transform group-hover:opacity-100 focus-visible:opacity-100",
                        expanded?.id === item.id && "rotate-180",
                      )}
                      onClick={() => toggleExpand(item)}
                    />
                  )}
                  <IconButton
                    size="sm"
                    label={item.pinned ? "Unpin" : "Pin"}
                    icon={item.pinned ? PinOff : Pin}
                    tabIndex={-1}
                    className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                    onClick={() => action.run(() => api.clipboard.pin(item.id, !item.pinned))}
                  />
                  <IconButton
                    size="sm"
                    label="Delete"
                    icon={Trash2}
                    tabIndex={-1}
                    className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                    onClick={() => action.run(() => api.clipboard.remove(item.id))}
                  />
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      <footer className="flex items-center justify-between border-t border-line px-3 py-2 text-[11px] text-subtle">
        <span>
          {status.data ? `${pluralize(status.data.stats.count, "item")} · ${formatBytes(status.data.stats.totalBytes)}` : ""}
        </span>
        {confirmClear ? (
          <span className="flex items-center gap-1.5">
            Clear unpinned history?
            <Button
              small
              variant="danger"
              onClick={() => action.run(() => api.clipboard.clear(false)).finally(() => setConfirmClear(false))}
            >
              Clear
            </Button>
            <Button small variant="ghost" onClick={() => setConfirmClear(false)}>
              Cancel
            </Button>
          </span>
        ) : (
          <Button small variant="ghost" icon={Trash2} disabled={!status.data?.stats.count} onClick={() => setConfirmClear(true)}>
            Clear history
          </Button>
        )}
      </footer>
    </div>
  );
}

function CaptureBanner({
  status,
  capture,
  onPause,
  onResume,
  onTurnOn,
  onSettings,
}: {
  status: ClipboardStatus | undefined;
  capture: ClipboardStatus["capture"];
  onPause: (minutes: number | null) => void;
  onResume: () => void;
  onTurnOn: () => void;
  onSettings: () => void;
}) {
  // One quiet line: state on the left, a single action on the right.
  const [dot, text, actionLabel, onAction] =
    capture === "off"
      ? ["bg-subtle", "History off", "Turn on", onTurnOn]
      : capture === "paused"
        ? ["bg-warn", status?.pausedUntil ? `Paused until ${formatTime(status.pausedUntil)}` : "Paused", "Resume", onResume]
        : ["bg-ok", "Saving copies", "Pause", () => onPause(null)];
  return (
    <div className="flex items-center gap-2 px-1 text-[11.5px]">
      <span className={cx("size-1.5 rounded-full", dot)} aria-hidden />
      <button type="button" onClick={onSettings} className="text-muted hover:text-fg" title="Clipboard settings">
        {text}
      </button>
      <button type="button" onClick={onAction} className="ml-auto font-semibold text-accent hover:brightness-110">
        {actionLabel}
      </button>
    </div>
  );
}
