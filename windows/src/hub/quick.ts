// The Hub's Quick tab: switches that show the value they really have (nothing assumes a
// default), the volume, the Bluetooth devices with their batteries, and a warning when a program
// other than Coucou's own visualizer is recording the microphone.

import { Bridge } from "../core/bridge";
import { h, clear } from "../views/dom";
import type { HubHost, HubTool } from "./types";

export interface QuickState {
  dnd: boolean | null;
  micMuted: boolean | null;
  micInUse: boolean;
  sinkMuted: boolean | null;
  volume: number | null;
  nightLight: boolean | null;
  bluetooth: { name: string; battery: number | null }[];
  layout: string | null;
}

function build(): HubHost {
  const switches = h("div", { class: "qk-grid" });
  const volume = h("input", { type: "range", min: "0", max: "100", step: "1", class: "qk-vol" }) as HTMLInputElement;
  const volLabel = h("span", { class: "qk-vol-l", text: "" });
  const info = h("div", { class: "qk-info" });
  const note = h("div", { class: "hub-err" });
  const el = h("div", { class: "qk" }, switches, h("div", { class: "qk-row" }, h("span", { class: "dim", text: "Volume" }), volume, volLabel), info, note);

  let timer: number | null = null;
  let state: QuickState | null = null;
  let dragging = false;
  let busy = false;

  async function flip(what: string, value: number) {
    note.textContent = "";
    try {
      await Bridge.quickSet(what, value);
    } catch (e) {
      note.textContent = String(e).replace(/^Error:\s*/, "");
    }
    await refresh();
  }

  function toggle(label: string, what: string, on: boolean | null, hint: string) {
    const b = h("button", { class: `qk-btn${on ? " on" : ""}`, title: hint, disabled: on === null }, h("span", { class: "qk-l", text: label }), h("span", { class: "qk-s", text: on === null ? "n/a" : on ? "on" : "off" }));
    b.addEventListener("click", () => void flip(what, on ? 0 : 1));
    return b;
  }

  function paint(q: QuickState) {
    clear(switches);
    switches.append(
      toggle("Do not disturb", "dnd", q.dnd, "Notification banners off (GNOME's own switch)"),
      toggle("Mic muted", "mic", q.micMuted, "Mute the default microphone"),
      toggle("Sound muted", "sink", q.sinkMuted, "Mute the default speakers"),
      toggle("Night light", "night", q.nightLight, "GNOME's night light"),
    );
    const lock = h("button", { class: "qk-btn", title: "Lock the screen" }, h("span", { class: "qk-l", text: "Lock screen" }), h("span", { class: "qk-s", text: "now" }));
    lock.addEventListener("click", () => void flip("lock", 1));
    switches.append(lock);
    if (!dragging && q.volume != null) {
      volume.value = String(Math.min(100, q.volume));
      volLabel.textContent = `${q.volume}%`;
    }
    volume.disabled = q.volume == null;
    clear(info);
    if (q.micInUse) info.append(h("div", { class: "qk-warn", text: "🎙 A program is recording the microphone" }));
    for (const d of q.bluetooth) info.append(h("div", { class: "qk-bt", text: `🎧 ${d.name}${d.battery != null ? ` · ${d.battery}%` : ""}` }));
    if (q.layout) info.append(h("div", { class: "dim", text: `Keyboard: ${q.layout}` }));
  }

  async function refresh() {
    if (busy) return;
    busy = true;
    try {
      const q = await Bridge.quickState();
      if (!q) return;
      const same = JSON.stringify(q) === JSON.stringify(state);
      state = q;
      if (!same) paint(q);
    } finally {
      busy = false;
    }
  }

  volume.addEventListener("input", () => {
    dragging = true;
    volLabel.textContent = `${volume.value}%`;
  });
  volume.addEventListener("change", async () => {
    dragging = false;
    await flip("volume", Number(volume.value));
  });

  return {
    el,
    sync() {},
    start() {
      void refresh();
      timer = window.setInterval(() => void refresh(), 2500);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      timer = null;
    },
  };
}

export const quickTool: HubTool = { id: "quick", label: "Quick", build };
