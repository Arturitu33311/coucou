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

export function buildHub(actions: ViewActions): ViewHost {
  context = { actions };
  const tabs = h("div", { class: "hub-tabs" });
  const body = h("div", { class: "hub-body" });
  const el = h("div", { class: "view hub" }, h("div", { class: "hub-wrap" }, tabs, body));
  let tabsKey = "";
  let mounted = "";
  // The strip scrolls sideways and its scrollbar is hidden: the ordinary mouse wheel (up and down)
  // moves it, as a touchpad's sideways swipe already does.
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
          if (a.offsetLeft < tabs.scrollLeft) tabs.scrollLeft = a.offsetLeft - 8;
          else if (a.offsetLeft + a.offsetWidth > tabs.scrollLeft + tabs.clientWidth) tabs.scrollLeft = a.offsetLeft + a.offsetWidth - tabs.clientWidth + 8;
        }
      }
      if (!toolById(State.hubTab)) State.hubTab = TOOLS[0]?.id ?? "";
      const current = host(State.hubTab);
      if (mounted !== State.hubTab && current) {
        mounted = State.hubTab;
        clear(body);
        body.append(current.el);
      }
      current?.sync();
    },
    focus() {
      host(State.hubTab)?.focus?.();
    },
  };
}
