// Live activities: what the server is busy with (downloads, a backup that is late). Several can run
// at once, so the pill shows them together — "3 downloads · 41 %" — and, when something else owns
// the pill (a recording, a reminder, the Pomodoro), a small chip at its end; the list, one row each,
// is in the Today tab. A click on either opens it.
//
// This is the one reader that looks while the island is minimised, so it is gentle: off until
// turned on, only with a server set, every 30 s while something runs, every 5 minutes when nothing
// does, and every 10 minutes while the server cannot be reached (an offline server is not an error
// worth a message). Nothing runs while Coucou is paused.

import { Bridge } from "../core/bridge";
import { State, pillActivities, type Activity } from "../core/state";
import type { Island } from "./island";

const BUSY_MS = 30_000;
const IDLE_MS = 5 * 60_000;
const AWAY_MS = 10 * 60_000;

export interface Summary {
  count: number;
  downloads: number;
  errors: number;
  /** Mean progress (0–1) of what reports one, or null. */
  progress: number | null;
  /** Longest time left among what runs. */
  eta: number | null;
  /** The pill's one line. */
  label: string;
}

export function etaWords(seconds: number): string {
  if (seconds < 90) return `${Math.max(1, Math.round(seconds))} s`;
  const m = Math.round(seconds / 60);
  if (m < 90) return `${m} min`;
  const h = Math.round(seconds / 3600);
  return h < 48 ? `${h} h` : `${Math.round(h / 24)} d`;
}

/** What a list of activities comes to, for the pill. Pure. */
export function summarize(list: Activity[]): Summary {
  const items = list.filter((a) => a.state !== "done");
  const errors = items.filter((a) => a.state === "error");
  const running = items.filter((a) => a.state !== "error");
  const known = running.filter((a) => a.state === "active" && a.progress != null);
  const progress = known.length > 0 ? known.reduce((s, a) => s + (a.progress ?? 0), 0) / known.length : null;
  const etas = running.map((a) => a.eta).filter((e): e is number => e != null);
  const eta = etas.length > 0 ? Math.max(...etas) : null;
  const downloads = running.filter((a) => a.kind === "download").length;
  const pct = progress != null ? ` · ${Math.round(progress * 100)} %` : "";

  let label: string;
  if (running.length === 0 && errors.length > 0) {
    const e = errors[0];
    label = errors.length === 1 ? `${e.title}${e.detail ? ` · ${e.detail}` : ""}` : `${errors.length} problems · ${e.title}`;
  } else if (running.length === 1) {
    label = `${running[0].title}${pct}`;
  } else if (downloads === running.length) {
    label = `${running.length} downloads${pct}`;
  } else {
    label = `${running.length} activities${pct}`;
  }
  if (errors.length > 0 && running.length > 0) label += ` · ${errors.length} ⚠`;
  return { count: items.length, downloads, errors: errors.length, progress, eta, label };
}

const enabled = () => State.settings.activitiesOn === true && !!State.settings.serverHost?.trim() && !State.paused;

let refreshNow: () => void = () => {};
/** Looks now (Today asks when it opens, so what it shows is fresh). */
export function refreshActivities() {
  refreshNow();
}

export function registerActivities(island: Island) {
  let timer: number | null = null;
  let failures = 0;
  let running = false;
  /** Errors already said once (an error is a flash when it appears, then only a row in Today). */
  const told = new Set<string>();
  let flashClear: number | null = null;

  /** Says, once, what newly went wrong — when the island is on screen already. */
  const tellErrors = (list: Activity[], firstLook: boolean) => {
    const errors = list.filter((a) => a.state === "error");
    const fresh = errors.filter((a) => !told.has(a.id));
    for (const id of [...told]) if (!errors.some((a) => a.id === id)) told.delete(id);
    for (const a of errors) told.add(a.id);
    if (firstLook || fresh.length === 0 || State.paused || State.mode === "hidden" || State.flash != null) return;
    const e = fresh[0];
    State.flash = { icon: "⚠", text: `${e.title}${e.detail ? ` · ${e.detail}` : ""}`, tone: "warn" };
    island.refreshStrip();
    if (flashClear != null) window.clearTimeout(flashClear);
    flashClear = window.setTimeout(() => {
      State.flash = null;
      flashClear = null;
      island.refreshStrip();
      State.notify();
    }, 5000);
  };
  let looked = false;

  const schedule = (ms: number) => {
    if (timer != null) window.clearTimeout(timer);
    timer = window.setTimeout(() => void look(), ms);
  };

  async function look() {
    timer = null;
    if (!enabled()) {
      running = false;
      if (State.activities.length > 0) {
        State.activities = [];
        island.refreshStrip();
      }
      return;
    }
    running = true;
    let next = IDLE_MS;
    try {
      const list = await Bridge.activities();
      failures = 0;
      State.activities = list;
      tellErrors(list, !looked);
      looked = true;
      next = pillActivities(list).some((a) => a.state === "active") ? BUSY_MS : IDLE_MS;
    } catch (e) {
      failures += 1;
      // Gone for a while: what was shown is stale, so it goes (after a couple of tries, not at the
      // first hiccup of a wifi change).
      if (failures >= 2 && State.activities.length > 0) State.activities = [];
      next = String(e).includes("not-configured") ? IDLE_MS : AWAY_MS;
    }
    island.refreshStrip();
    State.notify();
    schedule(next);
  }

  refreshNow = () => {
    if (enabled()) void look();
  };

  // Follows the setting (and the server, and pause): starts when it becomes enabled, stops when not.
  State.subscribe(() => {
    const on = enabled();
    if (on && !running && timer == null) {
      schedule(1500);
    } else if (!on && (running || timer != null)) {
      if (timer != null) window.clearTimeout(timer);
      timer = null;
      void look(); // clears what was shown
    }
  });
  if (enabled()) schedule(4000);
}
