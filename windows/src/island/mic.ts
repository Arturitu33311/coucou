// The microphone indicator. The top bar, where GNOME puts its orange dot, is hidden on this
// desktop, so the island says it: while a program other than Handy (which has its own pill) records
// the microphone, the island stays up with a small dot, and for a few seconds the pill says which
// program started. A click on the dot opens the Hub's Quick tab, where the microphone is muted.
//
// Only program names arrive (dictation.rs reads the sound server's own listing); no audio is
// opened. Nothing is shown when the setting is off.

import { onEvent } from "../core/bridge";
import { State } from "../core/state";
import type { Island } from "./island";

const FLASH_MS = 4000;

/** "Zoom and Chromium input", "Zoom, Chromium input and 1 more". */
export function names(apps: string[]): string {
  if (apps.length <= 1) return apps[0] ?? "";
  if (apps.length === 2) return `${apps[0]} and ${apps[1]}`;
  return `${apps[0]}, ${apps[1]} and ${apps.length - 2} more`;
}

export function registerMic(island: Island) {
  let clear: number | null = null;

  void onEvent<{ apps: string[] }>("mic", (e) => {
    const before = State.micApps;
    State.micApps = e.apps ?? [];
    const started = State.micApps.filter((a) => !before.includes(a));
    if (State.micOn() && started.length > 0 && State.flash == null && State.dictation === "off") {
      State.flash = { icon: "🎙", text: `${names(started)} ${started.length === 1 ? "is" : "are"} listening`, tone: "warn" };
      if (clear != null) window.clearTimeout(clear);
      clear = window.setTimeout(() => {
        State.flash = null;
        clear = null;
        island.refreshStrip();
        State.notify();
      }, FLASH_MS);
    }
    island.refreshStrip();
  });
}
