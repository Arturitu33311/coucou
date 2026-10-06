// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView from
// IslandViewContent.swift, extended with who you are talking to: the running
// Claude Code sessions (each with its state), a new agent, Jinx, and Mochi's own
// API chat when there is a key for it.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { Bridge, type ChatContext } from "../core/bridge";
import { Sound } from "../core/sound";
import { JINX_ID, State, type ChatMessage } from "../core/state";
import { jinxSendFailed, jinxStarted } from "../island/jinx";
import {
  adoptAgent, agentNote, agentsLoaded, agentThread, selectedAgent, startAgentSync, syncAgentsNow,
} from "../island/agents";
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
  if (message.role === "queued") {
    // Typed while the agent was busy: it is waiting its turn, and shows so.
    return h(
      "div",
      { class: "chat-row user queued" },
      h("div", { class: "bubble", text: message.content }),
      h("div", { class: "queued-tag", text: "⏳ queued — the agent is busy" }),
    );
  }
  if (message.role === "tool") {
    return h("div", { class: "chat-row" }, h("div", { class: "chat-tool", text: `⚙ ${message.content}` }));
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

/** The thread the chat is showing, whoever it is with. */
function thread(): ChatMessage[] {
  switch (State.chatTarget) {
    case "jinx": return State.jinxHistory;
    case "agent": return agentThread();
    case "new": return [];
    default: return State.chatHistory;
  }
}

const STATE_TEXT: Record<string, string> = {
  working: "working…",
  done: "done — waiting for your next message",
};

/** "5h 9% · 7d 59%": a window that has already reset since it was written shows a dash. */
function limitsText(): { text: string; title: string; level: string } {
  const l = State.limits;
  if (!l) return { text: "", title: "", level: "" };
  const now = Date.now() / 1000;
  const part = (label: string, w: typeof l.five) => {
    if (!w) return null;
    const stale = w.resetsAt != null && w.resetsAt < now;
    return { label, pct: stale ? null : w.pct, resets: w.resetsAt };
  };
  const parts = [part("5h", l.five), part("7d", l.seven)].filter((p) => p !== null);
  const worst = Math.max(0, ...parts.map((p) => p.pct ?? 0));
  const when = (t: number | null) => (t ? new Date(t * 1000).toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" }) : "");
  return {
    text: parts.map((p) => `${p.label} ${p.pct == null ? "–" : `${p.pct}%`}`).join(" · "),
    title: parts.map((p) => `${p.label} window${p.resets ? ` resets ${when(p.resets)}` : ""}`).join("\n"),
    level: worst >= 95 ? "crit" : worst >= 80 ? "warn" : "",
  };
}

function short(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

export function buildPrompt(onHeightChange: () => void): ViewHost {
  const chipRow = h("div", { class: "chip-row" });
  // Who the next message goes to.
  const targetRow = h("div", { class: "chat-target" });
  const statusTxt = h("span", {});
  const cwdSel = h("select", { class: "agent-cwd", title: "Folder the new agent works in" }) as HTMLSelectElement;
  // Usage limits, small, at the end of the line: "5h 9% · 7d 59%".
  const limitsEl = h("span", { class: "limits" });
  const statusRow = h("div", { class: "agent-status" }, statusTxt, cwdSel, limitsEl);
  let targetKey = "";
  let cwdKey = "";

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
    h(
      "div",
      { class: "card wash chat-card" },
      h("div", { class: "chat-body" }, targetRow, statusRow, chipRow, log, bar),
    ),
  );
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");

  let sending = false;
  let renderedKey = "";

  function pick(target: typeof State.chatTarget, agentId: string | null = null) {
    State.chatTarget = target;
    if (agentId) State.agentId = agentId;
    State.notify();
    onHeightChange();
    if (target === "agent") void syncAgentsNow();
    input.focus();
  }

  /** The chips: running agents (with their state), a new one, Jinx, Mochi's API chat. */
  function renderTargets() {
    const jinxOn = State.tasks.some((t) => t.id === JINX_ID);
    const key = [
      State.chatTarget, State.agentId, jinxOn, State.mochiApi,
      State.agents.map((a) => `${a.id}:${a.name}:${a.state}`).join("|"),
    ].join("~");
    if (key === targetKey) return;
    targetKey = key;
    clear(targetRow);
    for (const a of State.agents) {
      const on = State.chatTarget === "agent" && State.agentId === a.id;
      const chip = h(
        "button",
        { class: on ? "agent on" : "agent", title: `${a.name} — ${a.cwd}` },
        h("i", { class: `sdot ${a.state}` }),
        h("span", { text: short(a.name || a.id, 18) }),
      );
      chip.addEventListener("click", () => pick("agent", a.id));
      targetRow.append(chip);
    }
    const add = h("button", { class: State.chatTarget === "new" ? "on" : "", text: "+ New" });
    add.addEventListener("click", () => pick("new"));
    targetRow.append(add);
    if (jinxOn) {
      const j = h("button", { class: State.chatTarget === "jinx" ? "jinx on" : "jinx", text: "Jinx" });
      j.addEventListener("click", () => pick("jinx"));
      targetRow.append(j);
    }
    if (State.mochiApi) {
      const m = h("button", { class: State.chatTarget === "mochi" ? "on" : "", text: "Mochi (API)" });
      m.addEventListener("click", () => pick("mochi"));
      targetRow.append(m);
    }
  }

  /** The folders a new agent can start in: where the running ones are, and home. */
  function renderFolders() {
    const dirs = [...new Set(State.agents.map((a) => a.cwd).filter(Boolean))];
    const key = dirs.join("|");
    if (key === cwdKey) return;
    cwdKey = key;
    const keep = cwdSel.value;
    clear(cwdSel);
    cwdSel.append(h("option", { value: "", text: "~ (home)" }));
    for (const d of dirs) cwdSel.append(h("option", { value: d, text: short(d.replace(/^\/home\/[^/]+/, "~"), 48) }));
    cwdSel.value = dirs.includes(keep) ? keep : "";
  }

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

  /** A dropped file reaches a Claude Code agent as its path: it reads it itself. */
  function withFile(query: string): { text: string; sentPath: string | null } {
    const file = State.droppedFile;
    if (file && State.agentFilePath !== file.path) {
      return { text: `File: ${file.path}\n\n${query}`, sentPath: file.path };
    }
    return { text: query, sentPath: null };
  }

  /** A message into a running Claude Code session; its reply is read from its transcript. */
  async function submitToAgent(query: string) {
    const agent = selectedAgent();
    if (!agent) return;
    const { text, sentPath } = withFile(query);
    sending = true;
    Sound.play("send");
    State.notify();
    try {
      await Bridge.agentSend(agent.id, text);
      if (sentPath) State.agentFilePath = sentPath;
      await syncAgentsNow();
    } catch (err) {
      agentNote(`⚠ ${String(err).replace(/^Error:\s*/, "")}`);
      Sound.play("error");
    } finally {
      sending = false;
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  /** A new background agent; once it is running the chat moves onto it. */
  async function submitNew(query: string) {
    const { text, sentPath } = withFile(query);
    sending = true;
    Sound.play("send");
    State.notify();
    try {
      const id = await Bridge.agentStart(cwdSel.value, text);
      if (sentPath) State.agentFilePath = sentPath;
      await adoptAgent(id);
    } catch (err) {
      State.agentsError = String(err).replace(/^Error:\s*/, "");
      Sound.play("error");
    } finally {
      sending = false;
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
    if (State.chatTarget === "agent") {
      if (selectedAgent()?.state === "blocked") return;
      input.value = "";
      await submitToAgent(query);
      return;
    }
    if (State.chatTarget === "new") {
      input.value = "";
      await submitNew(query);
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
      startAgentSync();
      const file = State.droppedFile;
      const wantChip = file?.name ?? "";
      if (chipRow.dataset.label !== wantChip) {
        chipRow.dataset.label = wantChip;
        clear(chipRow);
        if (wantChip) chipRow.append(contextChip(wantChip));
      }

      // Without Jinx's pill there is no Jinx to talk to; without an API key, no Mochi.
      if (!State.tasks.some((t) => t.id === JINX_ID) && State.chatTarget === "jinx") State.chatTarget = "new";
      // (pickFirstTarget in island/agents.ts chooses it once the list of agents is known.)
      if (State.chatTarget === "mochi" && State.mochiApi === false && State.chatPicked && agentsLoaded()) {
        State.chatTarget = State.agents.length > 0 ? "agent" : "new";
        if (State.chatTarget === "agent") State.agentId = State.agentId ?? State.agents[0].id;
      }
      renderTargets();
      renderFolders();

      const target = State.chatTarget;
      const agent = target === "agent" ? selectedAgent() : undefined;
      const blocked = agent?.state === "blocked";

      // One line under the chips: the selected agent's state, or what is wrong.
      let status = "";
      if (State.agentsError && (target === "agent" || target === "new")) status = `⚠ ${State.agentsError}`;
      else if (agent && blocked) status = `waiting for your answer — answer its card, or run: claude attach ${agent.id}`;
      else if (agent) status = STATE_TEXT[agent.state] ?? agent.state;
      else if (target === "new") status = "starts a background agent in:";
      statusTxt.textContent = status;
      cwdSel.style.display = target === "new" && !State.agentsError ? "" : "none";
      statusRow.className = `agent-status${blocked ? " blocked" : ""}${agent?.state === "working" ? " working" : ""}`;
      const lim = limitsText();
      limitsEl.textContent = lim.text;
      limitsEl.title = lim.title;
      limitsEl.className = `limits ${lim.level}`;
      statusRow.style.display = status || lim.text ? "" : "none";

      const history = thread();
      const last = history[history.length - 1];
      // Jinx is "thinking" until her first words arrive; an agent while it works.
      const thinking =
        target === "jinx" ? State.jinxBusy && (!last || last.role === "user")
        : target === "agent" ? sending || agent?.state === "working"
        : target === "new" ? sending
        : State.stateOverride === "thinking";
      const key = `${target}:${State.agentId}:${history.length}:${last?.content.length ?? 0}:${thinking}`;
      if (key !== renderedKey) {
        renderedKey = key;
        clear(log);
        for (const m of history) log.append(bubble(m));
        if (thinking) log.append(typingDots());
        log.scrollTop = log.scrollHeight;
      }

      input.placeholder =
        target === "jinx" ? (history.length === 0 ? "Talk to Jinx…" : "Continue with Jinx…")
        : target === "agent" ? (blocked ? "Waiting for your answer…" : `Message ${short(agent?.name ?? "agent", 24).replace(/…$/, "")}…`)
        : target === "new" ? "What should the new agent do?"
        : State.chatHistory.length === 0 ? "Ask me anything…" : "Continue…";
      input.disabled = target === "jinx" ? State.jinxBusy : target === "agent" ? sending || blocked : sending;
    },
    focus() {
      startAgentSync();
      input.focus();
      input.select();
    },
  };
}
