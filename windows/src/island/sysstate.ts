// The island's quiet details: the battery, the mains and the internet. The system says when
// they change (sysstate.rs); the page keeps the last values for the hairline and the small
// "offline" mark, and gives a few seconds of news in the pill when something that matters
// happens — but only when the island is already on screen: plugging in the charger never wakes it.

import { Bridge, onEvent } from "../core/bridge";
import { State, type SysInfo } from "../core/state";
import type { Island } from "./island";
import { newsFor } from "./news";

export function registerSysState(island: Island) {
  let prev: SysInfo | null = null;
  let clear: number | null = null;

  const apply = (now: SysInfo, quiet: boolean) => {
    const was = prev;
    prev = now;
    State.sys = now;
    if (!quiet && was && State.mode !== "hidden" && !State.paused) {
      const news = newsFor(was, now);
      if (news) {
        State.flash = news.flash;
        island.refreshStrip();
        if (clear != null) window.clearTimeout(clear);
        clear = window.setTimeout(() => {
          State.flash = null;
          clear = null;
          island.refreshStrip();
          State.notify();
        }, news.ms);
      }
    }
    State.notify();
  };

  void Bridge.sysState().then((s) => s && apply(s, true));
  void onEvent<SysInfo>("sysstate", (s) => apply(s, false));
}
