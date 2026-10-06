// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView from
// IslandViewContent.swift.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { Bridge, type ChatContext } from "../core/bridge";
import { Sound } from "../core/sound";
import { JINX_ID, State, type ChatMessage } from "../core/state";
import { jinxSendFailed, jinxStarted } from "../island/jinx";
import type { ViewHost } from "./views";

let nextId = 1;

function bubble(message: ChatMessage): HTMLElement {
  if (message.role === "user") {
    return h(
      "div",
      { class: "chat-row user" },
      h("div", { class: "bubble", text: message.content }),
    );
  }
  return h("div", { class: "chat-row" }, h("div", { class: "reply", text: message.content }));
}

function typingDots(): HTMLElement {
  return h(
    "div",
    { class: "chat-row" },
    h("div", { class: "typing" }, h("i"), h("i"), h("i")),
  );
}

/** The coloured chip showing what the question is about (a dropped file). */
function contextChip(label: string): HTMLElement {
  const chip = h("div", { class: "chip" }, h("i", { class: "chip-dot" }), h("span", { text: label }));
  requestAnimationFrame(() => chip.classList.add("settled"));
  return chip;
}

/** The thread the chat is showing: Mochi's, or Jinx's when she is the one being talked to. */
function thread(): ChatMessage[] {
  return State.chatTarget === "jinx" ? State.jinxHistory : State.chatHistory;
}

export function buildPrompt(onHeightChange: () => void): ViewHost {
  const chipRow = h("div", { class: "chip-row" });
  // Only when the Jinx pill is on: who the next message goes to.
  const mochiBtn = h("button", { text: "Mochi" });
  const jinxBtn = h("button", { text: "Jinx" });
  const targetRow = h("div", { class: "chat-target" }, mochiBtn, jinxBtn);
  const setTarget = (t: "mochi" | "jinx") => {
    State.chatTarget = t;
    State.notify();
    onHeightChange();
    input.focus();
  };
  mochiBtn.addEventListener("click", () => setTarget("mochi"));
  jinxBtn.addEventListener("click", () => setTarget("jinx"));
  const log = h("div", { class: "chat-log" });
  const input = h("input", {
    type: "text",
    class: "chat-input",
    placeholder: "Ask me anything…",
    spellcheck: "false",
  }) as HTMLInputElement;
  const send = h("button", { class: "send-btn", title: "Send" }, svg(ICONS.arrowUp, 11));
  const bar = h("div", { class: "chat-bar" }, input, send);

  const el = h(
    "div",
    { class: "view" },
    h("div", { class: "card wash chat-card" }, h("div", { class: "chat-body" }, targetRow, chipRow, log, bar)),
  );
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");

  let sending = false;
  let renderedKey = "";

  /** One message to Jinx: the answer arrives as events (see island/jinx.ts). */
  async function submitToJinx(query: string) {
    // As with Mochi, the dropped file rides along with the thread's first message
    // (Jinx keeps the session); she gets its text, so what she can't read is refused.
    const file = State.droppedFile;
    const context: ChatContext | null =
      file && State.jinxFilePath !== file.path ? { kind: "file", name: file.name, path: file.path } : null;
    State.jinxHistory.push({ id: nextId++, role: "user", content: query });
    Sound.play("send");
    jinxStarted();
    State.notify();
    onHeightChange();
    try {
      await Bridge.jinxSend(query, context);
      if (context) State.jinxFilePath = context.kind === "file" ? context.path ?? null : null;
    } catch (err) {
      jinxSendFailed(String(err).replace(/^Error:\s*/, ""));
    } finally {
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  async function submit() {
    const query = input.value.trim();
    if (!query || sending) return;
    if (State.chatTarget === "jinx") {
      if (State.jinxBusy) return;
      input.value = "";
      await submitToJinx(query);
      return;
    }
    input.value = "";
    sending = true;
    Sound.play("send");

    State.chatHistory.push({ id: nextId++, role: "user", content: query });
    State.stateOverride = "thinking";
    State.notify();
    onHeightChange();

    const file = State.droppedFile;
    const context: ChatContext | null =
      State.chatHistory.length === 1 && file ? { kind: "file", name: file.name, path: file.path } : null;

    try {
      const reply = await Bridge.chatSend(query, context);
      State.chatHistory.push({ id: nextId++, role: "assistant", content: reply.text });
      State.stateOverride = null;
      Sound.play("finish");
    } catch (err) {
      State.stateOverride = null;
      State.noteMessage = String(err).replace(/^Error:\s*/, "");
      State.view = "note";
      Sound.play("error");
    } finally {
      sending = false;
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  send.addEventListener("click", () => void submit());
  input.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") {
      e.preventDefault();
      void submit();
    }
    e.stopPropagation(); // Escape closes the island, not the chat
  });

  return {
    el,
    sync() {
      const file = State.droppedFile;
      const wantChip = file?.name ?? "";
      if (chipRow.dataset.label !== wantChip) {
        chipRow.dataset.label = wantChip;
        clear(chipRow);
        if (wantChip) chipRow.append(contextChip(wantChip));
      }

      const jinxAvailable = State.tasks.some((t) => t.id === JINX_ID);
      if (!jinxAvailable) State.chatTarget = "mochi";
      const toJinx = State.chatTarget === "jinx";
      targetRow.style.display = jinxAvailable ? "" : "none";
      mochiBtn.classList.toggle("on", !toJinx);
      jinxBtn.classList.toggle("on", toJinx);

      const history = thread();
      const last = history[history.length - 1];
      // Jinx is "thinking" until her first words arrive.
      const thinking = toJinx
        ? State.jinxBusy && (!last || last.role === "user")
        : State.stateOverride === "thinking";
      const key = `${State.chatTarget}:${history.length}:${last?.content.length ?? 0}:${thinking}`;
      if (key !== renderedKey) {
        renderedKey = key;
        clear(log);
        for (const m of history) log.append(bubble(m));
        if (thinking) log.append(typingDots());
        log.scrollTop = log.scrollHeight;
      }

      input.placeholder = toJinx
        ? (history.length === 0 ? "Talk to Jinx…" : "Continue with Jinx…")
        : (State.chatHistory.length === 0 ? "Ask me anything…" : "Continue…");
      input.disabled = toJinx ? State.jinxBusy : sending;
    },
    focus() {
      input.focus();
      input.select();
    },
  };
}
