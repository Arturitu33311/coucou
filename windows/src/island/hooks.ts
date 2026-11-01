// Hook events from Claude Code and every other agent → island state.
// Port of HookServer.processEvent / processPermissionRequest from the macOS app.
// Difference from macOS: no terminal filter. On Windows the hook fires from any
// terminal (Windows Terminal, VS Code, PowerShell…) and all of them are handled.
// The relay has already mapped every agent's events and fields onto Claude
// Code's (hook/src/normalize.rs), so one handler serves them all.

import { Bridge, onEvent } from "../core/bridge";
import { buildFileDiff, fileName, makeDiffStep, toOneLine } from "../core/diff";
import { Sound } from "../core/sound";
import { State, type AskedQuestion, type QuestionItem } from "../core/state";
import { pillDefinition } from "../core/pills";
import { APPROVAL_AGENTS, agentColor, agentName, validateAgent } from "./agents";
import type { Island } from "./island";
import { parseClaudePlan, restorePlanUsage } from "../core/plan";
import { setClaudePlanUsage, storedClaudePlanUsage } from "../views/usage";
import { N_, t } from "../i18n/i18n";
import { ApprovalQueue } from "./approval-queue";
import { SessionRegistry } from "./session-registry";
import { AgentNotices, WAIT_MS, type Notice } from "./agent-notices";
import { Presence } from "../core/presence";

const CLAUDE_ID = "integration_claude";
const CURSOR_ID = "agent_cursor";

/** Clears the approval card if no decision was made before the hook gave up. */
let pendingTimeout: number | null = null;

/** Requests that came while another card was up: shown in turn (the Mac's ApprovalQueue). */
const waiting = new ApprovalQueue<HookPayload>();
/** When the request being handled again first arrived (its card gets only the time its relay has left). */
let replayingSince: number | null = null;
let queueWatch = false;

/** The sessions behind each pill: one session's Stop must not finish a pill a sibling is still working in. */
const sessions = new SessionRegistry();
/** Events that mean the session is mid-turn. */
const MID_TURN = new Set(["UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest", "SubagentStart"]);

/** How many sessions are live behind a pill, and how many are mid-turn (for the pill's "×2"). */
export function pillSessions(pillId: string): { live: number; working: number } {
  return sessions.count(pillId, Date.now());
}

/** What Jinx is told about the agents while you are away from the PC (agent-notices.ts). */
let notices = new AgentNotices();

/**
 * Away: another device has Mochi (or the panel: nobody in use). Here, the island itself shows it all. Not when the
 * device in use is a phone: it gets the session over the remote link already, and a notice would say it twice.
 */
function tellJinx(n: Notice | null) {
  if (!n || Presence.active || Presence.phone) return;
  void Bridge.jinxNotify(n.text, n.kind);
}

/** Tests start from an empty line and no sessions. */
export function resetApprovalQueueForTests(): void {
  while (waiting.size) waiting.next(Number.MAX_SAFE_INTEGER);
  sessions.clear();
  notices = new AgentNotices();
}

/** Once no card is up, the next waiting request gets its card. */
function watchQueue(island: Island) {
  if (queueWatch) return;
  queueWatch = true;
  State.subscribe(() => {
    if (!waiting.size || State.pendingApproval || State.pendingQuestion) return;
    // After the change that cleared the card has finished.
    window.setTimeout(() => {
      if (State.pendingApproval || State.pendingQuestion) return;
      const { next, expired } = waiting.next(Date.now());
      for (const w of expired) void Bridge.approvalDecline(w.requestId);
      if (!next) {
        if (expired.length) State.notify();
        return;
      }
      replayingSince = next.receivedAt;
      try {
        handleHook(island, next.payload);
      } finally {
        replayingSince = null;
      }
    }, 0);
  });
}

/** Takes the approval or question card down and gives the island back. */
export function dropPendingCard(island: Island): void {
  if (!State.pendingApproval) return;
  State.endApproval();
  island.dropPin();
  if (State.view === "approval" || State.view === "question") {
    island.setView(State.defaultView());
  }
  State.notify();
}

/** The return to idle that Stop arms, per pill, so the next turn can cancel it. */
const stopTimers = new Map<string, number>();

function cancelStopTimer(id: string): boolean {
  const timer = stopTimers.get(id);
  if (timer == null) return false;
  window.clearTimeout(timer);
  stopTimers.delete(id);
  return true;
}

/** Events after which a pending permission request of the same session is moot. */
const TURN_OVER = new Set(["Stop", "StopFailure", "UserPromptSubmit", "SessionEnd", "Interrupt"]);

interface HookPayload {
  hook_event_name?: string;
  request_id?: string;
  session_id?: string;
  cwd?: string;
  message?: string;
  /** UserPromptSubmit carries `prompt`; `message` belongs to Notification/Stop. */
  prompt?: string;
  /** Stop: the turn's final answer (Markdown) — Claude Code's, Hermes's, Codex's. */
  last_assistant_message?: string;
  tool_name?: string;
  tool_input?: Record<string, unknown>;
  /** Set by the relay when an edit was too big to forward whole (> 256 KB). */
  coucou_diff_truncated?: boolean;
  /** Optional agent tag: lowercase, digits and hyphens, ≤ 24 chars. */
  coucou_agent?: string;
  /** Hermes: where the session runs (telegram, discord…; "cli" in a terminal). */
  platform?: string;
  /** "cursor" when Claude Code runs in Cursor's terminal (set by the relay). */
  term_editor?: string;
  /** StatusLine (the plan usage relay): Claude Code's 5-hour and weekly limits. */
  rate_limits?: unknown;
  /** Set by `coucou-hook --ask`: this event is an AskUserQuestion waiting for answers. */
  coucou_kind?: string;
}

/** How long a question card may stay up — a little under the relay's 125 s. */
const QUESTION_CARD_MS = 122_000;

/**
 * `tool_input` of an AskUserQuestion call → the questions to show, or null when
 * it is not what Claude Code documents (1–4 questions, each with 2–4 options).
 * Null means the card is declined and the terminal asks as usual.
 */
export function parseQuestions(toolInput: Record<string, unknown> | undefined): QuestionItem[] | null {
  const raw = toolInput?.questions;
  if (!Array.isArray(raw) || raw.length < 1 || raw.length > 4) return null;
  const out: QuestionItem[] = [];
  for (const q of raw) {
    if (typeof q !== "object" || q === null) return null;
    const { question, header, options, multiSelect } = q as Record<string, unknown>;
    if (typeof question !== "string" || !question) return null;
    if (!Array.isArray(options) || options.length < 2 || options.length > 4) return null;
    const parsed: QuestionItem["options"] = [];
    for (const o of options) {
      const { label, description } = (o ?? {}) as Record<string, unknown>;
      if (typeof label !== "string" || !label) return null;
      parsed.push({ label, description: typeof description === "string" ? description : "" });
    }
    out.push({
      question,
      header: typeof header === "string" ? header.slice(0, 12) : "",
      options: parsed,
      multiSelect: multiSelect === true,
    });
  }
  return out;
}

const PROJECT_ALIASES: Record<string, string> = {
  "notch-buddy": "Notch Buddy",
  notchbuddy: "Notch Buddy",
  notch_buddy: "Notch Buddy",
};

function aliasProjectName(name: string): string {
  return PROJECT_ALIASES[name.toLowerCase()] ?? name;
}

function lastPathComponent(p: string): string {
  const cleaned = p.replace(/[\\/]+$/, "");
  const idx = Math.max(cleaned.lastIndexOf("\\"), cleaned.lastIndexOf("/"));
  return idx >= 0 ? cleaned.slice(idx + 1) : cleaned;
}

/**
 * localizedStep() — same labels as the macOS app, in English and shown in the
 * interface language (src/i18n) when the step is recorded.
 */
const TOOL_LABELS: Record<string, string> = {
  Bash: N_("Runs"),
  Read: N_("Reads"),
  Write: N_("Writes"),
  Edit: N_("Edits"),
  Glob: N_("Searches"),
  Grep: N_("Searches"),
  WebSearch: N_("Searches the web"),
  WebFetch: N_("Fetches"),
  TodoWrite: N_("Planning"),
  Task: N_("Agent"),
  LS: N_("Lists"),
  MultiEdit: N_("Edits"),
  NotebookEdit: N_("Notebook"),
  PowerShell: N_("Runs"),
  // Antigravity's tools (#298).
  run_command: N_("Runs"),
  view_file: N_("Reads"),
  write_to_file: N_("Writes"),
  replace_file_content: N_("Edits"),
  read_url_content: N_("Fetches"),
  search_web: N_("Searches the web"),
};

function stepLabel(tool: string, input: Record<string, unknown>): string {
  const label = TOOL_LABELS[tool] ? t(TOOL_LABELS[tool]) : tool;
  const str = (k: string) => (typeof input[k] === "string" ? (input[k] as string) : null);
  const cmd = str("command");
  if (cmd) return `${label} · ${cmd.slice(0, 40)}`;
  const path = str("path");
  if (path) return `${label} · ${lastPathComponent(path)}`;
  const file = str("file_path");
  if (file) return `${label} · ${lastPathComponent(file)}`;
  const query = str("query");
  if (query) return `${label} · ${query.slice(0, 40)}`;
  return label;
}

/**
 * What the Allow button actually authorises. Approving "Write" tells you nothing
 * — approving `Write · C:\…\.env` tells you everything, and the difference is
 * the whole point of approving from the island rather than blind.
 *
 * Ordered by how specific the field is, so an unfamiliar tool still shows
 * whatever identifying string it carries instead of falling back to its name.
 */
const APPROVAL_FIELDS = [
  "command", // Bash, PowerShell
  "file_path", // Write, Edit, MultiEdit, NotebookEdit
  "path", // Read, LS
  "url", // WebFetch
  "query", // WebSearch
  "pattern", // Glob, Grep
  "prompt", // Task
] as const;

function approvalTarget(tool: string, input: Record<string, unknown>): string {
  for (const field of APPROVAL_FIELDS) {
    const value = input[field];
    if (typeof value === "string" && value.trim()) {
      return `${tool} · ${value.trim()}`;
    }
  }
  return tool;
}

/**
 * The questions of an AskUserQuestion call, if the island can show all of them
 * as options to pick from. Anything it cannot is left to the terminal.
 */
function askedQuestions(tool: string, input: Record<string, unknown>): AskedQuestion[] | null {
  if (tool !== "AskUserQuestion" || !Array.isArray(input.questions)) return null;
  const out: AskedQuestion[] = [];
  for (const raw of input.questions as Record<string, unknown>[]) {
    const question = typeof raw?.question === "string" ? raw.question : "";
    const options = (Array.isArray(raw?.options) ? (raw.options as Record<string, unknown>[]) : [])
      .filter((o) => typeof o?.label === "string" && o.label)
      .map((o) => ({
        label: o.label as string,
        description: typeof o.description === "string" ? o.description : "",
      }));
    // A question cut short by the relay would be answered under the wrong text.
    if (!question || question.endsWith("…") || options.length < 2) return null;
    out.push({ question, options, multiSelect: raw.multiSelect === true });
  }
  return out.length > 0 ? out : null;
}

/** The Claude Code session's pill, named after its project for the session. */
function upsert(id: string, projectName: string, cwd: string, sessionId: string) {
  const t = State.upsertWorkspacePill(id, projectName, cwd);
  if (t && sessionId) t.sessionId = sessionId;
}

/** The session is over: the pill goes back as it was, or away if it was only there for it. */
function clearSession(id: string) {
  const t = State.tasks.find((x) => x.id === id);
  if (!t) return;
  if (!State.isKept(id)) {
    State.removeTask(id);
    return;
  }
  t.state = "idle";
  t.steps = [];
  t.stepIndex = 0;
  delete t.stepSeq;
  t.subagentsCount = 0;
  t.name = pillDefinition(id)?.name ?? t.name;
  t.pillBadge = null;
  t.sessionId = null;
  t.finalLine = null;
}

/** The final message stays on the card until the next turn starts. */
function clearFinalLine(id: string) {
  const t = State.tasks.find((x) => x.id === id);
  if (t) t.finalLine = null;
}

/**
 * PostToolUse of Edit / MultiEdit / Write → a diff stored for the pill and a
 * ticker step with its +N −M counts. Nothing is kept for a pill that does not
 * exist, so a stray event cannot grow memory. An edit the relay had to cut
 * would give wrong counts: the PreToolUse step ("Edits · file") stands alone.
 */
function recordDiff(agentId: string, payload: HookPayload) {
  if (payload.coucou_diff_truncated) return;
  if (!State.tasks.some((t) => t.id === agentId)) return;
  const diff = buildFileDiff(payload.tool_name ?? "", payload.tool_input ?? {});
  if (!diff) return;
  const id = State.appendSessionDiff(agentId, diff);
  State.appendStep(agentId, makeDiffStep(fileName(diff.path), diff.added, diff.removed, id));
}

/** A request was settled somewhere else (the phone, the clock): its card must not stay up. */
function resolvedElsewhere(island: Island, requestId: string) {
  if (waiting.remove(requestId)) State.notify();
  const approval = State.pendingApproval;
  if (approval && approval.requestId === requestId) {
    State.pendingApproval = null;
    State.isPinned = false;
    island.dropPin();
    State.updateTask(CLAUDE_ID, "working");
    State.setPillBadge(CLAUDE_ID, null);
    if (State.view === "approval") island.setView(State.defaultView());
  }
  const question = State.pendingQuestion;
  if (question && question.requestId === requestId) {
    State.pendingQuestion = null;
    State.isPinned = false;
    island.dropPin();
    State.updateTask(CLAUDE_ID, "working");
    State.setPillBadge(CLAUDE_ID, null);
    if (State.view === "question") island.setView(State.defaultView());
  }
  State.notify();
}

export function registerHookHandlers(island: Island) {
  void onEvent<{ request_id: string }>("hook-resolved", (e) => resolvedElsewhere(island, e.request_id));
  // The last plan numbers seen survive a restart, as on the Mac.
  State.planUsage ??= restorePlanUsage(storedClaudePlanUsage());
  void onEvent<HookPayload>("hook", (payload) => handleHook(island, payload));
  watchQueue(island);
}

function handleHook(island: Island, payload: HookPayload) {
  // Account-wide numbers from the status line relay, not part of any session:
  // keep the latest, nothing else (no reveal, no sound), paused or not.
  if (payload.hook_event_name === "StatusLine") {
    const usage = parseClaudePlan(payload.rate_limits);
    if (usage) setClaudePlanUsage(usage);
    return;
  }

  if (State.paused) {
    // Silence here used to cost Claude Code nearly two minutes: the relay waited
    // for a decision from an island that had already decided not to look. Say so,
    // and the terminal takes the question immediately.
    if (payload.request_id) void Bridge.approvalDecline(payload.request_id);
    return;
  }

  const name = payload.hook_event_name ?? "";
  const cwd = payload.cwd ?? "";
  const raw = lastPathComponent(cwd);
  const projectName = aliasProjectName(raw || "Session");

  // Route to the right pill. Valid coucou_agent → dynamic "agent_<name>" pill.
  // "claude" is reserved; absent or invalid → Claude Code's own pill: Cursor's
  // when it runs in Cursor's terminal (Mac #120), VS Code's otherwise.
  const validAgent = validateAgent(payload.coucou_agent);
  const workspaceId = payload.term_editor === "cursor" ? CURSOR_ID : CLAUDE_ID;
  const agentId = validAgent ? `agent_${validAgent}` : workspaceId;
  const isExternalAgent = validAgent !== null;
  const sessionId = payload.session_id ?? "";
  /** How Jinx names it: "Claude Code", "Antigravity", "OpenCode"… */
  const who = validAgent ? agentName(validAgent) : workspaceId === CURSOR_ID ? "Cursor" : "Claude Code";

  /** Alerts force the island open; work events only reveal the compact island. */
  const surface = (view: Parameters<Island["alert"]>[0], isAlert: boolean) => {
    if (State.mode === "expanded") {
      if (isAlert) island.setView(view);
    } else if (isAlert) {
      island.alert(view);
    } else if (State.mode === "hidden") {
      island.reveal();
    }
  };

  /** Ensure the agent pill exists (no-op for Claude Code). */
  const ensurePill = () => {
    if (isExternalAgent) {
      State.upsertExternalAgent(agentId, agentName(validAgent!), agentColor(validAgent!));
      const t = State.tasks.find((x) => x.id === agentId);
      if (t && cwd) t.sessionCwd = cwd;
      if (t && sessionId) t.sessionId = sessionId;
    } else {
      upsert(agentId, projectName, cwd, sessionId);
    }
  };

  /**
   * Called where a handler is about to replace `finished` with a newer state:
   * the timer Stop armed would otherwise put the pill back to idle over it. The
   * badge that timer was going to clear goes now.
   */
  const supersedeStop = () => {
    if (cancelStopTimer(agentId)) State.setPillBadge(agentId, null);
  };

  // The turn that asked for a permission is over — answered in the terminal,
  // interrupted, or a new prompt — so the card would be lying. It goes, and the
  // relay is released without a decision. Same rule as the Mac.
  const pending = State.pendingApproval;
  if (
    pending &&
    TURN_OVER.has(name) &&
    pending.pillId === agentId &&
    pending.sessionId === (payload.session_id ?? "")
  ) {
    if (pendingTimeout != null) window.clearTimeout(pendingTimeout);
    pendingTimeout = null;
    if (pending.requestId) void Bridge.approvalDecline(pending.requestId);
    dropPendingCard(island);
  }

  // Its waiting requests are moot too.
  if (TURN_OVER.has(name)) {
    for (const w of waiting.dropSession(agentId, payload.session_id ?? "")) void Bridge.approvalDecline(w.requestId);
  }

  // Read after the card above is dropped: its pill may have handed the front
  // back to the pill you were on.
  const focused = State.focusId === agentId;

  // `coucou-hook --ask`: an AskUserQuestion, and the relay is holding Claude Code
  // until we answer. Every path out of here is an answer, a decline or the timeout.
  if (payload.coucou_kind === "ask_user_question") {
    const requestId = payload.request_id ?? "";
    const questions = isExternalAgent ? null : parseQuestions(payload.tool_input);
    // One card at a time, and only one we know how to draw. Anything else goes
    // straight back to the terminal.
    const busy =
      State.pendingApproval !== null ||
      (State.pendingQuestion !== null && State.pendingQuestion.requestId !== requestId);
    if (!questions || busy) {
      if (requestId) void Bridge.approvalDecline(requestId);
      return;
    }
    upsert(agentId, projectName, cwd, payload.session_id ?? "");
    State.pendingQuestion = {
      requestId,
      sessionId: payload.session_id ?? "",
      questions,
      index: 0,
      answers: [],
    };
    if (requestId) void Bridge.approvalAck(requestId);
    State.updateTask(CLAUDE_ID, "question");
    State.isPinned = true;
    Sound.play("question");
    // Unlike a permission, a question holds the whole turn: the terminal shows
    // nothing until we answer. So it always takes the view, whoever had it.
    if (State.focusBeforeApproval == null) State.focusBeforeApproval = State.focusId;
    State.setFocus(CLAUDE_ID);
    island.alert("question");
    window.setTimeout(() => {
      // Only this card: a later question must not be cleared by an earlier timer.
      if (State.pendingQuestion?.requestId !== requestId) return;
      State.pendingQuestion = null;
      State.isPinned = false;
      island.dropPin();
      State.updateTask(CLAUDE_ID, "working");
      const previous = State.focusBeforeApproval;
      State.focusBeforeApproval = null;
      if (previous && State.tasks.some((t) => t.id === previous)) {
        State.setFocus(previous);
      }
      if (State.view === "question") island.setView(State.defaultView());
      State.notify();
    }, QUESTION_CARD_MS);
    State.notify();
    return;
  }

  // Who is live behind this pill (Stop and SessionEnd below ask it before touching the pill).
  if (sessionId && name !== "Stop" && name !== "SessionEnd" && name !== "StopFailure" && name !== "Interrupt") {
    sessions.note(agentId, sessionId, MID_TURN.has(name), Date.now());
  }

  switch (name) {
    case "SessionStart":
      ensurePill();
      clearFinalLine(agentId);
      // Hermes through its gateway says where the session comes from.
      if (validAgent === "hermes" && payload.platform && payload.platform !== "cli") {
        State.appendStep(agentId, payload.platform.charAt(0).toUpperCase() + payload.platform.slice(1));
      }
      surface("overview", false);
      Sound.play("work");
      break;

    case "UserPromptSubmit": {
      notices.begin(agentId, sessionId, Date.now());
      ensurePill();
      supersedeStop();
      clearFinalLine(agentId);
      State.updateTask(agentId, "thinking");
      // The field is `prompt`; reading `message` meant this step was always blank.
      const asked = payload.prompt ?? payload.message;
      if (asked) State.appendStep(agentId, asked.slice(0, 60));
      surface("overview", false);
      break;
    }

    case "PreToolUse": {
      // (OpenCode and Antigravity's turns may start with a tool, not a prompt.)
      notices.begin(agentId, sessionId, Date.now());
      ensurePill();
      supersedeStop();
      clearFinalLine(agentId);
      State.updateTask(agentId, "working");
      const tool = payload.tool_name ?? "Tool";
      // The question itself arrives through the dedicated `--ask` hook, above. This
      // is the general hook seeing the same call: it is not "work", so no step.
      if (tool === "AskUserQuestion") {
        State.updateTask(agentId, "question");
        surface("overview", false);
        break;
      }
      State.updateTask(agentId, "working");
      State.appendStep(agentId, stepLabel(tool, payload.tool_input ?? {}));
      surface("overview", false);
      break;
    }

    case "PostToolUse":
      supersedeStop();
      // The question was answered in the terminal: the card would be lying.
      if (
        payload.tool_name === "AskUserQuestion" &&
        State.pendingApproval?.questions &&
        State.pendingApproval.sessionId === (payload.session_id ?? "")
      ) {
        if (pendingTimeout != null) window.clearTimeout(pendingTimeout);
        pendingTimeout = null;
        dropPendingCard(island);
      }
      State.updateTask(agentId, "working");
      recordDiff(agentId, payload);
      break;

    case "PostToolUseFailure":
      supersedeStop();
      State.updateTask(agentId, "working");
      State.appendStep(agentId, t("⚠ failed"));
      break;

    case "Notification": {
      const message = payload.message ?? "";
      const lower = message.toLowerCase();
      if (lower.includes("rate limit") || lower.includes("limite d")) {
        supersedeStop();
        State.updateTask(agentId, "ratelimit");
        Sound.play("rate");
      } else if (message.endsWith("?")) {
        supersedeStop();
        State.updateTask(agentId, "question");
        State.appendStep(agentId, message);
        if (State.focusId !== agentId) {
          const name = isExternalAgent ? agentName(validAgent!) : "Claude Code";
          State.noteMessage = `${name}: ${message.slice(0, 50)}`;
          Sound.play("question");
          island.mochi.triggerEmote("surprised", 2.0);
          island.alert("note");
        }
      }
      break;
    }

    case "Stop": {
      tellJinx(notices.finished(agentId, sessionId, Date.now(), who, projectName));
      // A sibling session is still mid-turn behind this pill: only this session finished. Its last words go in
      // the steps; the pill keeps working.
      if (sessionId && sessions.endTurn(agentId, sessionId, Date.now()) === "sessionOnly") {
        const last = toOneLine(payload.last_assistant_message ?? payload.message ?? "");
        if (last) State.appendStep(agentId, last);
        break;
      }
      State.updateTask(agentId, "finished");
      // Claude Code puts the turn's answer in the Stop payload itself, so there is
      // no transcript to read (the relay does not even forward its path). Other
      // agents report their last words the same way (Hermes, Codex) or as
      // `message`.
      const finalText = toOneLine(payload.last_assistant_message ?? payload.message ?? "");
      if (finalText) {
        State.appendStep(agentId, finalText);
        const t = State.tasks.find((x) => x.id === agentId);
        if (t) t.finalLine = finalText;
      }
      Sound.play("finish");
      // A card waiting for an answer is never covered by another alert.
      if (focused && !State.pendingApproval) surface("finished", true);
      else {
        State.setPillBadge(agentId, "finished");
        island.mochi.triggerEmote("happy", 2.0);
      }
      cancelStopTimer(agentId);
      stopTimers.set(
        agentId,
        window.setTimeout(() => {
          stopTimers.delete(agentId);
          // An agent's pill goes away after its turn unless it is declared (or the main pill): then it stays,
          // as on the Mac, so Antigravity and OpenCode can sit next to Jinx like Claude Code.
          if (isExternalAgent && !State.isKept(agentId)) {
            State.removeTask(agentId);
          } else {
            State.updateTask(agentId, "idle");
            State.setPillBadge(agentId, null);
          }
        }, 5200),
      );
      break;
    }

    case "Interrupt":
      // Codex: the user stopped the turn. Back to idle, nothing to celebrate (unless a sibling still works).
      if (sessionId && sessions.endTurn(agentId, sessionId, Date.now()) === "sessionOnly") break;
      supersedeStop();
      State.updateTask(agentId, "idle");
      State.setPillBadge(agentId, null);
      break;

    case "StopFailure":
      tellJinx(notices.failed(agentId, sessionId, Date.now(), who, projectName));
      // The error shows whatever the siblings do; this session's turn is over.
      if (sessionId) sessions.endTurn(agentId, sessionId, Date.now());
      supersedeStop();
      State.updateTask(agentId, "error");
      Sound.play("error");
      if (focused && !State.pendingApproval) surface("error", true);
      else State.setPillBadge(agentId, "error");
      break;

    case "SessionEnd":
      notices.forget(agentId, sessionId);
      // Another session is still live behind this pill: only this one ended.
      if (sessionId && sessions.endSession(agentId, sessionId, Date.now()) === "sessionOnly") break;
      // Nothing left for the timer to do, and it must not outlive the session: a
      // pill recreated within 5.2 s would be removed by it.
      cancelStopTimer(agentId);
      State.clearSessionDiffs(agentId);
      if (isExternalAgent && !State.isKept(agentId)) {
        State.removeTask(agentId);
      } else {
        State.updateTask(agentId, "idle");
        clearSession(agentId);
      }
      break;

    case "SubagentStart": {
      const task = State.tasks.find((x) => x.id === agentId);
      if (task) task.subagentsCount = (task.subagentsCount ?? 0) + 1;
      State.appendStep(agentId, t("+ subagent"));
      State.notify();
      break;
    }

    case "SubagentStop": {
      const task = State.tasks.find((x) => x.id === agentId);
      if (task) task.subagentsCount = Math.max(0, (task.subagentsCount ?? 1) - 1);
      State.appendStep(agentId, t("• subagent done"));
      State.notify();
      break;
    }

    case "PermissionRequest": {
      // Claude Code raises a PermissionRequest for the AskUserQuestion tool itself
      // once the `--ask` hook has given the question back ("Reply in terminal", a
      // timeout, a card already up). An Allow button here would approve the tool
      // with no answer attached and hold the terminal, so hand it straight back:
      // the terminal's own picker takes the question, as on macOS. (Claude Code only: another
      // agent's tool of that name is an ordinary permission.)
      if (payload.tool_name === "AskUserQuestion" && !isExternalAgent) {
        if (payload.request_id) void Bridge.approvalDecline(payload.request_id);
        break;
      }

      // Only Claude Code and the agents the relay can answer for (Codex, Copilot
      // CLI, Muse Code — same as the Mac) get a card. Anyone else's request is
      // declined at once, so the agent asks in its own terminal.
      if (isExternalAgent && !APPROVAL_AGENTS.has(validAgent!)) {
        if (payload.request_id) void Bridge.approvalDecline(payload.request_id);
        const name = agentName(validAgent!);
        State.noteMessage = `${name} espera tu respuesta en su terminal`;
        Sound.play("question");
        island.mochi.triggerEmote("surprised", 2.2);
        island.alert("note");
        break;
      }

      const requestId = payload.request_id ?? "";
      // Still unanswered after a while, with you away: Jinx tells you where you are.
      if (requestId && replayingSince == null) {
        const tool = payload.tool_name ?? "Tool";
        window.setTimeout(() => {
          const still = State.pendingApproval?.requestId === requestId || waiting.position(requestId) > 0;
          if (still) tellJinx(notices.waiting(agentId, sessionId, Date.now(), who, projectName, tool));
        }, WAIT_MS);
      }
      // One card, one request: a second one never quietly replaces the first. It waits in line (up to five in
      // all, as on the Mac) and gets its card when this one is answered; past that, back to its terminal.
      if (State.pendingQuestion || (State.pendingApproval && State.pendingApproval.requestId !== requestId)) {
        const queued =
          requestId !== "" &&
          waiting.push({ requestId, pillId: agentId, sessionId, receivedAt: Date.now(), payload });
        if (!queued) {
          if (requestId) void Bridge.approvalDecline(requestId);
          break;
        }
        // The relay keeps waiting for it; its pill says it is asking.
        void Bridge.approvalAck(requestId);
        ensurePill();
        supersedeStop();
        State.updateTask(agentId, "approval");
        if (State.focusId !== agentId) State.setPillBadge(agentId, "approval");
        Sound.play("approval");
        break;
      }
      ensurePill();
      supersedeStop();
      if (pendingTimeout != null) window.clearTimeout(pendingTimeout);
      const tool = payload.tool_name ?? "Tool";
      const input = payload.tool_input ?? {};
      // Claude Code asking a question is not a permission to grant: the island
      // shows the options and sends back the one that was picked. Only Claude
      // Code asks questions this way.
      const questions = isExternalAgent ? null : askedQuestions(tool, input);
      const view = questions ? "question" : "approval";
      // The card always comes up, even over another pill or an island that is
      // already open: its pill comes to the front, and the one you were on
      // comes back once you answer (Mac #117, #120).
      State.beginApproval({
        requestId,
        sessionId,
        pillId: agentId,
        tool,
        command: approvalTarget(tool, input),
        ...(questions ? { questions } : {}),
      });
      // The relay's short ack window closes in 800 ms; everything below this
      // line is synchronous, so the card really is up by the time it lands.
      if (requestId) void Bridge.approvalAck(requestId);
      State.updateTask(agentId, view);
      Sound.play(view);
      // Any agent's card (Claude Code, Codex, Copilot CLI, Muse Code) comes up
      // the same way: beginApproval brought its pill to the front.
      island.alert(view);
      // Coucou answers within 108 s or not at all; after that the terminal has
      // taken over and the card would be lying.
      // (A request that waited in line gets only the time its relay has left.)
      pendingTimeout = window.setTimeout(
        () => {
          pendingTimeout = null;
          dropPendingCard(island);
        },
        replayingSince == null ? 110_000 : ApprovalQueue.cardMs(replayingSince, Date.now()),
      );
      break;
    }

    default:
      break;
  }
  State.notify();
}
