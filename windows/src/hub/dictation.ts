// Dictation in the notch: while Handy records, the minimised island's pill says "Listening…" with a
// clock and moving bars, and "Transcribing" for a moment when the key is let go. With the island
// open (where the pill is not on screen) a red hairline along its lower edge says the same, so
// the Tasks or chat field being dictated into is never covered. The Rust side (dictation.rs) only
// reports that Handy started or stopped capturing; nothing is heard here.

import { onEvent } from "../core/bridge";
import { State } from "../core/state";
import type { Island } from "../island/island";

/** Handy gives no sign when the text has been typed: the pill lingers this long, then goes. */
const TRANSCRIBING_MS = 2500;

export function registerDictation(island: Island) {
  let done: number | null = null;

  const set = (next: "off" | "listening" | "transcribing") => {
    State.dictation = next;
    if (next === "listening") State.dictationSince = Date.now();
    island.refreshStrip();
    State.notify();
  };

  void onEvent<{ recording: boolean }>("dictation", (e) => {
    if (State.paused) return;
    if (done != null) window.clearTimeout(done);
    done = null;
    if (e.recording) {
      set("listening");
    } else {
      set("transcribing");
      done = window.setTimeout(() => set("off"), TRANSCRIBING_MS);
    }
  });
}
