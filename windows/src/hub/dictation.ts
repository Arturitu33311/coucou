// Dictation in the notch: while Handy records, the island opens with "Listening…" for as long as
// the key is held, and says "Transcribing…" for a moment when it is let go. The Rust side
// (dictation.rs) only reports that Handy started or stopped capturing; nothing is heard here.

import { onEvent } from "../core/bridge";
import { State } from "../core/state";
import type { Island } from "../island/island";

/** A note folds away after about six seconds: ask again before it does, while the key is held. */
const KEEP_EVERY_MS = 4000;

export function registerDictation(island: Island) {
  let keep: number | null = null;

  const show = (message: string) => {
    State.noteMessage = message;
    island.alert("note");
  };
  const stopKeeping = () => {
    if (keep != null) window.clearInterval(keep);
    keep = null;
  };

  void onEvent<{ recording: boolean }>("dictation", (e) => {
    if (State.paused) return;
    // A permission or a question waiting for an answer keeps the island: the indicator must not cover it.
    if (State.pendingApproval || State.pendingQuestion) return;
    stopKeeping();
    if (e.recording) {
      show("🎙  Listening…");
      keep = window.setInterval(() => show("🎙  Listening…"), KEEP_EVERY_MS);
    } else {
      show("✍️  Transcribing…");
    }
  });
}
