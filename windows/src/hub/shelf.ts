// The Hub's Shelf: the files dropped on the island (the inbox), with what to do with them —
// open one, hand it to Jinx or to Claude Code, or take it off. Nothing is read from them here.

import { Bridge } from "../core/bridge";
import { JINX_ID, State } from "../core/state";
import { h, clear } from "../views/dom";
import type { HubContext, HubHost, HubTool } from "./types";

interface Item {
  name: string;
  path: string;
  size: number;
  ageSecs: number;
}

const size = (b: number) => (b < 1024 ? `${b} B` : b < 1048576 ? `${(b / 1024).toFixed(0)} KB` : `${(b / 1048576).toFixed(1)} MB`);
const age = (s: number) => (s < 90 ? "just now" : s < 5400 ? `${Math.round(s / 60)} min ago` : s < 129600 ? `${Math.round(s / 3600)} h ago` : `${Math.round(s / 86400)} d ago`);

function build(ctx: HubContext): HubHost {
  const list = h("div", { class: "sh-list" });
  const count = h("div", { class: "hub-hint", text: "" });
  const note = h("div", { class: "hub-err" });
  const folder = h("button", { class: "hub-btn sm", text: "Open folder" });
  const add = h("input", {
    type: "text", class: "sh-add", placeholder: "Add a file: paste its path and press Enter", spellcheck: "false",
  }) as HTMLInputElement;
  const el = h("div", { class: "sh" }, h("div", { class: "sh-head" }, count, folder), list, h("div", { class: "sh-foot" }, add), note);

  let items: Item[] = [];
  let key = "";
  let timer: number | null = null;

  /** Hand a shelf file to the chat: it rides with the first message, to whoever is asked. */
  function toChat(item: Item, who: "jinx" | "claude") {
    State.droppedFile = { name: item.name, path: item.path };
    State.promptContext = { kind: "file", name: item.name, path: item.path };
    State.jinxFilePath = null;
    State.agentFilePath = null;
    if (who === "jinx") {
      ctx.actions.talkToJinx();
    } else {
      State.chatTarget = State.agents.length > 0 ? "agent" : "new";
      if (State.chatTarget === "agent") State.agentId = State.agentId ?? State.agents[0].id;
      ctx.actions.setView("prompt");
    }
  }

  function render() {
    const jinxOn = State.tasks.some((t) => t.id === JINX_ID);
    const next = JSON.stringify([items.map((i) => [i.path, Math.floor(i.ageSecs / 60)]), jinxOn]);
    if (next === key) return;
    key = next;
    count.textContent = items.length === 0 ? "Nothing on the shelf. Drop a file on the island (+) or add one by path." : `${items.length} file${items.length === 1 ? "" : "s"} · kept for a week`;
    clear(list);
    for (const it of items.slice(0, 20)) {
      const open = h("button", { class: "hub-btn sm", text: "Open" });
      open.addEventListener("click", () => void Bridge.shelfOpen(it.path).catch((e) => (note.textContent = String(e).replace(/^Error:\s*/, ""))));
      const claude = h("button", { class: "hub-btn sm", text: "Claude" });
      claude.addEventListener("click", () => toChat(it, "claude"));
      const del = h("button", { class: "hub-btn sm", text: "✕", title: "Remove from the shelf" });
      del.addEventListener("click", async () => {
        try {
          await Bridge.shelfRemove(it.path);
        } catch (e) {
          note.textContent = String(e).replace(/^Error:\s*/, "");
        }
        await refresh();
      });
      const row = h("div", { class: "sh-row" }, h("span", { class: "sh-name", text: it.name, title: it.path }), h("span", { class: "sh-meta", text: `${size(it.size)} · ${age(it.ageSecs)}` }), open);
      if (jinxOn) {
        const jinx = h("button", { class: "hub-btn sm", text: "Jinx" });
        jinx.addEventListener("click", () => toChat(it, "jinx"));
        row.append(jinx);
      }
      row.append(claude, del);
      list.append(row);
    }
  }

  async function refresh() {
    try {
      items = (await Bridge.shelfList()) ?? [];
    } catch {
      items = [];
    }
    key = "";
    render();
  }

  folder.addEventListener("click", () => void Bridge.shelfOpen(null));
  add.addEventListener("keydown", async (e) => {
    e.stopPropagation(); // Escape closes the island, not the field
    if (e.key !== "Enter") return;
    const path = add.value.trim().replace(/^'(.*)'$/, "$1").replace(/^file:\/\//, "");
    if (!path) return;
    note.textContent = "";
    try {
      await Bridge.ingestFile(path);
      add.value = "";
    } catch (err) {
      note.textContent = String(err).replace(/^Error:\s*/, "");
    }
    await refresh();
  });

  return {
    el,
    sync: render,
    focus: () => add.focus(),
    start() {
      void refresh();
      timer = window.setInterval(() => void refresh(), 4000);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      timer = null;
    },
  };
}

export const shelfTool: HubTool = { id: "shelf", label: "Shelf", typing: true, build };
