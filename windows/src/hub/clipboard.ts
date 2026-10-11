// The Hub's Clipboard: what was copied, to put back with a click. Off until turned on (it sees what
// passes through the clipboard). The history is kept in memory only; pinning an item keeps it on
// this computer for later days. Never shared with Jinx and never sent anywhere.
//
// How it behaves:
//   * a click on an item puts it back on the clipboard (you then paste where you were);
//   * 📌 keeps an item (it stays at the top and survives a restart), × forgets one;
//   * the search box filters; "Clear" forgets everything that is not pinned;
//   * what a password manager marks as secret is never in the list (clipboard.rs).

import { Bridge, onEvent } from "../core/bridge";
import { State } from "../core/state";
import { h, clear } from "../views/dom";
import type { HubHost, HubTool } from "./types";

export interface Entry {
  id: number;
  text: string;
  at: number;
  pinned: boolean;
}

const nowSec = () => Math.floor(Date.now() / 1000);

/** "now", "5 min", "3 h", "2 d". */
export function ago(at: number, now = nowSec()): string {
  const s = Math.max(0, now - at);
  if (s < 45) return "now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  const hrs = Math.round(m / 60);
  return hrs < 48 ? `${hrs} h` : `${Math.round(hrs / 24)} d`;
}

/** One line of what was copied, for the list. */
export function preview(text: string, max = 110): string {
  const line = text.replace(/\s*\n\s*/g, " ⏎ ").replace(/\s+/g, " ").trim();
  return line.length > max ? `${line.slice(0, max - 1)}…` : line;
}

/** Pinned first (newest first within), then the rest as they come; filtered by the search. */
export function view(list: Entry[], query: string): Entry[] {
  const q = query.trim().toLowerCase();
  const kept = q ? list.filter((e) => e.text.toLowerCase().includes(q)) : list;
  return [...kept.filter((e) => e.pinned), ...kept.filter((e) => !e.pinned)];
}

function build(): HubHost {
  const search = h("input", { type: "text", class: "sh-add", spellcheck: "false", maxlength: "80", placeholder: "Search what you copied…" }) as HTMLInputElement;
  const clearBtn = h("button", { class: "hub-btn sm", text: "Clear", title: "Forget everything that is not pinned" });
  const head = h("div", { class: "cb-head" }, search, clearBtn);
  const rows = h("div", { class: "sh-list" });
  const foot = h("div", { class: "cb-foot hub-hint" });
  const err = h("div", { class: "hub-err" });
  const el = h("div", { class: "sh cb" }, head, err, rows, foot);

  let list: Entry[] = [];
  let said = "";
  let sayTimer: number | null = null;
  let tick: number | null = null;

  const on = () => State.settings.clipboardHistory === true;

  function say(text: string) {
    said = text;
    if (sayTimer != null) window.clearTimeout(sayTimer);
    sayTimer = window.setTimeout(() => {
      said = "";
      sayTimer = null;
      render();
    }, 1600);
    render();
  }

  async function run<T>(p: Promise<T>, then: (v: T) => void) {
    try {
      err.textContent = "";
      then(await p);
    } catch (e) {
      err.textContent = String(e).replace(/^Error:\s*/, "");
    }
    render();
  }

  function row(e: Entry): HTMLElement {
    const pin = h("button", { class: `hub-btn sm cb-pin${e.pinned ? " on" : ""}`, title: e.pinned ? "Unpin" : "Keep this (it stays after a restart)", text: "📌" });
    pin.addEventListener("click", (ev) => {
      ev.stopPropagation();
      void run(Bridge.clipboardPin(e.id, !e.pinned), (l) => (list = l));
    });
    const del = h("button", { class: "hub-btn sm", title: "Forget this", text: "×" });
    del.addEventListener("click", (ev) => {
      ev.stopPropagation();
      void run(Bridge.clipboardDelete(e.id), (l) => (list = l));
    });
    const r = h(
      "div",
      { class: `sh-row cb-row${e.pinned ? " pinned" : ""}`, title: e.text.slice(0, 400) },
      h("div", { class: "sh-name", text: preview(e.text) }),
      h("div", { class: "sh-meta", text: e.pinned ? "pinned" : ago(e.at) }),
      pin,
      del,
    );
    // The whole row is the action: put it back, say so.
    r.addEventListener("click", () => {
      void run(Bridge.clipboardCopy(e.id), () => say("Copied — paste it where you were"));
    });
    return r;
  }

  function render() {
    clear(rows);
    const enabled = on();
    search.disabled = !enabled;
    clearBtn.toggleAttribute("disabled", !enabled || list.every((e) => e.pinned));
    if (!enabled) {
      const turnOn = h("button", { class: "hub-btn", text: "Turn on" });
      turnOn.addEventListener("click", () => {
        State.settings.clipboardHistory = true;
        void Bridge.saveSettings(State.settings);
        State.notify();
        render();
      });
      rows.append(
        h("div", { class: "hub-hint", text: "Clipboard history is off. When on, Coucou keeps the text you copy (in memory only, never sent anywhere) so you can put it back with a click. Items from password managers are never kept." }),
        turnOn,
      );
      foot.textContent = "";
      return;
    }
    const shown = view(list, search.value);
    for (const e of shown) rows.append(row(e));
    if (shown.length === 0) {
      rows.append(h("div", { class: "hub-hint", text: list.length === 0 ? "Nothing yet. Copy some text and it appears here." : "Nothing matches." }));
    }
    const unpinned = list.filter((e) => !e.pinned).length;
    foot.textContent = said || `${unpinned} kept in memory · ${list.length - unpinned} pinned`;
  }

  search.addEventListener("keydown", (e) => e.stopPropagation()); // typing must never reach the island's keys
  search.addEventListener("input", render);
  clearBtn.addEventListener("click", () => void run(Bridge.clipboardClear(), (l) => (list = l)));

  void onEvent<Entry[]>("clipboard", (l) => {
    list = l ?? [];
    render();
  });

  return {
    el,
    sync: render,
    focus: () => search.focus(),
    start() {
      void run(Bridge.clipboardList(), (l) => (list = l));
      // "5 min" ages; nothing runs when the panel is not shown.
      tick = window.setInterval(render, 30_000);
    },
    stop() {
      if (tick != null) window.clearInterval(tick);
      tick = null;
    },
  };
}

export const clipboardTool: HubTool = { id: "clipboard", label: "Clipboard", typing: true, build };
