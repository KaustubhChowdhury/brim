import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { UTILITY_LABEL } from "../components/icons";
import { cx } from "../components/ui";
import { WIDE_TABS, type Utility } from "../lib/types";

/** Text tabs for the wide layout, with a highlight that springs to the active tab. */
export function WideTabs({ active, onSelect }: { active: Utility; onSelect: (u: Utility) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pill, setPill] = useState<{ x: number; w: number } | null>(null);
  const [animate, setAnimate] = useState(false);
  const index = WIDE_TABS.indexOf(active);

  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    const measure = () => {
      const el = root.querySelectorAll<HTMLElement>('[role="tab"]')[index];
      setPill(el ? { x: el.offsetLeft, w: el.offsetWidth } : null);
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
      className="relative flex items-center gap-0.5"
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
        className={cx("wide-tab-pill pointer-events-none absolute inset-y-0 left-0 rounded-full", animate && "slide-indicator")}
        style={{
          width: pill?.w ?? 0,
          transform: `translateX(${pill?.x ?? 0}px)`,
          opacity: pill ? 1 : 0,
        }}
      />
      {WIDE_TABS.map((u) => (
        <button
          key={u}
          type="button"
          role="tab"
          aria-selected={u === active}
          tabIndex={u === active || (index < 0 && u === WIDE_TABS[0]) ? 0 : -1}
          onClick={() => onSelect(u)}
          className={cx(
            "press relative h-7 rounded-full px-3 text-[11.5px] font-semibold tracking-[0.01em]",
            u === active ? "text-accent" : "text-muted hover:text-fg",
          )}
        >
          {UTILITY_LABEL[u]}
        </button>
      ))}
    </div>
  );
}
