// Claude Code sessions in the chat: which ones are running and in what state, and
// what is being said in the selected one. Nothing runs unless the chat is on screen
// (the island costs nothing while it is closed); the Rust side asks Claude Code itself.

import { Bridge } from "../core/bridge";
import { State, type AgentSession, type ChatMessage } from "../core/state";

const POLL_MS = 1200;
let running = false;
let tick = 0;
let nextId = 2_000_000;
/** The first list has arrived: choosing a target before that would see no agents. */
let listLoaded = false;

const wanted = () => State.view === "prompt" && State.mode === "expanded";
const sleep = (ms: number) => new Promise<void>((r) => window.setTimeout(r, ms));

export function selectedAgent(): AgentSession | undefined {
  return State.agents.find((a) => a.id === State.agentId);
}

/** The messages of the selected agent (empty until its transcript has been read). */
export function agentThread(): ChatMessage[] {
  const a = selectedAgent();
  return a ? State.agentThreads[a.sessionId]?.msgs ?? [] : [];
}

/** Whether the first list of running agents has arrived. */
export const agentsLoaded = () => listLoaded;

/** The agent to open on: one that is waiting for you, else one you can message, else any. */
function bestAgent(list: AgentSession[]): AgentSession | undefined {
  return list.find((a) => a.state === "blocked") ?? list.find((a) => a.state !== "working") ?? list[0];
}

/** Starts reading while the chat is open; a no-op when it already is. */
export function startAgentSync() {
  if (running || !wanted()) return;
  running = true;
  if (State.mochiApi === null) {
    void Bridge.secretPresent("anthropic-api-key").then((present) => {
      State.mochiApi = !!present;
      pickFirstTarget();
      State.notify();
    });
  }
  void loop();
}

/** Read now rather than at the next beat (right after sending a message). */
export async function syncAgentsNow() {
  await refreshList();
  await refreshThread();
}

async function loop() {
  try {
    while (wanted()) {
      if (tick % 5 === 0) await refreshLimits();
      if (tick++ % 2 === 0) await refreshList();
      await refreshThread();
      await sleep(POLL_MS);
    }
  } finally {
    running = false;
  }
}

/** Without an API key Mochi's own chat has nothing to talk to: open on an agent. */
export function pickFirstTarget() {
  if (State.chatPicked || State.mochiApi === null || !listLoaded) return;
  State.chatPicked = true;
  if (State.chatTarget === "mochi" && State.mochiApi === false) {
    const best = bestAgent(State.agents);
    State.chatTarget = best ? "agent" : "new";
    State.agentId = best?.id ?? null;
  }
}

async function refreshLimits() {
  const limits = (await Bridge.rateLimits()) ?? null;
  if (JSON.stringify(limits) !== JSON.stringify(State.limits)) {
    State.limits = limits;
    State.notify();
  }
}

async function refreshList() {
  let list: AgentSession[];
  try {
    list = await Bridge.agentsList();
  } catch (err) {
    const message = String(err).replace(/^Error:\s*/, "");
    if (State.agentsError !== message) {
      State.agentsError = message;
      State.notify();
    }
    return;
  }
  const changed = JSON.stringify(list) !== JSON.stringify(State.agents) || State.agentsError !== null || !listLoaded;
  listLoaded = true;
  State.agents = list;
  State.agentsError = null;
  // The selected session ended: move to another one rather than show a dead thread.
  if (State.chatTarget === "agent" && !list.some((a) => a.id === State.agentId)) {
    State.agentId = bestAgent(list)?.id ?? null;
    if (!State.agentId) State.chatTarget = "new";
  }
  pickFirstTarget();
  if (changed) State.notify();
}

async function refreshThread() {
  const agent = selectedAgent();
  if (State.chatTarget !== "agent" || !agent?.sessionId) return;
  const thread = (State.agentThreads[agent.sessionId] ??= { offset: 0, msgs: [] });
  try {
    const got = await Bridge.agentMessages(agent.sessionId, thread.offset);
    if (got.messages.length === 0 && got.offset === thread.offset) return;
    thread.offset = got.offset;
    for (const m of got.messages) applyMessage(thread.msgs, m);
    if (thread.msgs.length > 300) thread.msgs.splice(0, thread.msgs.length - 300);
    State.notify();
  } catch {
    // A session without a transcript yet (just started): the next beat tries again.
  }
}

/** One transcript record into a thread; the agent's queue comes and goes (see agents.rs). */
function applyMessage(msgs: ChatMessage[], m: { role: string; text: string }) {
  if (m.role === "queued") {
    msgs.push({ id: nextId++, role: "queued", content: m.text });
  } else if (m.role === "dequeued") {
    // It ran (the transcript then records it as a normal message) or was taken back.
    const i = m.text ? msgs.findIndex((x) => x.role === "queued" && x.content === m.text) : msgs.findIndex((x) => x.role === "queued");
    if (i >= 0) {
      if (m.text) msgs[i].role = "user"; // absorbed into the running turn: delivered
      else msgs.splice(i, 1);
    }
  } else if (m.role === "queue-clear") {
    for (let i = msgs.length - 1; i >= 0; i--) if (msgs[i].role === "queued") msgs.splice(i, 1);
  } else {
    msgs.push({ id: nextId++, role: m.role as ChatMessage["role"], content: m.text });
  }
}

/** A just-started agent takes a moment to be listed; the chat moves onto it once it is. */
export async function adoptAgent(id: string) {
  for (let i = 0; i < 10; i++) {
    await refreshList();
    if (State.agents.some((a) => a.id === id)) {
      State.chatTarget = "agent";
      State.agentId = id;
      State.notify();
      await refreshThread();
      return;
    }
    await sleep(500);
  }
  throw new Error("The agent was started but has not shown up in the list yet");
}

/** A line in the selected agent's thread that did not come from its transcript. */
export function agentNote(text: string) {
  const agent = selectedAgent();
  if (!agent) return;
  const thread = (State.agentThreads[agent.sessionId] ??= { offset: 0, msgs: [] });
  thread.msgs.push({ id: nextId++, role: "assistant", content: text });
  State.notify();
}
