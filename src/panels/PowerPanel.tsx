import { Lock, LogOut, Moon, Power, RotateCcw, type LucideIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import type { PowerAction } from "../lib/types";
import { ErrorNote, cx } from "../components/ui";

const ACTIONS: Array<{ id: PowerAction; label: string; icon: LucideIcon; confirm?: string }> = [
  { id: "lock", label: "Lock", icon: Lock },
  { id: "sleep", label: "Sleep", icon: Moon },
  { id: "restart", label: "Restart", icon: RotateCcw, confirm: "Restart now?" },
  { id: "shutdown", label: "Shut down", icon: Power, confirm: "Shut down now?" },
  { id: "signout", label: "Sign out", icon: LogOut, confirm: "Sign out now?" },
];

/** Lock, sleep, restart, shut down and sign out. Destructive actions need a second click. */
export function PowerPanel() {
  const [armed, setArmed] = useState<PowerAction | null>(null);
  const [error, setError] = useState<string | null>(null);

  // An armed confirmation expires so a stray click later can't restart the PC.
  useEffect(() => {
    if (!armed) return;
    const t = window.setTimeout(() => setArmed(null), 4000);
    return () => window.clearTimeout(t);
  }, [armed]);

  const run = (a: (typeof ACTIONS)[number]) => {
    setError(null);
    if (a.confirm && armed !== a.id) {
      setArmed(a.id);
      return;
    }
    setArmed(null);
    api.system.power(a.id).catch((e) => setError(e.message));
  };

  return (
    <div className="flex h-full flex-col items-center justify-center gap-5 px-6">
      <div className="flex flex-wrap justify-center gap-3">
        {ACTIONS.map((a, i) => {
          const isArmed = armed === a.id;
          return (
            <button
              key={a.id}
              type="button"
              onClick={() => run(a)}
              style={{ animationDelay: `${i * 40}ms` }}
              className={cx(
                "fade-in press flex w-24 flex-col items-center gap-2 rounded-2xl px-2 py-4 text-xs font-medium",
                isArmed ? "bg-danger/15 text-danger" : "bg-field hover:bg-hover",
              )}
            >
              <span
                className={cx(
                  "flex size-11 items-center justify-center rounded-full",
                  isArmed ? "bg-danger text-white" : "bg-accent/15 text-accent",
                )}
              >
                <a.icon className="size-5" aria-hidden />
              </span>
              <span key={String(isArmed)} className="pop">{isArmed ? "Click again" : a.label}</span>
            </button>
          );
        })}
      </div>
      <p className="h-4 text-xs text-muted" role="status">
        {armed ? ACTIONS.find((a) => a.id === armed)?.confirm : ""}
      </p>
      {error && <ErrorNote error={{ code: "power", message: error }} onDismiss={() => setError(null)} />}
    </div>
  );
}
