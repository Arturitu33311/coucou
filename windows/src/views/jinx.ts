// The Jinx pill's card: who she is right now, her last words, and the way in.

import { h, dot } from "./dom";
import { JINX_ID, State } from "../core/state";
import type { ViewActions, ViewHost } from "./views";

const STATE_LABEL: Record<string, string> = {
  thinking: "thinking…",
  working: "working…",
  approval: "waiting for you",
  finished: "done",
  error: "something went wrong",
};

export function buildJinxCard(actions: ViewActions): ViewHost {
  const state = h("span", { class: "jx-state" });
  const head = h("div", { class: "jx-head" }, dot("#39FF14", 7), h("span", { text: "Jinx" }), state);
  const reply = h("div", { class: "jx-reply" });
  const talk = h("button", { class: "btn primary", onclick: () => actions.talkToJinx() }, h("span", { text: "Talk to Jinx" }));
  const stop = h("button", { class: "btn secondary", onclick: () => actions.stopJinx() }, h("span", { text: "Stop" }));
  const el = h("div", { class: "jx-card" }, head, reply, h("div", { class: "jx-actions" }, talk, stop));

  return {
    el,
    sync() {
      const task = State.tasks.find((t) => t.id === JINX_ID);
      state.textContent = task ? STATE_LABEL[task.state] ?? "" : "";
      const last = [...State.jinxHistory].reverse().find((m) => m.role === "assistant" && m.content.trim());
      const step = task && task.state === "working" ? task.steps[task.steps.length - 1] : "";
      const text = step || last?.content || "Talk to me here — I also ask here before doing anything risky.";
      reply.textContent = text;
      reply.classList.toggle("hint", !step && !last);
      stop.style.display = State.jinxBusy ? "" : "none";
    },
  };
}
