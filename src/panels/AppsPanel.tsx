import { AppWindow, EyeOff, Eye, LayoutGrid, Pin, PinOff, Plus, RefreshCw, Search, Trash2, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, toAppError } from "../lib/ipc";
import { useAction, useLoader, useSettings, useTauriEvent } from "../lib/hooks";
import type { AppEntry } from "../lib/types";
import { EmptyState, ErrorNote, IconButton, Spinner, cx, inputClass } from "../components/ui";
import { useDock } from "../dock/context";

function AppIcon({ app, size = 32 }: { app: AppEntry; size?: number }) {
  return app.icon ? (
    <img src={app.icon} alt="" width={size} height={size} className={cx(!app.available && "opacity-40 grayscale")} draggable={false} />
  ) : (
    <AppWindow className="text-subtle" style={{ width: size, height: size }} aria-hidden />
  );
}

export function AppsPanel() {
  const { settings } = useSettings();
  const { collapse } = useDock();
  const apps = useLoader(api.apps.list, ["apps:changed"]);
  const action = useAction();
  const [query, setQuery] = useState("");
  const [scanning, setScanning] = useState(false);
  const [missing, setMissing] = useState<AppEntry | null>(null);

  useEffect(() => {
    api.apps.scanning().then(setScanning).catch(() => {});
  }, []);
  useTauriEvent("apps:changed", () => setScanning(false));

  const all = apps.data ?? [];
  const pinned = useMemo(
    () => all.filter((a) => a.pinned).sort((a, b) => (a.pinOrder ?? 0) - (b.pinOrder ?? 0)),
    [all],
  );
  const recent = useMemo(
    () =>
      all
        .filter((a) => a.lastUsedAt != null && !a.excluded && a.available)
        .sort((a, b) => (b.lastUsedAt ?? 0) - (a.lastUsedAt ?? 0))
        .slice(0, 6),
    [all],
  );
  const q = query.trim().toLowerCase();
  const results = useMemo(() => {
    if (!q) return [];
    return all
      .filter((a) => a.name.toLowerCase().includes(q))
      .sort((a, b) => {
        const ap = a.name.toLowerCase().startsWith(q) ? 0 : 1;
        const bp = b.name.toLowerCase().startsWith(q) ? 0 : 1;
        return ap - bp || Number(b.available) - Number(a.available) || a.name.localeCompare(b.name);
      })
      .slice(0, 50);
  }, [all, q]);

  const launch = async (app: AppEntry) => {
    action.setError(null);
    try {
      await api.apps.launch(app.id);
      setMissing(null);
      if (settings.collapseAfterLaunch) collapse(false);
    } catch (e) {
      const err = toAppError(e);
      action.setError(err);
      if (err.code === "missing_app" || err.code === "missing_path") setMissing(app);
    }
  };

  const row = (app: AppEntry) => (
    <li key={app.id} className="row-in group flex items-center gap-1 rounded-lg pr-1 transition-colors duration-150 hover:bg-hover">
      <button
        type="button"
        onClick={() => launch(app)}
        className="press press-soft flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2 py-1.5 text-left"
        title={app.available ? `Open ${app.name}` : `${app.name} is not available`}
      >
        <AppIcon app={app} size={24} />
        <span className={cx("truncate text-[13px]", !app.available && "text-subtle line-through")}>{app.name}</span>
      </button>
      <IconButton
        size="sm"
        label={app.pinned ? `Unpin ${app.name}` : `Pin ${app.name}`}
        icon={app.pinned ? PinOff : Pin}
        className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
        onClick={() => action.run(() => api.apps.pin(app.id, !app.pinned))}
      />
      <IconButton
        size="sm"
        label={app.excluded ? `Show ${app.name} in recents` : `Hide ${app.name} from recents`}
        icon={app.excluded ? Eye : EyeOff}
        className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
        onClick={() => action.run(() => api.apps.exclude(app.id, !app.excluded))}
      />
    </li>
  );

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-1.5 px-3 pt-3">
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
              } else if (e.key === "Enter" && results[0]) {
                e.preventDefault();
                launch(results[0]);
              }
            }}
            placeholder="Search apps"
            aria-label="Search apps"
            className={cx(inputClass, "pl-8")}
          />
        </div>
        <IconButton label="Add an app or shortcut" icon={Plus} onClick={() => action.run(() => api.apps.addCustom())} />
        <IconButton
          label="Refresh app list"
          icon={RefreshCw}
          className={scanning ? "animate-spin" : undefined}
          onClick={() => {
            setScanning(true);
            action.run(() => api.apps.refresh());
          }}
        />
      </div>

      <div className="space-y-2 px-3 pt-2">
        <ErrorNote error={action.error ?? apps.error} onDismiss={() => action.setError(null)} />
        {missing && (
          <div className="flex items-center gap-2 rounded-lg bg-warn/10 px-2.5 py-2 text-xs">
            <TriangleAlert className="size-3.5 shrink-0 text-warn" aria-hidden />
            <span className="flex-1">Remove “{missing.name}” from your pinned apps?</span>
            <button
              type="button"
              className="font-medium text-warn underline"
              onClick={() => action.run(() => api.apps.remove(missing.id)).then(() => setMissing(null))}
            >
              Remove
            </button>
          </div>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3 pt-2">
        {apps.loading && !apps.data ? (
          <Spinner label="Loading apps" />
        ) : q ? (
          results.length ? (
            <ul aria-label="Search results">{results.map(row)}</ul>
          ) : (
            <EmptyState icon={Search} title="No apps found">
              Nothing matches “{query}”. Use + to add a program or shortcut.
            </EmptyState>
          )
        ) : (
          <>
            <SectionTitle>Pinned</SectionTitle>
            {pinned.length ? (
              <ul className="grid grid-cols-4 gap-1 px-1" aria-label="Pinned apps">
                {pinned.map((app) => (
                  <li key={app.id} className="row-in group relative">
                    <button
                      type="button"
                      onClick={() => launch(app)}
                      title={app.available ? app.name : `${app.name} is not available`}
                      className="press flex w-full flex-col items-center gap-1 rounded-xl px-1 py-2 hover:bg-hover"
                    >
                      <AppIcon app={app} size={32} />
                      <span className={cx("w-full truncate text-center text-[11px]", !app.available && "text-subtle")}>
                        {app.name}
                      </span>
                    </button>
                    {app.source === "custom" && (
                      <IconButton
                        size="sm"
                        label={`Remove ${app.name}`}
                        icon={Trash2}
                        className="absolute right-0 top-0 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                        onClick={() => action.run(() => api.apps.remove(app.id))}
                      />
                    )}
                  </li>
                ))}
              </ul>
            ) : (
              <p className="px-2 pb-2 text-xs text-muted">
                No pinned apps yet. Search for an app and use the pin button, or add one with +.
              </p>
            )}
            {settings.trackRecentApps && recent.length > 0 && (
              <>
                <SectionTitle>Recent</SectionTitle>
                <ul aria-label="Recent apps">{recent.map(row)}</ul>
              </>
            )}
            <SectionTitle>All apps {scanning && <span className="font-normal normal-case">· scanning…</span>}</SectionTitle>
            {all.length ? (
              <ul aria-label="All apps">{all.filter((a) => a.available).map(row)}</ul>
            ) : scanning ? (
              <Spinner label="Finding apps" />
            ) : (
              <EmptyState icon={LayoutGrid} title="No apps found">
                Use refresh to scan the Start menu again.
              </EmptyState>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="px-2 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-wide text-subtle">{children}</h3>;
}
