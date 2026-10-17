import { Keyboard } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useLoader, useSettings } from "../lib/hooks";
import { Button, ErrorNote, cx } from "../components/ui";

export const DEFAULT_SHORTCUT = "Alt+Shift+Space";

const MODIFIER_CODES = new Set(["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"]);

/** Builds an accelerator string ("Ctrl+Alt+Space") from a key event, or null if incomplete. */
export function acceleratorFromEvent(e: Pick<KeyboardEvent, "ctrlKey" | "altKey" | "shiftKey" | "metaKey" | "code">): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  // Require a non-Shift modifier so ordinary typing is never captured globally.
  if (!e.ctrlKey && !e.altKey && !e.metaKey) return null;
  let key = e.code;
  if (/^Key[A-Z]$/.test(key)) key = key.slice(3);
  else if (/^Digit[0-9]$/.test(key)) key = key.slice(5);
  if (!key || key === "Unidentified") return null;
  return [...mods, key].join("+");
}

export function ShortcutField() {
  const { settings, update } = useSettings();
  const status = useLoader(api.settings.shortcutStatus, ["settings:changed"]);
  const action = useAction();
  const [recording, setRecording] = useState(false);
  const [preview, setPreview] = useState<string | null>(null);

  return (
    <div className="flex flex-col items-end gap-1">
      <div className="flex items-center gap-2">
        <button
          type="button"
          aria-label={recording ? "Press the new shortcut" : `Shortcut: ${settings.shortcut}. Click to change.`}
          onClick={() => {
            setRecording(true);
            setPreview(null);
          }}
          onBlur={() => setRecording(false)}
          onKeyDown={(e) => {
            if (!recording) return;
            e.preventDefault();
            e.stopPropagation();
            if (e.key === "Escape") {
              setRecording(false);
              return;
            }
            const acc = acceleratorFromEvent(e.nativeEvent);
            if (acc) {
              setPreview(acc);
              setRecording(false);
              action
                .run(() => update({ shortcut: acc }))
                .finally(() => {
                  setPreview(null);
                  status.reload();
                });
            }
          }}
          className={cx(
            "inline-flex h-8 min-w-36 items-center justify-center gap-1.5 rounded-lg border px-3 font-mono text-xs",
            recording ? "border-accent bg-accent/10 text-accent" : "border-line bg-field",
          )}
        >
          <Keyboard className="size-3.5" aria-hidden />
          {recording ? "Press keys…" : (preview ?? settings.shortcut)}
        </button>
        {settings.shortcut !== DEFAULT_SHORTCUT && (
          <Button small variant="ghost" onClick={() => action.run(() => update({ shortcut: DEFAULT_SHORTCUT })).then(() => status.reload())}>
            Reset
          </Button>
        )}
      </div>
      {status.data && !status.data.registered && !action.error && (
        <p className="text-[11px] text-warn">This shortcut is not active — another app may be using it.</p>
      )}
      <ErrorNote error={action.error} onDismiss={() => action.setError(null)} />
    </div>
  );
}
