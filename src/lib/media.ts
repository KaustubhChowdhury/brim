import { useEffect, useState } from "react";
import { api } from "./ipc";
import { useTauriEvent } from "./hooks";
import type { MediaState, VolumeState } from "./types";

/** Live media state pushed by the backend (`media:state`). */
export function useMediaState(): MediaState | null {
  const [state, setState] = useState<MediaState | null>(null);
  useEffect(() => {
    api.media.state().then(setState).catch(() => setState(null));
  }, []);
  useTauriEvent<MediaState>("media:state", setState);
  return state;
}

/** Live system volume pushed by the backend (`volume:state`). */
export function useVolumeState(): VolumeState | null {
  const [state, setState] = useState<VolumeState | null>(null);
  useEffect(() => {
    api.volume.state().then(setState).catch(() => setState(null));
  }, []);
  useTauriEvent<VolumeState>("volume:state", setState);
  return state;
}
