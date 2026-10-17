import { Clipboard, Music, ShieldCheck, SquareCheck, LayoutGrid } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useSettings } from "../lib/hooks";
import { Button, ErrorNote, Toggle, cx } from "../components/ui";

export function Welcome({ onDone }: { onDone: () => void }) {
  const { settings } = useSettings();
  const [capture, setCapture] = useState<boolean | null>(null);
  const [startup, setStartup] = useState(false);
  const action = useAction();

  return (
    <div className="h-full overflow-y-auto">
      <div className="welcome mx-auto max-w-xl px-8 py-10">
        <img src="/logo.svg" alt="" className="mb-4 size-12" />
        <h1 className="text-2xl font-semibold">Welcome to Brim</h1>
        <p className="mt-2 text-[13px] text-muted">
          A compact island that floats above your windows. Click it — or press{" "}
          <kbd className="rounded bg-field px-1.5 py-0.5 font-mono text-xs text-fg">{settings.shortcut}</kbd> — to reach music,
          clipboard history, apps and tasks without switching windows.
        </p>

        <div className="mt-5 flex items-start gap-3 rounded-xl border border-line bg-[var(--surface-solid)] p-4">
          <ShieldCheck className="mt-0.5 size-5 shrink-0 text-accent" aria-hidden />
          <p className="text-xs text-muted">
            <span className="font-medium text-fg">Local-first.</span> No account, no cloud and no AI. Everything you keep stays in
            a database on this PC, and you can delete it any time from Settings.
          </p>
        </div>

        <h2 className="mt-7 text-sm font-semibold">Clipboard history</h2>
        <p className="mt-1 text-xs text-muted">
          Keep a searchable history of text you copy (stored for 7 days, up to 200 items). Copies that look like passwords or
          come from password managers are skipped on a best-effort basis. You can pause or turn this off at any time.
        </p>
        <div role="radiogroup" aria-label="Clipboard history" className="mt-3 grid grid-cols-2 gap-2">
          {[
            { v: true, title: "Keep clipboard history", body: "Text only, on this PC" },
            { v: false, title: "Not now", body: "You can turn it on later" },
          ].map((o) => (
            <button
              key={String(o.v)}
              type="button"
              role="radio"
              aria-checked={capture === o.v}
              onClick={() => setCapture(o.v)}
              className={cx(
                "press press-soft rounded-xl border p-3 text-left",
                capture === o.v ? "border-accent bg-accent/10" : "border-line hover:bg-hover",
              )}
            >
              <div className="text-[13px] font-medium">{o.title}</div>
              <div className="text-xs text-muted">{o.body}</div>
            </button>
          ))}
        </div>

        <h2 className="mt-7 text-sm font-semibold">Your four slots</h2>
        <p className="mt-1 text-xs text-muted">The dock starts with these. Change them any time in Settings › Dock slots.</p>
        <div className="mt-3 flex gap-2">
          {[
            { icon: Music, label: "Music" },
            { icon: Clipboard, label: "Clipboard" },
            { icon: LayoutGrid, label: "Apps" },
            { icon: SquareCheck, label: "Tasks" },
          ].map(({ icon: Icon, label }) => (
            <div key={label} className="flex flex-1 flex-col items-center gap-1 rounded-xl bg-field py-3 text-xs">
              <Icon className="size-5" aria-hidden />
              {label}
            </div>
          ))}
        </div>

        <div className="mt-6">
          <Toggle label="Launch Brim when I sign in" checked={startup} onChange={setStartup} />
        </div>

        <div className="mt-6 space-y-3">
          <ErrorNote error={action.error} />
          <Button
            variant="primary"
            className="w-full"
            disabled={capture === null}
            busy={action.busy}
            onClick={() =>
              action
                .run(() => api.settings.finishOnboarding({ clipboardCapture: capture === true, launchAtStartup: startup }))
                .then((r) => r && onDone())
            }
          >
            {capture === null ? "Choose a clipboard option to continue" : "Start using Brim"}
          </Button>
        </div>
      </div>
    </div>
  );
}
