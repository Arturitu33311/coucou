// The Hub view and the little runtime that starts and stops its tools.

import { h, clear } from "../views/dom";
import { State } from "../core/state";
import type { ViewHost, ViewActions } from "../views/views";
import type { HubContext, HubHost, HubTool } from "./types";
import { TOOLS } from "./tools";

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
  let to = -1;
  if (e.key === "ArrowRight") to = Math.min(TOOLS.length - 1, at + 1);
  else if (e.key === "ArrowLeft") to = Math.max(0, at - 1);
  else if (/^[1-9]$/.test(e.key)) to = Number(e.key) - 1;
  if (to < 0 || to >= TOOLS.length) return false;
  if (to !== at) selectTool(TOOLS[to].id);
  return true;
}

export function buildHub(actions: ViewActions): ViewHost {
  context = { actions };
  const tabs = h("div", { class: "hub-tabs" });
  const body = h("div", { class: "hub-body" });
  const el = h("div", { class: "view hub" }, h("div", { class: "hub-wrap" }, tabs, body));
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
  // The strip scrolls sideways and its scrollbar is hidden: the ordinary mouse wheel (up and down)
  // moves it, as a touchpad's sideways swipe already does.
  // A soft fade on the side that has more tabs, so the strip does not look cut off.
  const edges = () => {
    tabs.classList.toggle("fade-l", tabs.scrollLeft > 2);
    tabs.classList.toggle("fade-r", tabs.scrollLeft + tabs.clientWidth < tabs.scrollWidth - 2);
  };
  tabs.addEventListener("scroll", edges, { passive: true });
  tabs.addEventListener(
    "wheel",
    (e) => {
      if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
        tabs.scrollLeft += e.deltaY;
        e.preventDefault();
      }
      e.stopPropagation();
    },
    { passive: false },
  );

  return {
    el,
    sync() {
      const key = `${State.hubTab}`;
      if (key !== tabsKey) {
        tabsKey = key;
        // Rebuilding the strip would send it back to the left: keep where it was.
        const kept = tabs.scrollLeft;
        clear(tabs);
        let active: HTMLElement | null = null;
        for (const t of TOOLS) {
          const b = h("button", { class: t.id === State.hubTab ? "on" : "", text: t.label });
          b.addEventListener("click", () => {
            actions.blip();
            selectTool(t.id);
          });
          tabs.append(b);
          if (t.id === State.hubTab) active = b;
        }
        tabs.scrollLeft = kept;
        // With ten tabs the last ones are off to the right: bring the chosen one into view.
        if (active && tabs.clientWidth > 0) {
          const a = active as HTMLElement;
          if (a.offsetLeft < tabs.scrollLeft) tabs.scrollTo({ left: a.offsetLeft - 8, behavior: "smooth" });
          else if (a.offsetLeft + a.offsetWidth > tabs.scrollLeft + tabs.clientWidth) {
            tabs.scrollTo({ left: a.offsetLeft + a.offsetWidth - tabs.clientWidth + 8, behavior: "smooth" });
          }
        }
        edges();
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
