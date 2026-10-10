// Notification peek: another program's notification shown in the island for a few seconds.
// Off until turned on in Settings (it reads what the notification says); the Rust side only
// sends it here when it is on, and nothing is kept.

import { onEvent } from "../core/bridge";
import { reportLocalNews } from "../core/presence";
import { Sound } from "../core/sound";
import { State } from "../core/state";
import type { Island } from "../island/island";

interface Peek {
  app: string;
  summary: string;
  body: string;
}

export function registerNotificationPeek(island: Island) {
  void onEvent<Peek>("notification", (n) => {
    if (!State.settings.notificationPeek || State.paused) return;
    // Something waiting for an answer (a permission, a question) keeps the island: a message must not cover it.
    if (State.pendingApproval || State.pendingQuestion || State.isPinned) return;
    const head = [n.app, n.summary].filter(Boolean).join(" — ");
    State.noteMessage = n.body ? `${head}: ${n.body}` : head;
    // Mochi is startled, here and on the phone (whichever one he is drawn on).
    island.mochi.triggerEmote("surprised", 1.8);
    void reportLocalNews();
    Sound.play("pop");
    island.alert("note");
  });
}
