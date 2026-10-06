// The Hub's Pomodoro: a focus/break timer with statistics.
//
// The timer runs in the page and needs no panel: one timeout fires at the end of a period,
// whether the island is open or not, and the state is kept so a restart carries on. The log
// of what was run, and the statistics, are Rust's (pomodoro.rs).

import { Bridge } from "../core/bridge";
import { Sound } from "../core/sound";
import { State } from "../core/state";
import { h } from "../views/dom";
import type { Island } from "../island/island";
import { sharing, shareReadmeOnce } from "./share";
import type { HubHost, HubTool } from "./types";

const PRESETS = [
  { label: "25 / 5", focus: 25, brk: 5, long: 15 },
  { label: "50 / 10", focus: 50, brk: 10, long: 20 },
  { label: "15 / 3", focus: 15, brk: 3, long: 10 },
];
/// A longer break after this many focus periods.
const CYCLE = 4;
const KEY = "coucou.pomodoro";

type Phase = "idle" | "focus" | "break";

interface PState {
  phase: Phase;
  /** The break after the last focus of a cycle. */
  long: boolean;
  preset: number;
  /** Focus periods finished in this cycle. */
  cycle: number;
  /** When the period ends; null while idle or paused. */
  endsAt: number | null;
  /** What was left when paused. */
  remainingMs: number | null;
}

export interface PomoStats {
  todaySessions: number;
  todayMinutes: number;
  weekMinutes: number[];
  streak: number;
  peakHour: number | null;
  totalSessions: number;
}

let st: PState = load();
let timer: number | null = null;
let stats: PomoStats | null = null;
let announce: (message: string) => void = () => {};

function load(): PState {
  const fresh: PState = { phase: "idle", long: false, preset: 0, cycle: 0, endsAt: null, remainingMs: null };
  try {
    const saved = JSON.parse(window.localStorage.getItem(KEY) ?? "null") as PState | null;
    if (!saved || typeof saved.phase !== "string") return fresh;
    // A period that ended while Coucou was closed is over, not owed.
    if (saved.endsAt != null && saved.endsAt < Date.now()) return { ...fresh, preset: saved.preset ?? 0 };
    return { ...fresh, ...saved, preset: Math.min(saved.preset ?? 0, PRESETS.length - 1) };
  } catch {
    return fresh;
  }
}

function save() {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(st));
  } catch {
    /* the timer still runs without being remembered */
  }
}

const preset = () => PRESETS[st.preset];
const minutes = (p: Phase, long: boolean) => (p === "focus" ? preset().focus : long ? preset().long : preset().brk);

export function leftMs(): number {
  if (st.endsAt != null) return Math.max(0, st.endsAt - Date.now());
  if (st.remainingMs != null) return st.remainingMs;
  return minutes(st.phase === "idle" ? "focus" : st.phase, st.long) * 60_000;
}

const running = () => st.endsAt != null;

function arm() {
  if (timer != null) window.clearTimeout(timer);
  timer = null;
  if (st.endsAt != null) timer = window.setTimeout(finish, Math.max(0, st.endsAt - Date.now()) + 20);
}

async function record(kind: "focus" | "break", seconds: number, completed: boolean) {
  if (seconds < 10) return;
  try {
    await Bridge.pomodoroLog(kind, seconds, completed);
    await refreshStats();
  } catch {
    /* a log that cannot be written must not stop the timer */
  }
}

/** The period ran to its end. */
function finish() {
  timer = null;
  const was = st.phase;
  const full = minutes(was === "idle" ? "focus" : was, st.long) * 60;
  if (was === "focus") {
    void record("focus", full, true);
    st.cycle += 1;
    st.long = st.cycle % CYCLE === 0;
    st.phase = "break";
    st.endsAt = Date.now() + minutes("break", st.long) * 60_000;
    st.remainingMs = null;
    announce(st.long ? `Focus done — take a long break (${preset().long} min)` : `Focus done — ${preset().brk} min break`);
  } else if (was === "break") {
    void record("break", full, true);
    st.phase = "idle";
    st.endsAt = null;
    st.remainingMs = null;
    st.long = false;
    announce("Break over — ready for another focus?");
  }
  save();
  arm();
  State.notify();
}

export function startPause() {
  if (st.phase === "idle") {
    st.phase = "focus";
    st.endsAt = Date.now() + minutes("focus", false) * 60_000;
    st.remainingMs = null;
  } else if (running()) {
    st.remainingMs = leftMs();
    st.endsAt = null;
  } else {
    st.endsAt = Date.now() + (st.remainingMs ?? minutes(st.phase, st.long) * 60_000);
    st.remainingMs = null;
  }
  save();
  arm();
  State.notify();
}

/** End this period now (what was run is logged as not completed). */
export function skip() {
  if (st.phase === "idle") return;
  const total = minutes(st.phase, st.long) * 60;
  const done = total - Math.round(leftMs() / 1000);
  void record(st.phase === "focus" ? "focus" : "break", done, false);
  if (st.phase === "focus") {
    st.phase = "break";
    st.long = false;
    st.endsAt = Date.now() + minutes("break", false) * 60_000;
  } else {
    st.phase = "idle";
    st.endsAt = null;
    st.long = false;
  }
  st.remainingMs = null;
  save();
  arm();
  State.notify();
}

export function reset() {
  if (st.phase === "focus") {
    const done = minutes("focus", false) * 60 - Math.round(leftMs() / 1000);
    void record("focus", done, false);
  }
  st = { ...st, phase: "idle", long: false, cycle: 0, endsAt: null, remainingMs: null };
  save();
  arm();
  State.notify();
}

export function setPreset(i: number) {
  if (st.phase !== "idle") return; // a running period keeps its length
  st.preset = i;
  save();
  State.notify();
}

export async function refreshStats() {
  try {
    stats = await Bridge.pomodoroStats();
    if (sharing()) {
      shareReadmeOnce();
      void Bridge.sharePush("pomodoro.json", JSON.stringify({ updated: new Date().toISOString(), ...stats }, null, 2)).catch(() => {});
    }
  } catch {
    /* no statistics yet */
  }
  State.notify();
}

/** The island says so when a period ends: a sound and a note, even with the island shut. */
export function registerPomodoro(island: Island) {
  announce = (message) => {
    Sound.play("finish");
    State.noteMessage = `🍅 ${message}`;
    island.alert("note");
  };
  arm(); // a timer carried over from the last run
}

const clock = (ms: number) => {
  const s = Math.ceil(ms / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
};

function build(): HubHost {
  const time = h("div", { class: "pm-time", text: "25:00" });
  const phase = h("div", { class: "pm-phase", text: "" });
  const main = h("button", { class: "hub-btn primary", text: "Start" });
  const skipBtn = h("button", { class: "hub-btn", text: "Skip" });
  const resetBtn = h("button", { class: "hub-btn", text: "Reset" });
  const presets = h("div", { class: "pm-presets" });
  const today = h("div", { class: "pm-stat", text: "" });
  const streak = h("div", { class: "pm-stat dim", text: "" });
  const week = h("div", { class: "pm-week" });
  const peak = h("div", { class: "pm-stat dim", text: "" });

  main.addEventListener("click", startPause);
  skipBtn.addEventListener("click", skip);
  resetBtn.addEventListener("click", reset);
  PRESETS.forEach((p, i) => {
    const b = h("button", { class: "hub-btn", text: p.label });
    b.addEventListener("click", () => setPreset(i));
    presets.append(b);
  });
  const bars = Array.from({ length: 7 }, () => h("i"));
  week.append(...bars);

  const el = h(
    "div",
    { class: "pm" },
    h("div", { class: "pm-left" }, time, phase, h("div", { class: "pm-btns" }, main, skipBtn, resetBtn), presets),
    h("div", { class: "pm-right" }, today, streak, week, peak),
  );

  let tick: number | null = null;

  function render() {
    time.textContent = clock(leftMs());
    const label = st.phase === "idle" ? "Ready" : st.phase === "focus" ? "Focus" : st.long ? "Long break" : "Break";
    phase.textContent = `${label}${st.phase !== "idle" && !running() ? " · paused" : ""} · ${(st.cycle % CYCLE) + (st.phase === "focus" ? 1 : 0)}/${CYCLE}`;
    main.textContent = st.phase === "idle" ? "Start" : running() ? "Pause" : "Resume";
    skipBtn.style.display = st.phase === "idle" ? "none" : "";
    resetBtn.style.display = st.phase === "idle" && st.cycle === 0 ? "none" : "";
    [...presets.children].forEach((b, i) => {
      b.classList.toggle("on", i === st.preset);
      (b as HTMLButtonElement).disabled = st.phase !== "idle";
    });
    if (stats) {
      today.textContent = `Today: ${stats.todaySessions} session${stats.todaySessions === 1 ? "" : "s"} · ${stats.todayMinutes} min`;
      streak.textContent = stats.streak > 0 ? `🔥 ${stats.streak} day${stats.streak === 1 ? "" : "s"} in a row` : "No streak yet";
      const max = Math.max(30, ...stats.weekMinutes);
      bars.forEach((b, i) => {
        b.style.height = `${Math.max(2, Math.round((stats!.weekMinutes[i] / max) * 28))}px`;
        b.title = `${stats!.weekMinutes[i]} min`;
        b.classList.toggle("today", i === 6);
      });
      peak.textContent = stats.peakHour == null ? "" : `Best hour: ${String(stats.peakHour).padStart(2, "0")}:00`;
    }
  }

  return {
    el,
    sync: render,
    start() {
      void refreshStats();
      render();
      tick = window.setInterval(render, 500);
    },
    stop() {
      if (tick != null) window.clearInterval(tick);
      tick = null;
    },
  };
}

export const pomodoroTool: HubTool = { id: "pomodoro", label: "Pomodoro", build };

/** What Jinx is told about the timer. */
export function pomodoroSummary(): { phase: Phase; minutesLeft: number | null; running: boolean; cycle: number } {
  return {
    phase: st.phase,
    minutesLeft: st.phase === "idle" ? null : Math.ceil(leftMs() / 60_000),
    running: running(),
    cycle: st.cycle,
  };
}

export function pomodoroStatsNow(): PomoStats | null {
  return stats;
}
