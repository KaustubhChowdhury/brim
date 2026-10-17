// Motion helpers built on the spring curves in styles.css.

import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { AnimationLevel } from "./types";

/** How long to wait for the panel to shrink into the island before resizing the window. */
export function closeDelayMs(level: AnimationLevel): number {
  if (level === "none") return 0;
  if (level === "reduced" || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return 160;
  return 280; // --ease-close reaches 99% at 275 ms
}

/** Keeps an element mounted while its exit animation plays. */
export function usePresence(open: boolean, exitMs = 150): { mounted: boolean; state: "enter" | "exit" } {
  const [mounted, setMounted] = useState(open);
  const [state, setState] = useState<"enter" | "exit">("enter");
  useEffect(() => {
    if (open) {
      setMounted(true);
      setState("enter");
      return;
    }
    setState("exit");
    const t = window.setTimeout(() => setMounted(false), exitMs);
    return () => window.clearTimeout(t);
  }, [open, exitMs]);
  return { mounted, state };
}

type Layer = { id: string; node: ReactNode; leaving: boolean; initial: boolean };

/**
 * Crossfades between children identified by `id`. The outgoing child stays mounted (same
 * React instance, so no reload or focus jump) while it fades out; the incoming one slides in
 * from the side its `order` implies, like iOS tab transitions.
 */
export function PanelSwap({ id, order, children }: { id: string; order: number; children: ReactNode }) {
  const [layers, setLayers] = useState<Layer[]>([{ id, node: null, leaving: false, initial: true }]);
  const [dir, setDir] = useState(1);
  const prevChildren = useRef<ReactNode>(children);
  const prevOrder = useRef(order);
  const currentId = useRef(id);

  useLayoutEffect(() => {
    if (currentId.current === id) return;
    const outgoing = prevChildren.current;
    setDir(order >= prevOrder.current ? 1 : -1);
    setLayers((ls) => [
      ...ls
        .filter((l) => l.id !== id)
        .map((l) => (l.leaving ? l : { ...l, leaving: true, node: outgoing })),
      { id, node: null, leaving: false, initial: false },
    ]);
    currentId.current = id;
  }, [id, order]);

  // Runs after the effect above in the same commit: remember what was just rendered.
  useLayoutEffect(() => {
    prevChildren.current = children;
    prevOrder.current = order;
  });

  const hasLeaving = layers.some((l) => l.leaving);
  useEffect(() => {
    if (!hasLeaving) return;
    const t = window.setTimeout(() => setLayers((ls) => ls.filter((l) => !l.leaving)), 220);
    return () => window.clearTimeout(t);
  }, [hasLeaving, layers]);

  return (
    <div className="relative h-full">
      {layers.map((l) => (
        <div
          key={l.id}
          className={l.leaving ? "swap-leave" : l.initial ? "h-full" : "swap-enter h-full"}
          style={{ "--swap-dir": dir } as CSSProperties}
          aria-hidden={l.leaving || undefined}
          inert={l.leaving || undefined}
        >
          {l.leaving ? l.node : children}
        </div>
      ))}
    </div>
  );
}

/**
 * Style for a progress/level fill that animates on the GPU: a full-width bar slid left, so
 * changes never trigger layout (smooth at any refresh rate) and the rounded end stays round.
 * The track must clip its content (`overflow-hidden`).
 */
export function fillStyle(fraction: number): CSSProperties {
  const f = Math.min(1, Math.max(0, Number.isFinite(fraction) ? fraction : 0));
  return { transform: `translateX(${(f - 1) * 100}%)` };
}
