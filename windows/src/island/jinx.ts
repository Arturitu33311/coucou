// Jinx (Hermes) as a character: turns the `jinx` events Rust streams into chat
// bubbles, the pill's state and — when Jinx wants to run something risky — the
// same approval card Claude Code's permission requests use.

import { Bridge, onEvent } from "../core/bridge";
import { Sound } from "../core/sound";
import { JINX_ID, State, type ChatMessage } from "../core/state";
import type { Island } from "./island";

type Approval = { kind: "approval"; runId: string; requestId: string; command: string; description: string };
type JinxEvent =
  | { kind: "delta"; text: string }
  | { kind: "tool"; name: string; preview: string }
  | Approval
  | { kind: "resolved"; requestId: string }
  | { kind: "done"; text: string }
  | { kind: "error"; message: string };

let nextId = 1_000_000;
/** The assistant bubble being filled by the current run, once it has started one. */
let streaming: ChatMessage | null = null;
/** Approvals that arrived while another card held the island. */
const waiting: Approval[] = [];

export function registerJinxHandlers(island: Island) {
  void onEvent<JinxEvent>("jinx", (ev) => handle(island, ev));
}

/** Called when the user sends a message to Jinx from the chat. */
export function jinxStarted() {
  streaming = null;
  State.jinxBusy = true;
  State.updateTask(JINX_ID, "thinking");
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
      Sound.play("finish");
      finish(island, "finished");
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
