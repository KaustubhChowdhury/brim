import type { AppEntry, SlotView } from "../lib/types";
import { slotIcon } from "../components/icons";
import { cx } from "../components/ui";

export function SlotButton({
  slot,
  app,
  active,
  playing,
  onActivate,
}: {
  slot: SlotView;
  app: AppEntry | undefined;
  active: boolean;
  playing: boolean;
  onActivate: (slot: SlotView) => void;
}) {
  const Icon = slotIcon(slot.kind, slot.icon);
  const broken = slot.status !== "ok";
  const label = broken ? `${slot.label} — needs attention` : slot.kind === "app" ? `Open ${slot.label}` : slot.label;
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={slot.kind === "app" ? undefined : active}
      title={broken ? (slot.statusMessage ?? label) : slot.label}
      onClick={() => onActivate(slot)}
      className={cx(
        // The accent background is a separate indicator that springs between slots.
        "press relative flex size-10 shrink-0 items-center justify-center rounded-[min(calc(var(--radius)-6px),14px)]",
        active ? "text-accent-contrast" : "text-fg/85 hover:bg-hover hover:text-fg",
      )}
    >
      {slot.kind === "app" && app?.icon ? (
        <img src={app.icon} alt="" className={cx("size-6", broken && "opacity-40 grayscale")} draggable={false} />
      ) : (
        <Icon className={cx("size-[19px]", broken && "opacity-40")} aria-hidden />
      )}
      {playing && !active && (
        <span className="absolute bottom-1 right-1 flex h-2 items-end gap-px" aria-hidden>
          <span className="eq-bar h-2 w-[2px] rounded-full bg-accent" />
          <span className="eq-bar h-2 w-[2px] rounded-full bg-accent [animation-delay:-300ms]" />
          <span className="eq-bar h-2 w-[2px] rounded-full bg-accent [animation-delay:-600ms]" />
        </span>
      )}
      {broken && <span className="absolute right-1 top-1 size-2 rounded-full bg-warn" aria-hidden />}
    </button>
  );
}
