// The Hub's Notes: one scratch pad, saved as you type. With Jinx shared, the same text is on
// the server (~/.hermes/state/coucou/notes.md) for her to read.

import { Bridge } from "../core/bridge";
import { State } from "../core/state";
import { h } from "../views/dom";
import type { HubHost, HubTool } from "./types";

const clockOf = (unix: number) => new Date(unix * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

function build(): HubHost {
  const pad = h("textarea", {
    class: "nt-pad", placeholder: "Notes… (saved as you type; Jinx can read them)", spellcheck: "false",
  }) as HTMLTextAreaElement;
  const status = h("div", { class: "nt-status", text: "" });
  const el = h("div", { class: "nt" }, pad, status);

  let loaded = false;
  let dirty = false;
  let saveTimer: number | null = null;
  let poll: number | null = null;
  let note = "";

  async function save() {
    if (saveTimer != null) window.clearTimeout(saveTimer);
    saveTimer = null;
    if (!dirty) return;
    dirty = false;
    try {
      await Bridge.notesSave(pad.value);
      note = "saved";
    } catch (e) {
      dirty = true;
      note = String(e).replace(/^Error:\s*/, "");
    }
    await paint();
  }

  async function paint() {
    const sharing = State.settings.jinxShare !== false && !!State.settings.serverHost?.trim();
    let jinx = "only here — set a server in Settings → Hub to share with Jinx";
    if (sharing) {
      const s = await Bridge.shareStatus().catch(() => null);
      jinx = s?.lastError ? `Jinx: not in step (${s.lastError})` : s?.lastOk ? `Jinx can read this · synced ${clockOf(s.lastOk)}` : "Jinx can read this";
    }
    status.textContent = `${pad.value.length} chars · ${note || "saved"} · ${jinx}`;
    status.classList.toggle("bad", jinx.includes("not in step") || (note !== "" && note !== "saved"));
  }

  pad.addEventListener("input", () => {
    dirty = true;
    note = "…";
    if (saveTimer != null) window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => void save(), 700);
    void paint();
  });
  // Escape closes the island, not the pad; typing must never reach the island's own keys.
  pad.addEventListener("keydown", (e) => e.stopPropagation());

  return {
    el,
    sync() {},
    focus: () => pad.focus(),
    async start() {
      if (!loaded) {
        loaded = true;
        pad.value = await Bridge.notesLoad().catch(() => "");
      }
      note = "saved";
      void paint();
      poll = window.setInterval(() => void paint(), 3000);
    },
    stop() {
      if (poll != null) window.clearInterval(poll);
      poll = null;
      void save(); // what was typed when the panel went away is kept
    },
  };
}

export const notesTool: HubTool = { id: "notes", label: "Notes", typing: true, build };
