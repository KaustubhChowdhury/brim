import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { UTILITY_ICON, UTILITY_LABEL } from "../components/icons";
import { cx } from "../components/ui";
import { WIDE_TABS, type Utility } from "../lib/types";

/** Icon-over-label tabs for the wide layout, with an underline that springs to the active tab. */
export function WideTabs({ active, onSelect }: { active: Utility; onSelect: (u: Utility) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pill, setPill] = useState<{ x: number; w: number } | null>(null);
  // The underline spans the active tab's label, not the whole button.
  const [animate, setAnimate] = useState(false);
  const index = WIDE_TABS.indexOf(active);

  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    const measure = () => {
      const el = root.querySelectorAll<HTMLElement>('[role="tab"]')[index];
      const label = el?.querySelector<HTMLElement>("[data-label]");
      setPill(el && label ? { x: el.offsetLeft + label.offsetLeft - 4, w: label.offsetWidth + 8 } : null);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(root);
    return () => ro.disconnect();
  }, [index]);

  useEffect(() => {
    if (pill && !animate) {
      const id = requestAnimationFrame(() => setAnimate(true));
      return () => cancelAnimationFrame(id);
    }
  }, [pill, animate]);

  const move = (delta: number) => {
    const next = WIDE_TABS[(Math.max(0, index) + delta + WIDE_TABS.length) % WIDE_TABS.length]!;
    onSelect(next);
    requestAnimationFrame(() => ref.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.focus());
  };

  return (
    <div
      ref={ref}
      role="tablist"
      aria-label="Utilities"
      className="wide-tabs relative flex h-[46px] items-stretch"
      onKeyDown={(e) => {
        if (e.key === "ArrowRight") {
          e.preventDefault();
          move(1);
        } else if (e.key === "ArrowLeft") {
          e.preventDefault();
          move(-1);
        }
      }}
    >
      <span
        aria-hidden
        className={cx("wide-tab-line pointer-events-none absolute bottom-0 left-0 h-[2px] rounded-full", animate && "slide-indicator")}
        style={{
          width: pill?.w ?? 0,
          transform: `translateX(${pill?.x ?? 0}px)`,
          opacity: pill ? 1 : 0,
        }}
      />
      {WIDE_TABS.map((u) => {
        const Icon = UTILITY_ICON[u];
        return (
          <button
            key={u}
            type="button"
            role="tab"
            aria-selected={u === active}
            tabIndex={u === active || (index < 0 && u === WIDE_TABS[0]) ? 0 : -1}
            onClick={() => onSelect(u)}
            className={cx(
              "press-soft press relative flex w-[104px] flex-col items-center justify-center gap-[3px] pb-1 text-[11.5px] font-medium",
              u === active ? "text-fg" : "text-subtle hover:text-muted",
            )}
          >
            <Icon className="size-[17px]" strokeWidth={u === active ? 2.2 : 1.8} aria-hidden />
            <span data-label>{UTILITY_LABEL[u]}</span>
          </button>
        );
      })}
    </div>
  );
}
