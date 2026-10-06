// The Hub's Tasks: a short to-do list with reminders. Type "llamar a mamá a las 17:30" or
// "stretch in 20 min" and the time is taken out of the sentence (tasks.rs does the reading).
//
// With Jinx shared, the list and hers are one: what you create here is handed to her pendientes
// (so her briefings and nudges know it), what you tell her appears here under "From Jinx", and
// finishing a shared task on either side finishes it on the other (pendientes.rs does the talking).
//
// Like the Pomodoro, the reminders need no panel: one timeout fires at the next due reminder
// whether the island is open or not, and is re-armed whenever the list changes. Nothing polls
// while the island is shut, so a hidden island still costs nothing. A reminder is announced once
// (the flag is kept in the file), so a restart does not repeat it; an overdue one is announced
// when Coucou starts. The sync with Jinx runs once at start, then only while the panel is open.

import { Bridge } from "../core/bridge";
import { Sound } from "../core/sound";
import { State } from "../core/state";
import { h, clear } from "../views/dom";
import type { Island } from "../island/island";
import { sharing } from "./share";
import type { HubHost, HubTool } from "./types";

export interface Task {
  id: string;
  title: string;
  done: boolean;
  remindAt: number | null;
  notified: boolean;
  created: number;
  completedAt: number | null;
  /** Shared with Jinx. */
  jinx: boolean;
  /** Her id for it, once she has it. */
  jinxId: string | null;
}

/** One of Jinx's own pendientes. */
export interface JinxRow {
  id: string;
  titulo: string;
  due: string | null;
  categoria: string | null;
  source: string | null;
  status: string;
}

interface SyncResult {
  tasks: Task[];
  jinx: JinxRow[];
  error: string | null;
}

/** setTimeout overflows (and fires at once) beyond ~24.8 days: wait a day at most, then look again. */
const MAX_WAIT_MS = 24 * 3600 * 1000;
/** While the panel is open, how often Jinx's list is looked at again. */
const SYNC_EVERY_MS = 60_000;

let list: Task[] = [];
let fromJinx: JinxRow[] = [];
let syncError: string | null = null;
let timer: number | null = null;
let announce: (message: string) => void = () => {};
let loaded = false;
let syncing = false;
let syncAgain = false;

const nowSec = () => Math.floor(Date.now() / 1000);

function set(next: Task[]) {
  list = next;
  arm();
  State.notify();
}

function arm() {
  if (timer != null) window.clearTimeout(timer);
  timer = null;
  const next = list.filter((t) => !t.done && !t.notified && t.remindAt != null).sort((a, b) => a.remindAt! - b.remindAt!)[0];
  if (!next) return;
  const wait = Math.min(MAX_WAIT_MS, Math.max(0, next.remindAt! * 1000 - Date.now()) + 20);
  timer = window.setTimeout(fire, wait);
}

function fire() {
  timer = null;
  const now = nowSec();
  const due = list.filter((t) => !t.done && !t.notified && t.remindAt != null && t.remindAt <= now);
  if (due.length > 0) {
    announce(due.length === 1 ? due[0].title : `${due.length} reminders: ${due[0].title}…`);
    for (const t of due) {
      t.notified = true; // not announced twice even if the file cannot be written
      void Bridge.tasksNotified(t.id).catch(() => {});
    }
    State.notify();
  }
  arm();
}

async function reload() {
  try {
    set(await Bridge.tasksList());
    loaded = true;
  } catch {
    /* no list yet */
  }
}

/** A round with Jinx's pendientes: hand over, follow what was closed, list what she holds. */
async function sync() {
  if (!sharing()) {
    fromJinx = [];
    syncError = null;
    return;
  }
  if (syncing) {
    syncAgain = true; // something changed while a round was on: one more after it
    return;
  }
  syncing = true;
  try {
    const r = (await Bridge.tasksSync()) as SyncResult;
    set(r.tasks);
    fromJinx = r.jinx;
    syncError = r.error && r.error !== "busy" ? r.error : null;
  } catch (e) {
    syncError = String(e).replace(/^Error:\s*/, "");
  } finally {
    syncing = false;
    State.notify();
  }
  if (syncAgain) {
    syncAgain = false;
    void sync();
  }
}

/** The island says so when a reminder is due: a sound and a note, even with the island shut. */
export function registerTasks(island: Island) {
  announce = (message) => {
    Sound.play("finish");
    State.noteMessage = `⏰ ${message}`;
    island.alert("note");
  };
  // Overdue reminders are announced once, now; then one look at Jinx's list.
  void reload().then(() => {
    fire();
    void sync();
  });
}

/** How many open tasks are due or overdue (for whoever wants a badge). */
export function dueCount(): number {
  const now = nowSec();
  return list.filter((t) => !t.done && t.remindAt != null && t.remindAt <= now).length;
}

const pad = (n: number) => String(n).padStart(2, "0");

/** "17:30", "tomorrow 09:00", "Mon 09:00", or how late it is. */
function when(unix: number): { text: string; late: boolean } {
  const now = Date.now();
  const d = new Date(unix * 1000);
  const diff = unix * 1000 - now;
  if (diff < 0) {
    const m = Math.round(-diff / 60_000);
    const text = m < 1 ? "now" : m < 60 ? `${m} min late` : m < 48 * 60 ? `${Math.round(m / 60)} h late` : `${Math.round(m / 1440)} d late`;
    return { text, late: true };
  }
  const clock = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  const days = Math.floor((d.getTime() - midnight.getTime()) / 86_400_000);
  const day = days === 0 ? "" : days === 1 ? "tomorrow " : `${d.toLocaleDateString([], { weekday: "short" })} `;
  return { text: `${day}${clock}`, late: false };
}

/** Her due dates are plain days ("2026-10-06"). */
function dueText(due: string): { text: string; late: boolean } {
  const d = new Date(`${due}T00:00:00`);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const days = Math.round((d.getTime() - today.getTime()) / 86_400_000);
  if (days < 0) return { text: `${-days} d late`, late: true };
  if (days === 0) return { text: "today", late: false };
  if (days === 1) return { text: "tomorrow", late: false };
  return { text: d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" }), late: false };
}

function tomorrowAtNine(): number {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  d.setHours(9, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

function build(): HubHost {
  const input = h("input", {
    type: "text", class: "sh-add", spellcheck: "false", maxlength: "200",
    placeholder: "Add a task… (“llamar a mamá a las 17:30”, “stretch in 20 min”, “mañana 9:00”)",
  }) as HTMLInputElement;
  const rows = h("div", { class: "sh-list" });
  const foot = h("div", { class: "tk-foot" });
  const err = h("div", { class: "hub-err", text: "" });
  // Why Jinx could not be reached: its own line, so it goes away when she can be again.
  const syncNote = h("div", { class: "hub-err", text: "" });
  const el = h("div", { class: "sh" }, input, err, syncNote, rows, foot);

  let settle: number | null = null;
  /** A local change is done at once; Jinx hears of it a moment later (one round for a burst of them). */
  function soon() {
    if (settle != null) window.clearTimeout(settle);
    settle = window.setTimeout(() => {
      settle = null;
      void sync().then(render);
    }, 800);
  }

  async function run(p: Promise<Task[]>) {
    try {
      err.textContent = "";
      set(await p);
      soon();
    } catch (e) {
      err.textContent = String(e).replace(/^Error:\s*/, "");
    }
    render();
  }

  async function jinxDo(id: string, action: "hecho" | "descartado") {
    try {
      err.textContent = "";
      await Bridge.jinxResolve(id, action);
      fromJinx = fromJinx.filter((r) => r.id !== id);
      void sync().then(render);
    } catch (e) {
      err.textContent = String(e).replace(/^Error:\s*/, "");
    }
    render();
  }

  input.addEventListener("keydown", (e) => {
    e.stopPropagation(); // typing must never reach the island's own keys
    if (e.key === "Enter" && input.value.trim()) {
      const text = input.value;
      input.value = "";
      void run(Bridge.tasksAdd(text));
    }
  });

  function row(t: Task): HTMLElement {
    const box = h("button", { class: `tk-box${t.done ? " on" : ""}`, title: t.done ? "Reopen" : "Done", text: t.done ? "✓" : "" });
    box.addEventListener("click", () => void run(Bridge.tasksDone(t.id, !t.done)));
    const title = h("div", { class: `sh-name${t.done ? " tk-done" : ""}`, text: t.title, title: t.title });
    const kids: (Node | null)[] = [box, title];
    if (t.remindAt != null && !t.done) {
      const w = when(t.remindAt);
      kids.push(h("div", { class: `sh-meta${w.late ? " tk-late" : ""}`, text: `⏰ ${w.text}` }));
      const snooze = (label: string, until: () => number, tip: string) => {
        const b = h("button", { class: "hub-btn sm", text: label, title: tip });
        b.addEventListener("click", () => void run(Bridge.tasksSnooze(t.id, until())));
        return b;
      };
      kids.push(
        snooze("+10m", () => nowSec() + 600, "Remind me in 10 minutes"),
        snooze("+1h", () => nowSec() + 3600, "Remind me in an hour"),
        snooze("☀", tomorrowAtNine, "Remind me tomorrow at 09:00"),
      );
    }
    if (sharing() && !t.done) {
      // On: hers too (a dot while she has not got it yet). Off: only here.
      const chip = h("button", {
        class: `hub-btn sm tk-jinx${t.jinx ? " on" : ""}`,
        text: t.jinx ? (t.jinxId ? "Jinx ✓" : "Jinx …") : "Jinx",
        title: t.jinx ? "Also on Jinx's list — click to keep it only here" : "Only here — click to put it on Jinx's list too",
      });
      chip.addEventListener("click", () => void run(Bridge.tasksSetJinx(t.id, !t.jinx)));
      kids.push(chip);
    }
    const del = h("button", { class: "hub-btn sm", text: "×", title: "Delete" });
    del.addEventListener("click", () => void run(Bridge.tasksDelete(t.id)));
    kids.push(del);
    return h("div", { class: "sh-row" }, ...kids);
  }

  /** One of her own pendientes: told to her, not made here. */
  function jinxRow(r: JinxRow): HTMLElement {
    const kids: Node[] = [h("div", { class: "sh-name", text: r.titulo, title: r.titulo })];
    if (r.due) {
      const d = dueText(r.due);
      kids.push(h("div", { class: `sh-meta${d.late ? " tk-late" : ""}`, text: d.text }));
    }
    if (r.source === "teams") {
      // The school's assignments are mirrored by her own job: closing them here would be undone.
      kids.push(h("div", { class: "sh-meta", text: "Teams" }));
    } else {
      const ok = h("button", { class: "hub-btn sm", text: "✓", title: "Done" });
      ok.addEventListener("click", () => void jinxDo(r.id, "hecho"));
      const no = h("button", { class: "hub-btn sm", text: "×", title: "Dismiss it" });
      no.addEventListener("click", () => void jinxDo(r.id, "descartado"));
      kids.push(ok, no);
    }
    return h("div", { class: "sh-row tk-fromjinx" }, ...kids);
  }

  function render() {
    clear(rows);
    // Open ones first (reminders by time, then the rest), the finished ones at the bottom.
    const open = list.filter((t) => !t.done).sort((a, b) => (a.remindAt ?? Infinity) - (b.remindAt ?? Infinity) || a.created - b.created);
    const done = list.filter((t) => t.done).sort((a, b) => (b.completedAt ?? 0) - (a.completedAt ?? 0));
    for (const t of [...open, ...done]) rows.append(row(t));
    if (fromJinx.length > 0) {
      rows.append(h("div", { class: "cal-day", text: "From Jinx" }));
      for (const r of fromJinx) rows.append(jinxRow(r));
    }
    if (list.length === 0 && fromJinx.length === 0) {
      rows.append(h("div", { class: "hub-hint", text: "Nothing to do. Add a task above; put a time in it and Mochi will remind you." }));
    }
    syncNote.textContent = syncError ? `Jinx: ${syncError}` : "";
    clear(foot);
    foot.append(h("span", { class: "hub-hint", text: `${open.length} open${sharing() ? " · shared with Jinx" : ""}` }));
    if (done.length > 0) {
      const clearBtn = h("button", { class: "hub-btn sm", text: `Clear ${done.length} done` });
      clearBtn.addEventListener("click", () => void run(Bridge.tasksClearDone()));
      foot.append(clearBtn);
    }
  }

  let tick: number | null = null;
  let poll: number | null = null;
  return {
    el,
    sync: render,
    focus: () => input.focus(),
    start() {
      if (!loaded) void reload().then(render);
      render();
      void sync().then(render);
      // "5 min late" ages, and Jinx may have heard something new; nothing runs when the panel is not shown.
      tick = window.setInterval(render, 30_000);
      poll = window.setInterval(() => void sync().then(render), SYNC_EVERY_MS);
    },
    stop() {
      if (tick != null) window.clearInterval(tick);
      if (poll != null) window.clearInterval(poll);
      if (settle != null) {
        window.clearTimeout(settle);
        settle = null;
        void sync(); // a change made a moment ago still reaches her
      }
      tick = poll = null;
    },
  };
}

export const tasksTool: HubTool = { id: "tasks", label: "Tasks", typing: true, build };
