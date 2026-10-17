import { createContext, useContext } from "react";
import type { Utility } from "../lib/types";

export interface DockControls {
  utility: Utility;
  setUtility: (u: Utility) => void;
  /** Animates the panel closed. `restoreFocus` hands focus back to the previous app. */
  collapse: (restoreFocus: boolean) => void;
  openSettings: (section?: string) => void;
}

export const DockContext = createContext<DockControls | null>(null);

export function useDock(): DockControls {
  const ctx = useContext(DockContext);
  if (!ctx) throw new Error("useDock must be used inside the dock");
  return ctx;
}
