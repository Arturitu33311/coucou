// The Hub's Calendar: the next days of the calendar GNOME keeps (the one Jinx reads), and
// Jinx's pending tasks. The same events are copied for her to ~/.hermes/state/coucou/.

import { Bridge } from "../core/bridge";
import { State } from "../core/state";
import { h, clear } from "../views/dom";
import { shareReadmeOnce } from "./share";
import type { HubHost, HubTool } from "./types";

export interface CalEvent {
  id: string;
  summary: string;
  start: number;
  end: number;
  allDay: boolean;
}

interface Task {
  title: string;
  due: string | null;
  category: string | null;
  priority: number;
}

const DAYS = 7;
const hhmm = (unix: number) => new Date(unix * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });

function dayLabel(unix: number, now: Date): string {
  const d = new Date(unix * 1000);
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((start(d) - start(now)) / 86_400_000);
  if (diff <= 0) return "Today";
  if (diff === 1) return "Tomorrow";
  return d.toLocaleDateString([], { weekday: "long", day: "numeric", month: "short" });
}

/** "2026-10-08" → "Oct 8"; anything else as it is. */
function dueLabel(due: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(due);
  if (!m) return due;
  return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])).toLocaleDateString([], { month: "short", day: "numeric" });
}

/** What Jinx is given: the same list, plain. */
export function eventsForJinx(events: CalEvent[]): string {
  return JSON.stringify(
    {
      updatedAt: new Date().toISOString(),
      note: "Alan's calendar for the next days, as his desktop shows it (Google Calendar via GNOME). Times are unix seconds.",
      events: events.map((e) => ({ title: e.summary, start: e.start, end: e.end, allDay: e.allDay, startsAt: new Date(e.start * 1000).toLocaleString() })),
    },
    null,
    1,
  );
}

function build(): HubHost {
  const events = h("div", { class: "cal-list" });
  const tasks = h("div", { class: "cal-tasks" });
  const note = h("div", { class: "hub-err" });
  const el = h("div", { class: "cal" }, h("div", { class: "cal-col" }, events), h("div", { class: "cal-col side" }, tasks), note);

  let timer: number | null = null;
  let taskTimer: number | null = null;
  let last = "";
  let pushed = "";

  function paintEvents(list: CalEvent[]) {
    const key = JSON.stringify(list.map((e) => [e.id, e.start, e.end])) + Math.floor(Date.now() / 60_000);
    if (key === last) return;
    last = key;
    clear(events);
    if (list.length === 0) {
      events.append(h("div", { class: "hub-hint", text: `Nothing in the next ${DAYS} days.` }));
      return;
    }
    const now = new Date();
    const nowS = now.getTime() / 1000;
    let day = "";
    for (const e of list) {
      const label = dayLabel(e.start < nowS - 3600 ? nowS : e.start, now);
      if (label !== day) {
        day = label;
        events.append(h("div", { class: "cal-day", text: label }));
      }
      const running = !e.allDay && e.start <= nowS && nowS < e.end;
      const when = e.allDay ? "all day" : `${hhmm(e.start)}–${hhmm(e.end)}`;
      events.append(h("div", { class: `cal-row${running ? " now" : ""}${e.end < nowS ? " past" : ""}` }, h("span", { class: "cal-when", text: when }), h("span", { class: "cal-what", text: e.summary })));
    }
  }

  async function loadEvents() {
    try {
      const list = ((await Bridge.calendarEvents(DAYS)) as CalEvent[]) ?? [];
      note.textContent = "";
      paintEvents(list);
      // Jinx gets the same list, whenever it changed.
      const body = eventsForJinx(list);
      const stable = JSON.stringify(list.map((e) => [e.id, e.start, e.end]));
      if (stable !== pushed && State.settings.jinxShare !== false) {
        pushed = stable;
        shareReadmeOnce();
        void Bridge.sharePush("calendar.json", body).catch(() => {});
      }
    } catch (e) {
      note.textContent = String(e).replace(/^Error:\s*/, "");
    }
  }

  async function loadTasks() {
    try {
      const list = ((await Bridge.jinxTasks()) as Task[]) ?? [];
      clear(tasks);
      tasks.append(h("div", { class: "cal-day", text: "Pending · Jinx" }));
      if (list.length === 0) tasks.append(h("div", { class: "hub-hint", text: "Nothing pending." }));
      for (const t of list.slice(0, 6)) {
        tasks.append(h("div", { class: "cal-task" }, h("span", { class: "cal-what", text: t.title }), t.due ? h("span", { class: "cal-when", text: dueLabel(t.due) }) : h("span")));
      }
    } catch (e) {
      clear(tasks);
      const msg = String(e).replace(/^Error:\s*/, "");
      tasks.append(h("div", { class: "hub-hint", text: msg === "not-configured" ? "Set the server in Settings → Hub to see Jinx's pending tasks." : msg }));
    }
  }

  return {
    el,
    sync() {},
    start() {
      void loadEvents();
      void loadTasks();
      timer = window.setInterval(() => void loadEvents(), 60_000);
      taskTimer = window.setInterval(() => void loadTasks(), 120_000);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      if (taskTimer != null) window.clearInterval(taskTimer);
      timer = taskTimer = null;
    },
  };
}

export const calendarTool: HubTool = { id: "calendar", label: "Calendar", build };
