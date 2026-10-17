import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { api, toAppError } from "./ipc";
import type { AppError, Settings } from "./types";
import { applyTheme } from "./theme";

/** Subscribes to a backend event for the lifetime of the component. */
export function useTauriEvent<T>(event: string, handler: (payload: T) => void) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    listen<T>(event, (e) => ref.current(e.payload)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [event]);
}

/** Loads data, reloading when any of `events` fires. */
export function useLoader<T>(load: () => Promise<T>, events: string[] = [], deps: unknown[] = []) {
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(true);
  const loadRef = useRef(load);
  loadRef.current = load;
  const seq = useRef(0);

  const reload = useCallback(() => {
    const mine = ++seq.current;
    loadRef
      .current()
      .then((d) => {
        if (mine === seq.current) {
          setData(d);
          setError(null);
        }
      })
      .catch((e) => {
        if (mine === seq.current) setError(toAppError(e));
      })
      .finally(() => {
        if (mine === seq.current) setLoading(false);
      });
  }, []);

  useEffect(reload, deps);
  useEffect(() => {
    const offs: Array<Promise<UnlistenFn>> = [...events, "data:reset"].map((ev) => listen(ev, () => reload()));
    return () => {
      offs.forEach((p) => p.then((f) => f()));
    };
  }, [events.join("|")]);

  return { data, error, loading, reload, setData };
}

/** Re-renders every `intervalMs` while mounted (for relative times and progress bars). */
export function useNow(intervalMs: number, active = true): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs, active]);
  return now;
}

/** Runs an async action, exposing busy state and the last error. */
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const run = useCallback(async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      setError(toAppError(e));
      return undefined;
    } finally {
      setBusy(false);
    }
  }, []);
  return { busy, error, setError, run };
}

interface SettingsCtx {
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<Settings>;
}

const SettingsContext = createContext<SettingsCtx | null>(null);

export function SettingsProvider({ children, fallback }: { children: ReactNode; fallback: ReactNode }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<AppError | null>(null);

  useEffect(() => {
    api.settings.get().then(setSettings).catch((e) => setError(toAppError(e)));
  }, []);
  useTauriEvent<Settings>("settings:changed", setSettings);

  useEffect(() => {
    if (!settings) return;
    applyTheme(settings);
    if (settings.theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyTheme(settings);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [settings]);

  const update = useCallback(async (patch: Partial<Settings>) => {
    const next = await api.settings.update(patch);
    setSettings(next);
    return next;
  }, []);

  if (error) {
    return (
      <div role="alert" className="p-4 text-sm text-[var(--danger)]">
        Settings could not be loaded: {error.message}
      </div>
    );
  }
  if (!settings) return <>{fallback}</>;
  return <SettingsContext.Provider value={{ settings, update }}>{children}</SettingsContext.Provider>;
}

export function useSettings(): SettingsCtx {
  const ctx = useContext(SettingsContext);
  if (!ctx) throw new Error("useSettings must be used inside SettingsProvider");
  return ctx;
}
