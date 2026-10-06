// What a change in the battery, the mains or the internet is worth saying in the pill (and what
// is not). Pure and without imports, so it can be tried on its own.

import type { Flash, SysInfo } from "../core/state";

export const NEWS_MS = 3000;
const NEWS_URGENT_MS = 6000;
/** Battery levels worth a word on the way down. */
const LOW_STEPS = [20, 10, 5];

/** What, if anything, a change from `was` to `now` is worth saying. Pure, so it can be tested. */
export function newsFor(was: SysInfo, now: SysInfo): { flash: Flash; ms: number } | null {
  if (was.online && !now.online) return { flash: { icon: "⌁", text: "Offline", tone: "warn" }, ms: NEWS_MS };
  if (!was.online && now.online) return { flash: { icon: "●", text: "Back online", tone: "ok" }, ms: NEWS_MS };
  const pct = now.battery == null ? "" : ` · ${now.battery}%`;
  if (!was.plugged && now.plugged) return { flash: { icon: "⚡", text: `${now.charging ? "Charging" : "Plugged in"}${pct}`, tone: "ok" }, ms: NEWS_MS };
  if (was.plugged && !now.plugged) return { flash: { icon: "🔋", text: `On battery${pct}`, tone: "" }, ms: NEWS_MS };
  if (!now.plugged && was.battery != null && now.battery != null) {
    const step = LOW_STEPS.find((t) => was.battery! > t && now.battery! <= t);
    if (step != null) {
      return { flash: { icon: "🪫", text: `Battery ${now.battery}%`, tone: step <= 10 ? "crit" : "warn" }, ms: step <= 10 ? NEWS_URGENT_MS : NEWS_MS };
    }
  }
  return null;
}

