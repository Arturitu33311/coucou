// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView from
// IslandViewContent.swift, extended with who you are talking to: the running
// agent sessions (Claude Code and OpenCode, each with its state), a new agent,
// Jinx, and Mochi's own chat with an AI provider when one is set up.
//
// Mochi's chat is upstream's: answers are rendered as Markdown (markdown.ts), a
// local model streams its answer (`chat-delta` events), and the model name above
// the text field opens the picker — provider chips, then that provider's models.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { renderMarkdown } from "./markdown";
import { Bridge, onEvent, type ChatContext, type ModelInfo } from "../core/bridge";
import {
  activeModel, pickModel, providerDef, visibleProviders, withModel, type ProviderDef,
} from "../core/providers";
import { Sound } from "../core/sound";
import { JINX_ID, BACKEND_MOCHI, State, type ChatMessage } from "../core/state";
import { jinxSendFailed, jinxStarted, loadJinxThread } from "../island/jinx";
import { modeLabel } from "../core/jinx-thread";
import {
  adoptAgent, agentNote, agentsLoaded, agentThread, selectedAgent, startAgentSync, syncAgentsNow,
} from "../island/claude-sessions";
import type { ViewHost } from "./views";
import { offerOf } from "./yesno";
import { N_, t, tl } from "../i18n/i18n";

const STRINGS = {
  placeholderFirst: N_("Ask me anything…"),
  placeholderNext: N_("Continue…"),
  send: N_("Send"),
  switchModel: N_("Switch provider or model"),
  noModel: N_("Choose a model"),
  loading: N_("Loading models…"),
  noKey: N_("No API key — add it in Settings."),
  openSettings: N_("Open Settings"),
};

let nextId = 1;

function bubble(message: ChatMessage): HTMLElement {
  if (message.role === "user") {
    return h(
      "div",
      { class: message.via ? "chat-row user via" : "chat-row user" },
      // Jinx's shared thread says where each line was written (Telegram, a phone, this PC).
      message.via ? h("div", { class: "chat-via", text: message.via }) : null,
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
  const reply = h("div", { class: "reply" });
  renderMarkdown(reply, message.content);
  return h("div", { class: "chat-row" }, reply);
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

function saveSettings() {
  void Bridge.saveSettings(State.settings);
}

// ── Model picker ──────────────────────────────────────────────────────────────

interface Picker {
  el: HTMLElement;
  open(): void;
  close(): void;
  readonly isOpen: boolean;
}

/** Provider chips, then the chosen provider's models — ModelPickerView. */
function buildPicker(onChange: () => void): Picker {
  const chips = h("div", { class: "picker-chips" });
  const list = h("div", { class: "picker-list" });
  const el = h("div", { class: "picker" }, chips, h("div", { class: "picker-rule" }), list);

  /** Models already asked for, by provider; a model server is asked again each time. */
  const cache = new Map<string, ModelInfo[]>();
  let isOpen = false;
  let request = 0;

  function drawChips() {
    clear(chips);
    for (const p of visibleProviders(State.settings)) {
      const on = p.id === State.settings.chatProvider;
      const chip = h(
        "button",
        { class: on ? "picker-chip on" : "picker-chip", style: `--accent:${p.accent}` },
        h("i", { class: "picker-dot" }),
        h("span", { text: t(p.name) }),
      );
      chip.addEventListener("click", () => {
        if (p.id === State.settings.chatProvider) return;
        State.settings = { ...State.settings, chatProvider: p.id };
        saveSettings();
        Sound.play("pop");
        drawChips();
        void loadModels();
        onChange();
      });
      chips.append(chip);
    }
  }

  function status(text: string, withSettings = false) {
    clear(list);
    const line = h("div", { class: "picker-status", text });
    if (withSettings) {
      line.append(
        h("button", {
          class: "picker-link",
          text: tl(STRINGS.openSettings),
          onclick: () => void Bridge.openSettingsWindow(),
        }),
      );
    }
    list.append(line);
  }

  function drawModels(p: ProviderDef, models: ModelInfo[]) {
    clear(list);
    const current = activeModel(State.settings);
    for (const m of models) {
      const on = m.id === current;
      const row = h(
        "button",
        { class: on ? "picker-model on" : "picker-model", style: `--accent:${p.accent}`, title: m.id },
        h("span", { class: "picker-model-name", text: m.label }),
        on ? svg(ICONS.check, 11, { stroke: 2.2 }) : null,
      );
      row.addEventListener("click", () => {
        State.settings = withModel(State.settings, p.id, m.id);
        saveSettings();
        Sound.play("blip");
        close();
      });
      list.append(row);
    }
    list.querySelector(".picker-model.on")?.scrollIntoView({ block: "nearest" });
  }

  async function loadModels() {
    const p = providerDef(State.settings.chatProvider);
    const ticket = ++request;
    const cached = p.urlField ? undefined : cache.get(p.id);
    if (cached) {
      drawModels(p, cached);
      return;
    }
    // Nothing is asked of a provider that has no key yet.
    if (p.key && !(await Bridge.secretPresent(p.key))) {
      if (ticket === request) status(t(STRINGS.noKey), true);
      return;
    }
    if (ticket !== request) return;
    status(t(STRINGS.loading));
    try {
      const models = await Bridge.chatModels(p.id);
      if (ticket !== request) return;
      if (!p.urlField) cache.set(p.id, models);
      const keep = pickModel(p, models.map((m) => m.id), activeModel(State.settings));
      if (keep && keep !== activeModel(State.settings)) {
        State.settings = withModel(State.settings, p.id, keep);
        saveSettings();
        onChange();
      }
      drawModels(p, models);
    } catch (err) {
      if (ticket === request) status(String(err).replace(/^Error:\s*/, ""), Boolean(p.key));
    }
  }

  function open() {
    isOpen = true;
    el.classList.add("on");
    drawChips();
    onChange();
    void loadModels();
  }

  function close() {
    isOpen = false;
    request++;
    el.classList.remove("on");
    onChange();
  }

  return {
    el,
    open,
    close,
    get isOpen() {
      return isOpen;
    },
  };
}

// ── View ──────────────────────────────────────────────────────────────────────

export function buildPrompt(onHeightChange: () => void): ViewHost {
  const chipRow = h("div", { class: "chip-row" });
  // Who the next message goes to.
  const targetRow = h("div", { class: "chat-target" });
  const statusTxt = h("span", {});
  const cwdSel = h("select", { class: "agent-cwd", title: "Folder the new agent works in" }) as HTMLSelectElement;
  const backendSel = h("select", { class: "agent-cwd", title: "What runs the new agent" }) as HTMLSelectElement;
  backendSel.append(h("option", { value: "claude", text: "Claude Code" }));
  backendSel.append(h("option", { value: "opencode", text: "OpenCode" }));
  backendSel.addEventListener("change", () => {
    State.chatBackend = backendSel.value as "claude" | "opencode";
    State.notify();
  });
  // Usage limits, small, at the end of the line: "5h 9% · 7d 59%".
  const limitsEl = h("span", { class: "limits" });
  const statusRow = h("div", { class: "agent-status" }, statusTxt, cwdSel, backendSel, limitsEl);
  let targetKey = "";
  let cwdKey = "";

  const log = h("div", { class: "chat-log" });
  const input = h("input", {
    type: "text",
    class: "chat-input",
    placeholder: t(STRINGS.placeholderFirst),
    spellcheck: "false",
  }) as HTMLInputElement;
  const send = h("button", { class: "send-btn", title: tl(STRINGS.send) }, svg(ICONS.arrowUp, 11));
  const bar = h("div", { class: "chat-bar" }, input, send);

  // Mochi's provider and model (shown only when the chat is with Mochi).
  const modelDot = h("i", { class: "model-dot" });
  const modelName = h("span", { class: "model-name" });
  const modelBtn = h(
    "button",
    { class: "model-btn", title: tl(STRINGS.switchModel) },
    modelDot,
    modelName,
    svg(ICONS.chevronUpDown, 9, { stroke: 2 }),
  );
  const modelRow = h("div", { class: "model-row" }, modelBtn);

  const body = h("div", { class: "chat-body" });
  const picker = buildPicker(() => {
    body.classList.toggle("picking", picker.isOpen);
    drawModelButton();
  });
  body.append(targetRow, statusRow, chipRow, log, picker.el, modelRow, bar);

  const el = h("div", { class: "view" }, h("div", { class: "card wash chat-card" }, body));
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");

  let sending = false;
  let renderedKey = "";
  // A local model answers token by token: where its text so far is shown.
  let live: HTMLElement | null = null;

  function drawModelButton() {
    const p = providerDef(State.settings.chatProvider);
    modelDot.style.background = p.accent;
    modelName.textContent = activeModel(State.settings) || t(STRINGS.noModel);
    modelBtn.classList.toggle("open", picker.isOpen);
    modelBtn.disabled = sending;
  }

  modelBtn.addEventListener("click", () => {
    if (sending) return;
    if (picker.isOpen) picker.close();
    else picker.open();
  });

  void onEvent<string>("chat-delta", (text) => {
    if (!sending || State.chatTarget !== "mochi" || !text) return; // nothing visible yet: the dots stay
    if (!live) {
      live = h("div", { class: "reply" });
      log.querySelector(".typing")?.parentElement?.remove();
      log.append(h("div", { class: "chat-row" }, live));
    }
    renderMarkdown(live, text);
    log.scrollTop = log.scrollHeight;
  });

  function pick(target: typeof State.chatTarget, agentId: string | null = null) {
    if (target !== "mochi" && picker.isOpen) picker.close();
    State.chatTarget = target;
    if (agentId) State.agentId = agentId;
    State.notify();
    onHeightChange();
    if (target === "agent") void syncAgentsNow();
    input.focus();
  }

  /** The chips: running agents (Claude Code and OpenCode, with their state), a new one, Jinx, Mochi's API chat. */
  function renderTargets() {
    const jinxOn = State.tasks.some((t) => t.id === JINX_ID);
    const key = [
      State.chatTarget, State.agentId, jinxOn, State.mochiApi,
      State.agents.map((a) => `${a.id}:${a.name}:${a.state}:${a.kind}`).join("|"),
    ].join("~");
    if (key === targetKey) return;
    targetKey = key;
    clear(targetRow);
    for (const a of State.agents) {
      const on = State.chatTarget === "agent" && State.agentId === a.id;
      const who = a.kind === "opencode" ? BACKEND_MOCHI.opencode.name : BACKEND_MOCHI.claude.name;
      const chip = h(
        "button",
        { class: on ? `agent on ${a.kind}` : `agent ${a.kind}`, title: `${who} · ${a.name} — ${a.cwd}` },
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

  /** A new background agent (Claude Code) or session (OpenCode); once it is running the chat moves onto it. */
  async function submitNew(query: string) {
    const { text, sentPath } = withFile(query);
    sending = true;
    Sound.play("send");
    State.notify();
    try {
      const id = await Bridge.agentStart(cwdSel.value, text, backendSel.value as "claude" | "opencode");
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
    if (picker.isOpen) picker.close();
    input.value = "";
    sending = true;
    drawModelButton();
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
      live = null;
      renderedKey = ""; // the finished answer replaces the streamed one
      drawModelButton();
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  send.addEventListener("click", () => void submit());
  input.addEventListener("keydown", (e) => {
    const key = (e as KeyboardEvent).key;
    if (key === "Enter") {
      e.preventDefault();
      void submit();
    } else if (key === "Escape" && picker.isOpen) {
      e.preventDefault();
      picker.close();
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
      else if (agent && blocked) {
        status = agent.kind === "opencode"
          ? "waiting for your answer"
          : `waiting for your answer — answer its card, or run: claude attach ${agent.id}`;
      }
      else if (agent) status = STATE_TEXT[agent.state] ?? agent.state;
      else if (target === "new") status = "starts a background agent in:";
      else if (target === "jinx") {
        // Through the gateway: the thread shared with the phones and Telegram, and who answered last.
        void loadJinxThread();
        status = modeLabel(State.jinxMode);
      }
      statusTxt.textContent = status;
      cwdSel.style.display = target === "new" && !State.agentsError ? "" : "none";
      backendSel.style.display = target === "new" && !State.agentsError ? "" : "none";
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
      // A reply that ends on one yes/no question gets its two buttons (and only while it is the last
      // thing said and nothing is on its way).
      const offer = !thinking && !blocked && last?.role === "assistant" ? offerOf(last.content) : null;
      const key = `${target}:${State.agentId}:${history.length}:${last?.content.length ?? 0}:${thinking}:${offer ? 1 : 0}`;
      if (key !== renderedKey && !live) {
        renderedKey = key;
        clear(log);
        for (const m of history) log.append(bubble(m));
        if (thinking) log.append(typingDots());
        if (offer) {
          const answer = (word: string) => {
            input.value = word;
            void submit();
          };
          const yes = h("button", { class: "yn yes", text: offer.yes, title: offer.question });
          const no = h("button", { class: "yn no", text: offer.no, title: offer.question });
          yes.addEventListener("click", () => answer(offer.yes));
          no.addEventListener("click", () => answer(offer.no));
          log.append(h("div", { class: "yn-row" }, yes, no));
        }
        log.scrollTop = log.scrollHeight;
      }

      // The model row is Mochi's; leaving the chat (or Mochi) folds the picker away.
      modelRow.style.display = target === "mochi" ? "" : "none";
      if ((State.view !== "prompt" || target !== "mochi") && picker.isOpen) picker.close();
      drawModelButton();

      input.placeholder =
        target === "jinx" ? (history.length === 0 ? "Talk to Jinx…" : "Continue with Jinx…")
        : target === "agent" ? (blocked ? "Waiting for your answer…" : `Message ${short(agent?.name ?? "agent", 24).replace(/…$/, "")}…`)
        : target === "new" ? "What should the new agent do?"
        : t(State.chatHistory.length === 0 ? STRINGS.placeholderFirst : STRINGS.placeholderNext);
      input.disabled = target === "jinx" ? State.jinxBusy : target === "agent" ? sending || blocked : sending;
    },
    focus() {
      startAgentSync();
      input.focus();
      input.select();
    },
  };
}
