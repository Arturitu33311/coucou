// The Hub's Today: the day on one panel — what is next in the calendar, which reminders are due
// or late, how much focus is done, and the weather if it is on. It reads what the other tools
// already keep (nothing of its own is stored) and, like them, looks only while it is on screen.

import { Bridge } from "../core/bridge";
import { State } from "../core/state";
import { h, clear } from "../views/dom";
import { etaWords, refreshActivities } from "../island/activities";
import { iconOf } from "../island/peripherals";
import type { CalEvent } from "./calendar";
import { pomodoroStatsNow, refreshStats } from "./pomodoro";
import { ensureTasksLoaded, openTasks } from "./tasks";
import { tile } from "./ui";
import type { HubHost, HubTool } from "./types";

interface Wx {
  city: string;
  temp: number;
  code: number;
  rainNext6h: number;
}

const pad = (n: number) => String(n).padStart(2, "0");
const clock = (unix: number) => {
  const d = new Date(unix * 1000);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
};

function hello(d: Date): string {
  const hr = d.getHours();
  const part = hr < 6 ? "Late night" : hr < 12 ? "Good morning" : hr < 20 ? "Good afternoon" : "Good evening";
  return `${part} · ${d.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" })}`;
}

/** "in 25 min", "in 2 h 10 min", "now". */
export function inWords(seconds: number): string {
  if (seconds <= 0) return "now";
  const m = Math.round(seconds / 60);
  if (m < 1) return "in under a minute";
  if (m < 60) return `in ${m} min`;
  const hrs = Math.floor(m / 60);
  const rest = m % 60;
  return rest === 0 ? `in ${hrs} h` : `in ${hrs} h ${rest} min`;
}

const symbol = (code: number) =>
  code === 0 ? "☀️" : code <= 2 ? "⛅" : code === 3 ? "☁️" : code <= 48 ? "🌫️" : code <= 67 ? "🌧️" : code <= 77 ? "❄️" : code <= 82 ? "🌦️" : code >= 95 ? "⛈️" : "🌡️";

interface Item {
  at: number;
  text: string;
  kind: "event" | "task";
  late: boolean;
  now: boolean;
}

function build(): HubHost {
  const title = h("div", { class: "td-hello" });
  const nextTile = tile("Next");
  const tasksTile = tile("To do");
  const focusTile = tile("Focus today");
  const grid = h("div", { class: "hub-grid g3" }, nextTile.el, tasksTile.el, focusTile.el);
  const list = h("div", { class: "td-list" });
  const wx = h("div", { class: "td-wx hub-hint" });
  // Batteries of paired devices: shown when there are any (a low one is coloured).
  const dev = h("div", { class: "td-dev" });
  // What the server is busy with (downloads, a backup that is late), one row each.
  const acts = h("div", { class: "td-acts" });
  const el = h("div", { class: "td" }, title, grid, acts, list, wx, dev);

  let events: CalEvent[] = [];
  let timer: number | null = null;
  let slow: number | null = null;
  let eventsOk = true;

  function render() {
    const now = new Date();
    const nowS = Math.floor(now.getTime() / 1000);
    title.textContent = hello(now);

    // Calendar: what has not ended yet and starts before tomorrow's dawn.
    const endOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime() / 1000;
    const upcoming = events.filter((e) => e.end > nowS && e.start < endOfDay + 6 * 3600).sort((a, b) => a.start - b.start);
    const running = upcoming.find((e) => !e.allDay && e.start <= nowS);
    const next = upcoming.find((e) => !e.allDay && e.start > nowS);
    if (running) nextTile.set(running.summary, null, `until ${clock(running.end)}`, "ok");
    else if (next) nextTile.set(next.summary, null, `${clock(next.start)} · ${inWords(next.start - nowS)}`, next.start - nowS < 900 ? "warn" : "");
    else nextTile.set(eventsOk ? "Nothing" : "–", null, eventsOk ? "no more events" : "calendar unavailable");

    // Tasks.
    const open = openTasks();
    const late = open.filter((t) => t.remindAt != null && t.remindAt <= nowS);
    tasksTile.set(String(open.length), null, late.length ? `${late.length} due or late` : "all on time", late.length ? "warn" : "");

    // Focus.
    const st = pomodoroStatsNow();
    if (st) focusTile.set(`${st.todayMinutes} min`, null, `${st.todaySessions} session${st.todaySessions === 1 ? "" : "s"}${st.streak > 1 ? ` · ${st.streak}-day streak` : ""}`);
    else focusTile.set("–", null, "");

    // One timeline: late and today's reminders, and the day's events, by time.
    const items: Item[] = [];
    for (const e of upcoming) {
      if (e.allDay || e.start >= endOfDay) continue;
      items.push({ at: Math.max(e.start, nowS - 1), text: e.summary, kind: "event", late: false, now: e.start <= nowS });
    }
    for (const t of open) {
      if (t.remindAt == null || t.remindAt >= endOfDay) continue;
      items.push({ at: t.remindAt, text: t.title, kind: "task", late: t.remindAt <= nowS, now: false });
    }
    items.sort((a, b) => a.at - b.at);
    clear(list);
    for (const it of items.slice(0, 7)) {
      const when = it.late ? "late" : it.now ? "now" : clock(it.at);
      list.append(
        h(
          "div",
          { class: `td-row${it.late ? " late" : ""}${it.now ? " now" : ""}` },
          h("span", { class: "td-ico", text: it.kind === "event" ? "▪" : "⏰" }),
          h("span", { class: "cal-when", text: when }),
          h("span", { class: "cal-what", text: it.text }),
        ),
      );
    }
    clear(acts);
    const live = State.activities.filter((x) => x.state !== "done" || x.kind === "backup");
    // Errors first (a backup that is late is what needs a look), then what runs; two rows fit above
    // the day, the rest is "+ n more" (the whole panel scrolls).
    const order = { error: 0, active: 1, queued: 2, done: 3 } as const;
    live.sort((p, q) => order[p.state] - order[q.state]);
    for (const x of live.slice(0, 2)) {
      const icon = x.kind === "download" ? "⬇" : x.kind === "backup" ? "💾" : "⚙";
      const right = x.state === "error" ? x.detail || "problem" : x.state === "queued" ? "waiting" : x.kind === "backup" ? x.detail : x.progress != null ? `${Math.round(x.progress * 100)} %${x.eta != null ? ` · ${etaWords(x.eta)}` : ""}` : "";
      const bar = h("i");
      bar.style.width = `${Math.round((x.progress ?? 0) * 100)}%`;
      acts.append(
        h(
          "div",
          { class: `td-act ${x.state}`, title: `${x.app}: ${x.title}` },
          h("span", { class: "td-ico", text: icon }),
          h("span", { class: "cal-what", text: x.title }),
          h("span", { class: "cal-when", text: right }),
          h("div", { class: "td-bar" }, bar),
        ),
      );
    }
    if (live.length > 2) acts.append(h("div", { class: "hub-hint", text: `+ ${live.length - 2} more` }));
    clear(dev);
    for (const d of State.peripherals) {
      const low = !d.charging && d.percent <= 20;
      dev.append(h("span", { class: `td-bat${low ? " low" : ""}`, title: d.name, text: `${iconOf(d.kind)} ${d.name} ${d.percent}%${d.charging ? " ⚡" : ""}` }));
    }
    if (items.length === 0) {
      const undated = open.filter((t) => t.remindAt == null).slice(0, 4);
      if (undated.length === 0) list.append(h("div", { class: "hub-hint", text: "A clear day. Nothing timed on it." }));
      for (const t of undated) list.append(h("div", { class: "td-row" }, h("span", { class: "td-ico", text: "·" }), h("span", { class: "cal-what", text: t.title })));
    }
  }

  async function loadEvents() {
    try {
      events = ((await Bridge.calendarEvents(2)) as CalEvent[]) ?? [];
      eventsOk = true;
    } catch {
      eventsOk = false;
    }
    render();
  }

  async function loadWeather() {
    const s = State.settings;
    const city = (s.weatherCity ?? "").trim();
    if (!s.weatherOn || !city) {
      wx.textContent = "";
      return;
    }
    try {
      const w = (await Bridge.weatherGet(city)) as Wx;
      wx.textContent = `${symbol(w.code)} ${w.city} ${Math.round(w.temp)}°${w.rainNext6h >= 30 ? ` · rain ${w.rainNext6h}% soon` : ""}`;
    } catch {
      wx.textContent = "";
    }
  }

  return {
    el,
    sync: render,
    start() {
      render();
      void ensureTasksLoaded().then(render);
      void refreshStats().then(render);
      void loadEvents();
      void loadWeather();
      refreshActivities(); // so what is shown here is fresh (a no-op when it is off)
      // Minutes age and a calendar changes: a light look each minute, the weather every quarter hour.
      timer = window.setInterval(() => {
        render();
        void loadEvents();
      }, 60_000);
      slow = window.setInterval(() => void loadWeather(), 15 * 60_000);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      if (slow != null) window.clearInterval(slow);
      timer = slow = null;
    },
  };
}

export const todayTool: HubTool = { id: "today", label: "Today", build };
