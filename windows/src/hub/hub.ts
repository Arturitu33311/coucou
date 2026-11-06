// The Hub view and the little runtime that starts and stops its tools.

import { h, clear } from "../views/dom";
import { State } from "../core/state";
import type { ViewHost, ViewActions } from "../views/views";
import type { HubContext, HubHost, HubTool } from "./types";
import { TOOLS, HUB_GROUPS, toolsOf } from "./tools";

/** The tool each group was last left on, so a group chip returns to where you were in it. */
const lastInGroup = new Map<string, string>();

const built = new Map<string, HubHost>();
let running: string | null = null;

export function toolById(id: string): HubTool | undefined {
  return TOOLS.find((t) => t.id === id);
}

let context: HubContext | null = null;

function host(id: string): HubHost | null {
  let hst = built.get(id) ?? null;
  if (!hst) {
    const tool = toolById(id);
    if (!tool || !context) return null;
    hst = tool.build(context);
    built.set(id, hst);
  }
  return hst;
}

/** The tool whose panel is on screen right now, if any. */
export function hubShown(): string | null {
  return State.mode === "expanded" && State.view === "tool" ? State.hubTab : null;
}

/** Does the panel on screen have a text field? */
export function hubTyping(): boolean {
  const id = hubShown();
  return !!id && !!toolById(id)?.typing;
}

/** Called on every redraw: the shown tool starts, the one that was shown stops. */
export function syncHubRuntime() {
  const want = hubShown();
  if (want === running) return;
  if (running) built.get(running)?.stop?.();
  running = want;
  if (want) host(want)?.start?.();
}

export function selectTool(id: string) {
  State.hubTab = id;
  const g = toolById(id)?.group;
  if (g) lastInGroup.set(g, id);
  try {
    window.localStorage.setItem("coucou.hubTab", id);
  } catch {
    /* a remembered tab is a convenience */
  }
  State.notify();
}

/**
 * Keys for the Hub's tab strip while its panel is on screen: ← / → move along it, 1–9 pick a tab.
 * Ignored with a modifier, and while a text field has the keyboard (a tool's panel owns its keys).
 * True when the key was used.
 */
export function hubKey(e: KeyboardEvent): boolean {
  if (!hubShown() || e.ctrlKey || e.metaKey || e.altKey) return false;
  const t = e.target as HTMLElement | null;
  if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return false;
  const at = TOOLS.findIndex((x) => x.id === State.hubTab);
  // ← / → walk the whole Hub in order (crossing groups); 1–9 pick a tab inside the current group.
  if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
    const to = e.key === "ArrowRight" ? Math.min(TOOLS.length - 1, at + 1) : Math.max(0, at - 1);
    if (to !== at) selectTool(TOOLS[to].id);
    return true;
  }
  if (/^[1-9]$/.test(e.key)) {
    const group = toolsOf(toolById(State.hubTab)?.group ?? "today");
    const pick = group[Number(e.key) - 1];
    if (!pick) return false;
    if (pick.id !== State.hubTab) selectTool(pick.id);
    return true;
  }
  return false;
}

export function buildHub(actions: ViewActions): ViewHost {
  context = { actions };
  const groups = h("div", { class: "hub-groups" });
  const tabs = h("div", { class: "hub-tabs" });
  const body = h("div", { class: "hub-body" });
  const el = h("div", { class: "view hub" }, h("div", { class: "hub-wrap" }, groups, tabs, body));
  let tabsKey = "";
  let mounted = "";
  // A click inside the panel is the user saying "I am here": the island takes the keyboard so
  // ← → and 1–9 work (island.ts releases it when the pointer leaves).
  el.addEventListener(
    "pointerdown",
    () => {
      if (State.hubEngaged) return;
      State.hubEngaged = true;
      State.notify();
    },
    true,
  );
  // The wheel over the tab rows belongs to the Hub, not to the island's pill switching.
  for (const row of [groups, tabs]) row.addEventListener("wheel", (e) => e.stopPropagation(), { passive: true });

  return {
    el,
    sync() {
      const key = `${State.hubTab}`;
      if (key !== tabsKey) {
        tabsKey = key;
        const curGroup = toolById(State.hubTab)?.group ?? HUB_GROUPS[0].id;
        // Row one: the groups. A chip returns to the tab you last had open in that group.
        clear(groups);
        for (const g of HUB_GROUPS) {
          const b = h("button", { class: g.id === curGroup ? "on" : "", text: g.label });
          b.addEventListener("click", () => {
            actions.blip();
            const first = toolsOf(g.id)[0];
            const back = lastInGroup.get(g.id) ?? first?.id;
            if (back && g.id !== curGroup) selectTool(back);
          });
          groups.append(b);
        }
        // Row two: the tabs of the open group (2–4 of them, so no scrolling). A lone tab needs no row.
        clear(tabs);
        const inGroup = toolsOf(curGroup);
        tabs.style.display = inGroup.length > 1 ? "" : "none";
        for (const t of inGroup) {
          const b = h("button", { class: t.id === State.hubTab ? "on" : "", text: t.label });
          b.addEventListener("click", () => {
            actions.blip();
            selectTool(t.id);
          });
          tabs.append(b);
        }
      }
      if (!toolById(State.hubTab)) State.hubTab = TOOLS[0]?.id ?? "";
      const current = host(State.hubTab);
      if (mounted !== State.hubTab && current) {
        const from = TOOLS.findIndex((t) => t.id === mounted);
        const to = TOOLS.findIndex((t) => t.id === State.hubTab);
        mounted = State.hubTab;
        clear(body);
        body.append(current.el);
        // The panel slides in from the side its tab sits on; the very first one just appears.
        if (from >= 0) {
          current.el.style.setProperty("--dx", `${to > from ? 12 : -12}px`);
          current.el.classList.remove("hub-enter");
          void current.el.offsetWidth; // restart the animation when the same panel comes back
          current.el.classList.add("hub-enter");
        }
      }
      current?.sync();
    },
    focus() {
      host(State.hubTab)?.focus?.();
    },
  };
}
