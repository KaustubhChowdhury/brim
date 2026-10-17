import { ArrowDown, ArrowUp, RotateCcw, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { api } from "../lib/ipc";
import { useAction, useLoader } from "../lib/hooks";
import { UTILITIES, type SlotAssignment, type SlotView } from "../lib/types";
import { SLOT_ICONS, UTILITY_LABEL, slotIcon } from "../components/icons";
import { Button, ErrorNote, IconButton, cx, inputClass } from "../components/ui";
import { Section } from "./Section";

export function SlotsSection() {
  const slots = useLoader(api.slots.list, ["slots:changed", "apps:changed"]);
  const apps = useLoader(api.apps.list, ["apps:changed"]);
  const action = useAction();
  const save = (slot: number, a: SlotAssignment) =>
    action.run(() => api.slots.set(slot, a)).then((r) => r && slots.setData(r));

  const list = slots.data ?? [];
  const appOptions = (apps.data ?? []).filter((a) => a.available).sort((a, b) => a.name.localeCompare(b.name));

  return (
    <>
      <ErrorNote error={action.error ?? slots.error} onDismiss={() => action.setError(null)} />
      <Section
        title="Dock slots"
        description="The dock always shows four quick-access slots. Each can open a utility or launch an app."
      >
        {list.map((slot, i) => (
          <SlotEditor
            key={slot.slotNumber}
            slot={slot}
            first={i === 0}
            last={i === list.length - 1}
            appOptions={appOptions}
            onSave={(a) => save(slot.slotNumber, a)}
            onMove={(to) => action.run(() => api.slots.move(slot.slotNumber, to)).then((r) => r && slots.setData(r))}
          />
        ))}
      </Section>
      <Button icon={RotateCcw} onClick={() => action.run(() => api.slots.reset()).then((r) => r && slots.setData(r))}>
        Restore default slots
      </Button>
    </>
  );
}

function SlotEditor({
  slot,
  first,
  last,
  appOptions,
  onSave,
  onMove,
}: {
  slot: SlotView;
  first: boolean;
  last: boolean;
  appOptions: Array<{ id: string; name: string }>;
  onSave: (a: SlotAssignment) => void;
  onMove: (to: number) => void;
}) {
  const [label, setLabel] = useState(slot.customLabel ?? "");
  const Icon = slotIcon(slot.kind, slot.icon);
  const isApp = slot.kind === "app";
  const base: SlotAssignment = { kind: slot.kind, target: slot.target, label: slot.customLabel, icon: slot.icon };

  return (
    <div className="py-3">
      <div className="flex items-center gap-3">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-field">
          <Icon className="size-[18px]" aria-hidden />
        </div>
        <div className="grid flex-1 grid-cols-2 gap-2">
          <select
            aria-label={`Slot ${slot.slotNumber + 1} type`}
            className={inputClass}
            value={isApp ? "app" : slot.kind}
            onChange={(e) => {
              const kind = e.target.value;
              if (kind === "app") {
                const firstApp = appOptions[0];
                if (firstApp) onSave({ kind: "app", target: firstApp.id, label: null, icon: null });
              } else {
                onSave({ kind, target: null, label: null, icon: null });
              }
              setLabel("");
            }}
          >
            {UTILITIES.map((u) => (
              <option key={u} value={u}>
                {UTILITY_LABEL[u]}
              </option>
            ))}
            <option value="app" disabled={appOptions.length === 0}>
              Launch an app…
            </option>
          </select>
          {isApp ? (
            <select
              aria-label={`Slot ${slot.slotNumber + 1} app`}
              className={inputClass}
              value={slot.target ?? ""}
              onChange={(e) => onSave({ ...base, target: e.target.value })}
            >
              {!appOptions.some((a) => a.id === slot.target) && <option value={slot.target ?? ""}>{slot.label} (missing)</option>}
              {appOptions.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
          ) : (
            <select
              aria-label={`Slot ${slot.slotNumber + 1} icon`}
              className={inputClass}
              value={slot.icon ?? ""}
              onChange={(e) => onSave({ ...base, icon: e.target.value || null })}
            >
              <option value="">Default icon</option>
              {Object.keys(SLOT_ICONS).map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          )}
          <input
            aria-label={`Slot ${slot.slotNumber + 1} label`}
            placeholder={`Label (default: ${isApp ? "app name" : slot.label})`}
            value={label}
            maxLength={24}
            onChange={(e) => setLabel(e.target.value)}
            onBlur={() => {
              if ((label.trim() || null) !== slot.customLabel) onSave({ ...base, label: label.trim() || null });
            }}
            className={cx(inputClass, "col-span-2")}
          />
        </div>
        <div className="flex flex-col">
          <IconButton size="sm" label="Move up" icon={ArrowUp} disabled={first} onClick={() => onMove(slot.slotNumber - 1)} />
          <IconButton size="sm" label="Move down" icon={ArrowDown} disabled={last} onClick={() => onMove(slot.slotNumber + 1)} />
        </div>
      </div>
      {slot.status !== "ok" && (
        <p className="mt-2 flex items-center gap-1.5 text-xs text-warn">
          <TriangleAlert className="size-3.5" aria-hidden />
          {slot.statusMessage}
        </p>
      )}
    </div>
  );
}
