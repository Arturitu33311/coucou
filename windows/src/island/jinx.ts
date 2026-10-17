// Jinx (Hermes) as a character: turns the `jinx` events Rust streams into chat
// bubbles, the pill's state and — when Jinx wants to run something risky — the
// same approval card Claude Code's permission requests use.

import { Bridge, onEvent } from "../core/bridge";
import { Sound } from "../core/sound";
import { JINX_ID, State, type ChatMessage } from "../core/state";
import { threadMessages } from "../core/jinx-thread";
import { replyEmote } from "../core/jinx-mood";
import type { Island } from "./island";

type Approval = { kind: "approval"; runId: string; requestId: string; command: string; description: string };
type JinxEvent =
  | { kind: "delta"; text: string }
  | { kind: "tool"; name: string; preview: string }
  | Approval
  | { kind: "resolved"; requestId: string }
  | { kind: "done"; text: string; mode?: string; mood?: string }
  | { kind: "error"; message: string };

/** How long Mochi shows the mood of Jinx's reply: long enough to be seen when the island opens on it. */
const JINX_EMOTE_S = 3.2;

let nextId = 1_000_000;
/** The assistant bubble being filled by the current run, once it has started one. */
let streaming: ChatMessage | null = null;
/** Approvals that arrived while another card held the island. */
const waiting: Approval[] = [];

let islandRef: Island | null = null;

export function registerJinxHandlers(island: Island) {
  islandRef = island;
  void onEvent<JinxEvent>("jinx", (ev) => handle(island, ev));
  void onEvent<IslandMsg>("jinx-island", (m) => showIslandMessage(island, m));
}

/** What Jinx sent through the presence server while this laptop is the device in use (presence.rs). */
type IslandMsg = { id: number; text: string; kind: string; source: string; mood: string | null };

/** Is Jinx's chat what the island shows right now? */
function chatOpen(): boolean {
  return State.focusId === JINX_ID && State.mode === "expanded";
}

/**
 * Shown on the island, since Alan is here and not on Telegram: a reply to something written elsewhere comes
 * with the shared thread (unless her chat is open, where it is already); anything else is said as a line.
 */
export function showIslandMessage(island: Island, m: IslandMsg) {
  if (m.source === "jinx-chat") {
    void loadJinxThread(true);
    if (chatOpen()) return;
  } else {
    say(m.text);
  }
  if (!chatOpen()) State.setPillBadge(JINX_ID, "finished");
  Sound.play(m.kind === "alert" ? "approval" : "finish");
  if (State.mode === "hidden") island.reveal();
  const emote = replyEmote(m.mood, m.text);
  if (emote) island.emote(emote, JINX_EMOTE_S);
  State.notify();
}

/** Through the gateway Jinx's questions wait on the server; they are looked for this often while she works. */
const APPROVAL_POLL_MS = 3_000;
/** Questions already shown (run|request), so each one gets one card. */
const gatewaySeen = new Set<string>();
let polling = false;
const keyOf = (run: string, request: string) => `${run}|${request}`;

/**
 * While a message waits for Jinx through the gateway, the questions she asks (a command Hermes flagged) are
 * on the server: they get the same card as the Hermes API's. One answered on the phone loses its card here.
 * With the Hermes API the list is always empty (its questions arrive as events).
 */
async function pollGatewayApprovals() {
  if (polling) return;
  polling = true;
  try {
    while (State.jinxBusy && islandRef) {
      const items = await Bridge.jinxApprovals();
      if (items) {
        const live = new Set(items.map((i) => keyOf(i.run_id, i.request_id)));
        for (const it of items) {
          const k = keyOf(it.run_id, it.request_id);
          if (gatewaySeen.has(k)) continue;
          gatewaySeen.add(k);
          showApproval(islandRef, {
            kind: "approval", runId: it.run_id, requestId: it.request_id, command: it.command, description: it.description,
          });
        }
        const p = State.pendingApproval;
        if (p?.jinxRun) {
          const k = keyOf(p.jinxRun, p.requestId);
          if (gatewaySeen.has(k) && !live.has(k)) handle(islandRef, { kind: "resolved", requestId: p.requestId });
        }
        for (const k of [...gatewaySeen]) if (!live.has(k) && k !== keyOf(p?.jinxRun ?? "", p?.requestId ?? "")) gatewaySeen.delete(k);
      }
      await new Promise((r) => window.setTimeout(r, APPROVAL_POLL_MS));
    }
  } finally {
    polling = false;
  }
}

/** Lines of the shared thread to show. */
const THREAD_LINES = 60;
/** Don't ask the gateway again for the thread sooner than this, unless an answer just came. */
const THREAD_REFRESH_MS = 20_000;
let threadAt = 0;
let threadLoading = false;

/**
 * Shows Jinx's shared thread (Telegram, the phones, this PC) when she is reached through the gateway.
 * Never while a message is on its way: the local bubbles stay until the answer has landed in the thread.
 * `force` skips the throttle (right after an answer).
 */
export async function loadJinxThread(force = false) {
  if (threadLoading || State.jinxBusy) return;
  if (!force && Date.now() - threadAt < THREAD_REFRESH_MS) return;
  threadLoading = true;
  threadAt = Date.now();
  try {
    const lines = await Bridge.jinxHistory(THREAD_LINES);
    if (lines === null || State.jinxBusy) return; // Hermes API: the local history is the thread
    State.jinxHistory = threadMessages(lines, nextId);
    nextId += lines.length;
    State.notify();
  } catch {
    // The gateway did not answer: what is on screen stays.
  } finally {
    threadLoading = false;
  }
}

/** Called when the user sends a message to Jinx from the chat. */
export function jinxStarted() {
  streaming = null;
  State.jinxBusy = true;
  State.updateTask(JINX_ID, "thinking");
  void pollGatewayApprovals();
}

/** The send itself failed (no address, no key, unreachable): nothing is running. */
export function jinxSendFailed(message: string) {
  streaming = null;
  State.jinxBusy = false;
  say(`⚠ ${message}`);
  State.updateTask(JINX_ID, "error");
  window.setTimeout(() => State.updateTask(JINX_ID, "idle"), 5200);
}

function say(content: string): ChatMessage {
  const m: ChatMessage = { id: nextId++, role: "assistant", content };
  State.jinxHistory.push(m);
  return m;
}

function finish(island: Island, badge: "finished" | "error") {
  streaming = null;
  State.jinxBusy = false;
  State.updateTask(JINX_ID, badge === "finished" ? "finished" : "error");
  if (State.focusId !== JINX_ID || State.mode !== "expanded") State.setPillBadge(JINX_ID, badge);
  window.setTimeout(() => {
    if (!State.jinxBusy && !State.pendingApproval?.jinxRun) {
      State.updateTask(JINX_ID, "idle");
      State.setPillBadge(JINX_ID, null);
    }
  }, 5200);
  // The reply is what the user is waiting for: reveal the island if it is minimised.
  if (State.mode === "hidden") island.reveal();
  State.notify();
}

function clearApproval(island: Island) {
  State.pendingApproval = null;
  State.isPinned = false;
  island.dropPin();
  if (State.view === "approval") island.setView(State.defaultView());
  State.setPillBadge(JINX_ID, null);
}

function showApproval(island: Island, ev: Approval) {
  // One card at a time (a Claude Code request or question may hold it): keep Jinx
  // waiting rather than answering for the user, and try again shortly.
  if (State.pendingApproval || State.pendingQuestion) {
    if (!waiting.includes(ev)) waiting.push(ev);
    window.setTimeout(() => {
      const i = waiting.indexOf(ev);
      if (i < 0) return;
      waiting.splice(i, 1);
      showApproval(island, ev);
    }, 1500);
    return;
  }
  State.pendingApproval = {
    requestId: ev.requestId,
    sessionId: ev.runId,
    pillId: JINX_ID,
    tool: "Jinx",
    command: ev.command || ev.description || "…",
    jinxRun: ev.runId,
  };
  State.updateTask(JINX_ID, "approval");
  State.isPinned = true;
  Sound.play("approval");
  State.setFocus(JINX_ID);
  island.alert("approval");
  State.notify();
}

function handle(island: Island, ev: JinxEvent) {
  switch (ev.kind) {
    case "delta":
      if (!streaming) streaming = say("");
      streaming.content += ev.text;
      State.updateTask(JINX_ID, "working");
      break;

    case "tool":
      State.updateTask(JINX_ID, "working");
      State.appendStep(JINX_ID, `${ev.name}${ev.preview ? ` · ${ev.preview}` : ""}`.slice(0, 60));
      break;

    case "approval":
      showApproval(island, ev);
      break;

    case "resolved":
      // Answered somewhere else (Discord, the desktop client): drop our card.
      if (State.pendingApproval?.requestId === ev.requestId) clearApproval(island);
      for (let i = waiting.length - 1; i >= 0; i--) {
        if (waiting[i].requestId === ev.requestId) waiting.splice(i, 1);
      }
      State.updateTask(JINX_ID, "working");
      break;

    case "done":
      // Streaming may have shown the whole reply already; otherwise this is it.
      if (ev.text) {
        if (!streaming) say(ev.text);
        else if (streaming.content.trim() === "" || ev.text.length > streaming.content.length) {
          streaming.content = ev.text;
        }
      }
      if (ev.mode) State.jinxMode = ev.mode;
      {
        // The mood Jinx sent with the reply wins; without one, what her words read as.
        const emote = replyEmote(ev.mood, ev.text || streaming?.content);
        Sound.play("finish");
        finish(island, "finished");
        // Mochi wears the mood of what she said (after `finish`, so the emote is not drawn over).
        if (emote) island.emote(emote, JINX_EMOTE_S);
      }
      // Through the gateway the answer is in the shared thread now, with what the phones and
      // Telegram said meanwhile.
      if (ev.mode) void loadJinxThread(true);
      break;

    case "error":
      if (State.pendingApproval?.jinxRun) clearApproval(island);
      waiting.length = 0;
      say(`⚠ ${ev.message}`);
      Sound.play("error");
      finish(island, "error");
      break;
  }
}

/** The user's answer to a Jinx approval card. */
export function answerJinxApproval(allow: boolean) {
  const req = State.pendingApproval;
  if (!req?.jinxRun) return;
  void Bridge.jinxApprove(req.jinxRun, req.requestId, allow ? "once" : "deny").catch((err) => {
    say(`⚠ ${String(err).replace(/^Error:\s*/, "")}`);
    State.notify();
  });
}
