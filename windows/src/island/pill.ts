// The status pill: what the minimised island shows, in the place of the Music pill, when something
// asks for a glance — Handy recording, a reminder that went off, a bit of news (plugged in, back
// online) or the Pomodoro running. The same slot, the same shape and the same quiet look as the
// Music pill; the Music pill steps aside while this one has something to say.
//
// Everything that moves here is a CSS animation (they cost the page nothing), and the only
// clock is a one-second timer that runs while a timer is on show.

import { State, type PillKind } from "../core/state";
import { h } from "../views/dom";
import { dropReminder, reminderDone, reminderSnooze } from "../hub/tasks";
import { pomodoroPill, startPause } from "../hub/pomodoro";

export interface PillActions {
  /** The Pomodoro's own panel in the Hub. */
  openPomodoro(): void;
  blip(): void;
}

export interface PillHost {
  el: HTMLElement;
  /** Show what `State.pillKind()` says (cheap to call often). */
  sync(): void;
}

/** The colour Mochi takes while the pill is on show and he has nothing else to say. */
export function pillTint(kind: PillKind | null): string | null {
  switch (kind) {
    case "dictation":
      return "#FF5A5F";
    case "reminder":
      return "#F5A524";
    case "pomodoro":
      return State.pomodoroPhase === "break" ? "#4ADE80" : "#F4505E";
    case "flash":
      return State.flash?.tone === "warn" ? "#F5A524" : State.flash?.tone === "crit" ? "#F4505E" : State.flash?.tone === "ok" ? "#4ADE80" : null;
    default:
      return null;
  }
}

const clock = (ms: number) => {
  const s = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};
const RING_R = 9;
const RING_C = 2 * Math.PI * RING_R;

export function buildStatusPill(a: PillActions): PillHost {
  const el = h("div", { id: "status-pill" });
  let shownKey = "";
  let tick: number | null = null;
  /** Called every second while a clock is on show. */
  let update: () => void = () => {};

  // The island opens on mousedown when minimised; this pill has its own actions.
  el.addEventListener("mousedown", (e) => e.stopPropagation());

  function stopTick() {
    if (tick != null) window.clearInterval(tick);
    tick = null;
    update = () => {};
  }

  function button(label: string, title: string, on: () => void): HTMLElement {
    const b = h("button", { class: "sp-btn", text: label, title });
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      a.blip();
      on();
    });
    return b;
  }

  function buildDictation(listening: boolean) {
    if (listening) {
      const time = h("span", { class: "sp-time", text: "0:00" });
      el.append(
        h("span", { class: "sp-dot rec" }),
        h("span", { class: "sp-text", text: "Listening…" }),
        time,
        h("span", { class: "sp-bars" }, ...Array.from({ length: 5 }, (_, i) => h("i", { style: `animation-delay:${i * 0.13}s` }))),
      );
      update = () => {
        const s = Math.floor((Date.now() - State.dictationSince) / 1000);
        time.textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
      };
    } else {
      el.append(
        h("span", { class: "sp-text", text: "Transcribing" }),
        h("span", { class: "sp-dots" }, h("i"), h("i"), h("i")),
      );
    }
  }

  function buildReminder() {
    const r = State.reminders[0];
    const more = State.reminders.length - 1;
    const bell = h("span", { class: "sp-bell", text: "🔔" });
    const title = h("span", { class: "sp-text sp-grow", text: more > 0 ? `${r.title}  +${more}` : r.title, title: r.title });
    el.append(
      bell,
      title,
      button("✓", "Done", () => reminderDone(r.id)),
      button("+10m", "Remind me in 10 minutes", () => reminderSnooze(r.id, 600)),
      button("×", "Dismiss (it stays in the list)", () => dropReminder(r.id)),
    );
  }

  function buildFlash() {
    const f = State.flash;
    if (!f) return;
    el.append(h("span", { class: "sp-icon", text: f.icon }), h("span", { class: `sp-text sp-grow tone-${f.tone || "none"}`, text: f.text }));
  }

  function buildPomodoro() {
    const NS = "http://www.w3.org/2000/svg";
    const ring = document.createElementNS(NS, "svg");
    ring.setAttribute("class", "sp-ring");
    ring.setAttribute("viewBox", "0 0 24 24");
    ring.setAttribute("width", "24");
    ring.setAttribute("height", "24");
    const circle = (cls: string) => {
      const c = document.createElementNS(NS, "circle");
      c.setAttribute("class", cls);
      c.setAttribute("cx", "12");
      c.setAttribute("cy", "12");
      c.setAttribute("r", String(RING_R));
      return c;
    };
    const fg = circle("fg");
    fg.setAttribute("transform", "rotate(-90 12 12)");
    fg.setAttribute("stroke-dasharray", RING_C.toFixed(2));
    ring.append(circle("bg"), fg);
    const time = h("span", { class: "sp-time big" });
    const label = h("span", { class: "sp-label" });
    el.append(ring, time, label);
    el.classList.add("clickable");
    // A click starts or pauses the period; a double click or the right button opens the Pomodoro's panel.
    let clickTimer: number | null = null;
    el.onclick = (e) => {
      e.stopPropagation();
      if (clickTimer != null) return;
      clickTimer = window.setTimeout(() => {
        clickTimer = null;
        startPause();
      }, 260);
    };
    el.ondblclick = (e) => {
      e.stopPropagation();
      if (clickTimer != null) window.clearTimeout(clickTimer);
      clickTimer = null;
      a.openPomodoro();
    };
    el.oncontextmenu = (e) => {
      e.preventDefault();
      e.stopPropagation();
      a.openPomodoro();
    };
    update = () => {
      const p = pomodoroPill();
      if (p.phase === "idle") return;
      time.textContent = clock(p.leftMs);
      label.textContent = `${p.phase === "focus" ? "Focus" : p.long ? "Long break" : "Break"}${p.running ? "" : " · paused"}`;
      // The ring empties as the period runs out.
      fg.style.strokeDashoffset = String(RING_C * (1 - (p.totalMs > 0 ? p.leftMs / p.totalMs : 1)));
      el.classList.toggle("paused", !p.running);
      el.classList.toggle("break", p.phase === "break");
    };
  }

  function sync() {
    const kind = State.pillKind();
    const key =
      kind === "dictation" ? `d:${State.dictation}`
      : kind === "reminder" ? `r:${State.reminders.map((r) => r.id).join(",")}`
      : kind === "flash" ? `f:${State.flash?.text}`
      : kind === "pomodoro" ? `p:${State.pomodoroPhase}`
      : "";
    if (key !== shownKey) {
      shownKey = key;
      stopTick();
      el.replaceChildren();
      el.className = "";
      el.onclick = el.ondblclick = el.oncontextmenu = null;
      if (kind) el.classList.add(`k-${kind}`);
      switch (kind) {
        case "dictation":
          buildDictation(State.dictation === "listening");
          break;
        case "reminder":
          buildReminder();
          break;
        case "flash":
          buildFlash();
          break;
        case "pomodoro":
          buildPomodoro();
          break;
      }
      update();
      // One second is all a clock needs; the pill's other motion is CSS.
      if (kind === "dictation" || kind === "pomodoro") tick = window.setInterval(() => update(), 1000);
    } else if (kind === "pomodoro") {
      update(); // paused or resumed: the ring and the label follow
    }
  }

  return { el, sync };
}
