// What Jinx asks of the island that only the page can do (src-tauri/src/island_cmd.rs hands it over): Mochi's mood and
// opening a Hub tool. The rest of her orders (music, pills, apps) Rust runs itself. Rust has already checked the order
// against the closed list; the page checks it again, since it is the page that acts.

import { onEvent } from "../core/bridge";
import { validJinxEmote } from "../core/jinx-mood";
import { State } from "../core/state";
import type { Island } from "./island";

/** The Hub, as far as an order needs it (main.ts passes the real one; the page's tests a stand-in). */
export interface HubAccess {
  ids(): string[];
  select(id: string): void;
}

type Order =
  | { kind: "emote"; emote?: unknown; seconds?: unknown }
  | { kind: "hub"; tool?: unknown };

/** Mochi's face for a few seconds, 1–8. */
const clampSeconds = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? Math.min(8, Math.max(1, v)) : 3);

export function registerIslandCommands(island: Island, hub: HubAccess) {
  void onEvent<Order>("island-cmd", (order) => {
    if (State.paused) return;
    if (order.kind === "emote") {
      const emote = validJinxEmote(order.emote);
      if (!emote) return;
      // A hidden island shows nothing: bring it up first so the face is seen.
      if (State.mode === "hidden") island.reveal();
      island.emote(emote, clampSeconds(order.seconds));
      return;
    }
    if (order.kind === "hub") {
      const tool = typeof order.tool === "string" ? hub.ids().find((id) => id === order.tool) : undefined;
      if (!tool) return;
      hub.select(tool);
      island.expand("tool");
    }
  });
}
