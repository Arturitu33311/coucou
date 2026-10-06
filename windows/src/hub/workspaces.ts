// The Hub's Workspaces: a named set of apps, folders and links that open together with one
// click. The list is kept and checked by Rust (workspaces.rs), which also starts everything:
// this is only the list, and the small editor.

import { Bridge } from "../core/bridge";
import { h, clear } from "../views/dom";
import type { HubHost, HubTool } from "./types";

interface Item {
  kind: "app" | "folder" | "url";
  target: string;
  label: string;
}
export interface Workspace {
  id: string;
  name: string;
  items: Item[];
}
interface App {
  id: string;
  name: string;
}

const ICON = { app: "▣", folder: "▤", url: "↗" } as const;
const msg = (e: unknown) => String(e).replace(/^Error:\s*/, "");

function build(): HubHost {
  let spaces: Workspace[] = [];
  let loaded = false;
  let apps: App[] | null = null;
  /** The workspace being edited (a copy: nothing changes until Save). */
  let draft: Workspace | null = null;
  let result = "";
  let resultBad = false;
  let picking = false;

  const el = h("div", { class: "sh ws" });

  async function persist(next: Workspace[]): Promise<boolean> {
    try {
      spaces = (await Bridge.workspacesSave(next)) as Workspace[];
      return true;
    } catch (e) {
      result = msg(e);
      resultBad = true;
      return false;
    }
  }

  async function open(w: Workspace) {
    result = `Opening ${w.name}…`;
    resultBad = false;
    render();
    try {
      const failed = (await Bridge.workspacesLaunch(w.id)) as string[];
      resultBad = failed.length > 0;
      result = failed.length === 0 ? `✓ ${w.name}: ${w.items.length} opened` : `${w.name}: ${w.items.length - failed.length} opened · ${failed.join(" · ")}`;
    } catch (e) {
      result = msg(e);
      resultBad = true;
    }
    render();
  }

  // ── The list ──────────────────────────────────────────────────────────────────

  function renderList() {
    const add = h("button", { class: "hub-btn sm", text: "+ New workspace" });
    add.addEventListener("click", () => {
      draft = { id: crypto.randomUUID(), name: "", items: [] };
      result = "";
      render();
    });
    const rows = h("div", { class: "sh-list" });
    for (const w of spaces) {
      const openBtn = h("button", { class: "hub-btn primary sm", text: "Open", title: "Open all of it" });
      openBtn.addEventListener("click", () => void open(w));
      const edit = h("button", { class: "hub-btn sm", text: "Edit" });
      edit.addEventListener("click", () => {
        draft = JSON.parse(JSON.stringify(w)) as Workspace;
        result = "";
        render();
      });
      const what = w.items.length === 0 ? "empty" : w.items.map((i) => i.label).join(" · ");
      rows.append(h("div", { class: "sh-row" }, h("div", { class: "ws-name", text: w.name }), h("div", { class: "sh-name hub-hint", text: what, title: what }), openBtn, edit));
    }
    if (spaces.length === 0) {
      rows.append(h("div", { class: "hub-hint", text: "A workspace opens a set of apps, folders and links at once — “Study”, “Work”, “Game night”. Make one." }));
    }
    el.append(h("div", { class: "sh-head" }, h("span", { class: "hub-hint", text: `${spaces.length} workspace${spaces.length === 1 ? "" : "s"}` }), add), rows);
    if (result) el.append(h("div", { class: resultBad ? "hub-err" : "sh-up-status ok", text: result }));
  }

  // ── The editor ────────────────────────────────────────────────────────────────

  function renderEdit(d: Workspace) {
    const name = h("input", { type: "text", class: "sh-folder", spellcheck: "false", maxlength: "60", placeholder: "Name (Study, Work…)", value: d.name }) as HTMLInputElement;
    name.addEventListener("input", () => (d.name = name.value));
    name.addEventListener("keydown", (e) => e.stopPropagation());

    const items = h("div", { class: "sh-list ws-items" });
    d.items.forEach((it, i) => {
      const del = h("button", { class: "hub-btn sm", text: "×", title: "Take it out" });
      del.addEventListener("click", () => {
        d.items.splice(i, 1);
        render();
      });
      items.append(h("div", { class: "sh-row" }, h("span", { class: "ws-ico", text: ICON[it.kind] }), h("div", { class: "sh-name", text: it.label, title: it.target }), del));
    });
    if (d.items.length === 0) items.append(h("div", { class: "hub-hint", text: "Nothing in it yet. Add an app, a folder or a link." }));

    const addApp = h("button", { class: "hub-btn sm", text: "+ App" });
    addApp.addEventListener("click", async () => {
      picking = !picking;
      if (picking && !apps) apps = (await Bridge.workspacesApps().catch(() => [])) as App[];
      render();
    });
    const text = h("input", { type: "text", class: "sh-folder", spellcheck: "false", placeholder: "~/Documents/Course   or   https://…" }) as HTMLInputElement;
    text.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") addTarget();
    });
    const addTargetBtn = h("button", { class: "hub-btn sm", text: "+ Folder / link" });
    function addTarget() {
      const v = text.value.trim();
      if (!v) return;
      const url = /^https?:\/\//i.test(v);
      const label = url ? v.replace(/^https?:\/\//i, "").replace(/\/$/, "") : v.replace(/\/+$/, "").split("/").pop() || v;
      d.items.push({ kind: url ? "url" : "folder", target: v, label });
      text.value = "";
      render();
    }
    addTargetBtn.addEventListener("click", addTarget);

    const save = h("button", { class: "hub-btn primary sm", text: "Save" });
    save.addEventListener("click", async () => {
      if (!d.name.trim()) {
        result = "Give it a name";
        resultBad = true;
        return render();
      }
      const exists = spaces.some((w) => w.id === d.id);
      const next = exists ? spaces.map((w) => (w.id === d.id ? d : w)) : [...spaces, d];
      if (await persist(next)) {
        draft = null;
        picking = false;
        result = "";
      }
      render();
    });
    const cancel = h("button", { class: "hub-btn sm", text: "Cancel" });
    cancel.addEventListener("click", () => {
      draft = null;
      picking = false;
      result = "";
      render();
    });
    const buttons: Node[] = [save, cancel];
    if (spaces.some((w) => w.id === d.id)) {
      const remove = h("button", { class: "hub-btn sm", text: "Delete workspace" });
      remove.addEventListener("click", async () => {
        if (await persist(spaces.filter((w) => w.id !== d.id))) {
          draft = null;
          picking = false;
        }
        render();
      });
      buttons.push(remove);
    }

    el.append(name, items, h("div", { class: "ws-add" }, addApp, text, addTargetBtn));
    if (picking) el.append(renderPicker(d));
    el.append(h("div", { class: "tk-foot" }, h("div", { class: "pm-btns" }, ...buttons)));
    if (result) el.append(h("div", { class: resultBad ? "hub-err" : "hub-hint", text: result }));
    if (!d.name) setTimeout(() => name.focus(), 0); // a new one starts on its name
  }

  function renderPicker(d: Workspace): HTMLElement {
    const q = h("input", { type: "text", class: "sh-folder", spellcheck: "false", placeholder: "Search the installed apps…" }) as HTMLInputElement;
    const list = h("div", { class: "sh-list ws-apps" });
    const fill = () => {
      clear(list);
      const needle = q.value.trim().toLowerCase();
      const shown = (apps ?? []).filter((a) => !needle || a.name.toLowerCase().includes(needle)).slice(0, 60);
      for (const a of shown) {
        const b = h("button", { class: "ws-app", text: a.name, title: a.id });
        b.addEventListener("click", () => {
          if (!d.items.some((i) => i.target === a.id)) d.items.push({ kind: "app", target: a.id, label: a.name });
          picking = false;
          render();
        });
        list.append(b);
      }
      if (shown.length === 0) list.append(h("div", { class: "hub-hint", text: apps && apps.length === 0 ? "No installed apps were found." : "No app has that name." }));
    };
    q.addEventListener("input", fill);
    q.addEventListener("keydown", (e) => e.stopPropagation());
    fill();
    setTimeout(() => q.focus(), 0);
    return h("div", { class: "ws-picker" }, q, list);
  }

  function render() {
    clear(el);
    if (draft) renderEdit(draft);
    else renderList();
  }

  return {
    el,
    sync() {},
    focus() {},
    async start() {
      if (!loaded) {
        loaded = true;
        spaces = ((await Bridge.workspacesList().catch(() => [])) as Workspace[]) ?? [];
      }
      render();
    },
    stop() {
      // An unsaved draft is dropped when the panel goes away; the saved list is untouched.
      draft = null;
      picking = false;
    },
  };
}

export const workspacesTool: HubTool = { id: "workspaces", label: "Workspaces", typing: true, build };
