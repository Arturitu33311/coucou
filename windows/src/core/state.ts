// App state — mirror of AppState.swift (the parts the island needs).

import type { BotEmoteName, BotStateName, IslandMode, IslandViewName } from "./layout";
import type { EyeShape } from "../mochi/engine";

export type AgentSource = "claudeCode" | "n8n" | "agent";
export type PillBadge = "approval" | "finished" | "error";

export interface AgentTask {
  id: string;
  name: string;
  color: string;
  state: BotStateName;
  stepIndex: number;
  steps: string[];
  source: AgentSource;
  isIntegration: boolean;
  emote?: BotEmoteName | null;
  miniEye?: EyeShape | null;
  pillBadge?: PillBadge | null;
  sessionCwd?: string | null;
}

export interface ApprovalInfo {
  requestId: string;
  sessionId: string;
  tool: string;
  command: string;
  /** Set when Jinx (Hermes) is the one asking: the run to answer. */
  jinxRun?: string;
}

/** One multiple-choice question of an AskUserQuestion call. */
export interface QuestionItem {
  /** The full text. Also the key of its answer, so it is never altered. */
  question: string;
  /** ≤ 12 characters; a small label above the question. */
  header: string;
  options: { label: string; description: string }[];
  multiSelect: boolean;
}

/** An AskUserQuestion call waiting on the island, one question at a time. */
export interface QuestionInfo {
  requestId: string;
  sessionId: string;
  questions: QuestionItem[];
  /** The question on screen. */
  index: number;
  /** Answers given so far, one list of labels per question already answered. */
  answers: string[][];
}

export interface ChatMessage {
  id: number;
  /** "tool" is a line of what an agent is doing (Edit · src/a.ts), not talk. */
  role: "user" | "assistant" | "tool" | "queued";
  content: string;
}

export interface LimitWindow {
  pct: number;
  /** Unix seconds. */
  resetsAt: number | null;
}

/** A running Claude Code session, as `claude agents` lists it. */
export interface AgentSession {
  id: string;
  name: string;
  cwd: string;
  kind: string;
  status: string;
  /** working | done | blocked */
  state: string;
  sessionId: string;
  waitingFor?: string | null;
}

export type PromptContext =
  | { kind: "window"; appName: string; title: string; url?: string }
  | { kind: "file"; name: string; path?: string };

export interface ResultItem {
  label: string;
  detail: string;
  url?: string;
}

export interface SearchResult {
  title: string;
  items: ResultItem[];
  note?: string;
}

const task = (
  id: string, name: string, color: string, source: AgentSource,
): AgentTask => ({
  id, name, color, state: "idle", stepIndex: 0, steps: [], source, isIntegration: true,
});

/** AgentTask.integrationAgents — same ids, names and colours as macOS. */
export const INTEGRATION_AGENTS: AgentTask[] = [
  task("integration_claude", "Claude Code", "#D97757", "claudeCode"),
  task("integration_resend", "Resend", "#22C55E", "n8n"),
  task("integration_n8n", "n8n", "#F29B38", "n8n"),
  task("integration_vercel", "Vercel", "#7C5CFF", "n8n"),
  task("integration_github", "GitHub", "#F4505E", "n8n"),
  task("integration_notion", "Notion", "#8C8C8C", "n8n"),
  task("integration_calcom", "Cal.com", "#C9956A", "n8n"),
  task("integration_stripe", "Stripe", "#0570DE", "n8n"),
  // Same id and colour as the macOS "Apple Music" pill; here it follows whatever
  // MPRIS player is running (Linux).
  task("integration_music", "Music", "#FA2D48", "n8n"),
  // Jinx (the Hermes agent) as a character: chat, replies and permission requests.
  task("integration_jinx", "Jinx", "#39FF14", "n8n"),
];

export const TOGGLEABLE_INTEGRATION_IDS = [
  "integration_resend", "integration_n8n", "integration_vercel", "integration_github",
  "integration_notion", "integration_calcom", "integration_stripe", "integration_music", "integration_jinx",
];

export const MUSIC_ID = "integration_music";
export const JINX_ID = "integration_jinx";

/** What the player reports (see music.rs). */
export interface MusicTrack {
  player: string;
  identity: string;
  status: "Playing" | "Paused" | "Stopped" | string;
  title: string;
  artists: string[];
  album: string;
  lengthMs: number;
  artUrl: string;
  /** Where the track was at `positionAt` (epoch ms); the island extrapolates. */
  positionMs: number;
  positionAt: number;
  canSeek: boolean;
  canNext: boolean;
  canPrevious: boolean;
  /** The player's own volume, 0–1, or -1 when it has none. */
  volume: number;
}

export interface LyricLine {
  t: number;
  text: string;
}

/** The track on screen plus everything the island derived from it. */
export interface MusicInfo extends MusicTrack {
  /** Same song ⇔ same key; position and play/pause don't change it. */
  key: string;
  /** Album art as a data: URL, once fetched. */
  art: string | null;
  /** Average colour of the art, and a lighter shade of it readable on dark. */
  rgb: [number, number, number] | null;
  accent: string;
  lyrics: LyricLine[] | null;
  /** off = online fetching is switched off in Settings. */
  lyricsState: "off" | "loading" | "none" | "ready";
}

/** What an integration poller last reported. */
export interface IntegrationInfo {
  data: Record<string, unknown>;
  error: string | null;
  loaded: boolean;
  configured: boolean;
}

export interface Settings {
  soundEnabled: boolean;
  soundVolume: number;
  autoCloseInterval: number;
  absenceInterval: number;
  activeIntegrations: string[];
  screen: "primary" | "cursor";
  autostart: boolean;
  hooksInstalled: boolean;
  /** Claude model used by the chat. */
  model: string;
  /** Music pill: fetch lyrics (lrclib.net) and album art given as a web URL. */
  musicOnline: boolean;
  musicVisualizer: "off" | "wave" | "beat" | "realtime";
  /** Script to prefer when lrclib has several versions of a song. */
  musicLyricsLanguage: "any" | "original" | "latin";
  /** Keep the pill (and the minimised island) on the last track after the player closes. */
  musicKeep: boolean;
  /** What the wheel does over the minimised pill. */
  musicScroll: "track" | "volume";
  /** The minimised island opens when the pointer rests on it (no click) and folds back fast. */
  openOnHover: boolean;
  /** Seconds the pointer rests on the minimised island before it opens (open on hover). */
  hoverOpenDelay: number;
  /** Island position across the top: px from the centre, positive = right. */
  islandOffsetX: number;
}

export const DEFAULT_SETTINGS: Settings = {
  soundEnabled: true,
  soundVolume: 0.12,
  autoCloseInterval: 15,
  absenceInterval: 180,
  activeIntegrations: [
    "integration_resend", "integration_n8n", "integration_vercel", "integration_github",
  ],
  screen: "primary",
  autostart: false,
  hooksInstalled: false,
  model: "claude-opus-5",
  musicOnline: false,
  musicVisualizer: "realtime",
  musicLyricsLanguage: "any",
  musicKeep: true,
  musicScroll: "track",
  openOnHover: false,
  hoverOpenDelay: 0.9,
  islandOffsetX: 0,
};

type Listener = () => void;

class AppState {
  mode: IslandMode = "hidden";
  view: IslandViewName = "overview";

  tasks: AgentTask[] = [];
  focusId: string | null = null;

  stateOverride: BotStateName | null = null;

  /** Cursor in logical screen pixels, origin top-left (like AppState.mousePosition). */
  mouse = { x: 0, y: 0 };
  /** Cursor relative to the island's top-left corner. */
  mouseInIsland = { x: 0, y: 0 };

  isPinned = false;
  paused = false;

  uploadProgress = 0;
  uploadDuration = 2.4;
  fileDragOver = false;

  promptContext: PromptContext | null = null;
  droppedFile: { name: string; path: string } | null = null;
  noteMessage: string | null = null;
  searchResult: SearchResult | null = null;
  chatHistory: ChatMessage[] = [];
  /** Who the chat talks to: Mochi (Claude's API) or Jinx (Hermes). Each keeps its own thread. */
  chatTarget: "mochi" | "jinx" | "agent" | "new" = "mochi";
  /** The running Claude Code sessions, the selected one, and what has been read of each. */
  agents: AgentSession[] = [];
  agentId: string | null = null;
  agentThreads: Record<string, { offset: number; msgs: ChatMessage[] }> = {};
  agentsError: string | null = null;
  /** The file already handed to an agent (it can read it from the path). */
  agentFilePath: string | null = null;
  /** Is there an Anthropic API key (Mochi's own chat)? null until asked. */
  mochiApi: boolean | null = null;
  /** Claude Code's usage limits (percent used), when known. */
  limits: { five: LimitWindow | null; seven: LimitWindow | null; ts: number } | null = null;
  /** The chat has chosen its first target for this opening. */
  chatPicked = false;
  jinxHistory: ChatMessage[] = [];
  /** The dropped file already sent to Jinx (she keeps it for the whole session). */
  jinxFilePath: string | null = null;
  /** A Jinx run is under way (from sending until its last event). */
  jinxBusy = false;
  pendingApproval: ApprovalInfo | null = null;
  pendingQuestion: QuestionInfo | null = null;
  /** What the Music pill is showing; null when no player has a track. */
  music: MusicInfo | null = null;

  integrations: Record<string, IntegrationInfo> = {};

  lastActivity = performance.now();

  settings: Settings = { ...DEFAULT_SETTINGS };

  private listeners = new Set<Listener>();

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** Marks the UI dirty; the island re-renders on the next frame. */
  notify() {
    for (const fn of this.listeners) fn();
  }

  get focusTask(): AgentTask | null {
    return this.tasks.find((t) => t.id === this.focusId) ?? this.tasks[0] ?? null;
  }

  get effectiveState(): BotStateName {
    return this.stateOverride ?? this.focusTask?.state ?? "idle";
  }

  get otherTasks(): AgentTask[] {
    return this.tasks.filter((t) => t.id !== this.focusId);
  }

  setFocus(id: string) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    this.focusId = id;
    t.pillBadge = null;
    this.notify();
  }

  updateTask(id: string, state: BotStateName) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    t.state = state;
    this.notify();
  }

  appendStep(id: string, step: string) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    t.steps.push(step);
    if (t.steps.length > 20) t.steps.shift();
    t.stepIndex = t.steps.length - 1;
    this.notify();
  }

  setPillBadge(id: string, badge: PillBadge | null) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    t.pillBadge = badge;
    this.notify();
  }

  /** loadIntegrationTasks() — Claude Code always on, the rest opt-in (max 4). */
  loadIntegrationTasks() {
    for (const proto of INTEGRATION_AGENTS) {
      const shouldLoad =
        proto.id === "integration_claude" || this.settings.activeIntegrations.includes(proto.id);
      const idx = this.tasks.findIndex((t) => t.id === proto.id);
      if (shouldLoad && idx < 0) this.tasks.push({ ...proto, steps: [] });
      if (!shouldLoad && idx >= 0) this.tasks.splice(idx, 1);
    }
    // Order: integration_claude first, then agent_* pills (visible in slice(0,4)),
    // then other integrations in declaration order.
    const order = INTEGRATION_AGENTS.map((t) => t.id);
    this.tasks.sort((a, b) => {
      const isAgentA = a.id.startsWith("agent_");
      const isAgentB = b.id.startsWith("agent_");
      // integration_claude always first
      if (a.id === "integration_claude") return -1;
      if (b.id === "integration_claude") return 1;
      // agent_* before other integrations; preserve insertion order among themselves
      if (isAgentA && !isAgentB) return -1;
      if (isAgentB && !isAgentA) return 1;
      if (isAgentA && isAgentB) return 0;
      // both known integrations → declaration order
      return order.indexOf(a.id) - order.indexOf(b.id);
    });
    if (!this.focusId) this.focusId = "integration_claude";
    this.notify();
  }

  removeTask(id: string) {
    const idx = this.tasks.findIndex((t) => t.id === id);
    if (idx < 0) return;
    this.tasks.splice(idx, 1);
    if (this.focusId === id) this.focusId = this.tasks[0]?.id ?? "integration_claude";
    this.notify();
  }

  /** Creates a dynamic agent_ pill on first event; no-ops if it already exists.
   *  Inserted right after integration_claude so it appears in the visible slice(0,4). */
  upsertExternalAgent(id: string, name: string, color: string) {
    if (this.tasks.some((t) => t.id === id)) return;
    const at = this.tasks.findIndex((t) => t.id === "integration_claude") + 1;
    this.tasks.splice(at, 0, {
      id, name, color,
      state: "idle", stepIndex: 0, steps: [],
      source: "agent", isIntegration: false,
    });
    if (!this.focusId) this.focusId = id;
    this.notify();
  }

  toggleIntegration(id: string) {
    if (id === "integration_claude") return;
    const active = this.settings.activeIntegrations;
    if (active.includes(id)) {
      this.settings.activeIntegrations = active.filter((x) => x !== id);
      if (this.focusId === id) this.focusId = "integration_claude";
    } else {
      if (active.length >= 4) return;
      this.settings.activeIntegrations = [...active, id];
    }
    this.loadIntegrationTasks();
  }

  /**
   * True when the minimised island carries the Music pill: the pill is switched
   * on and a track is known (the last one stays after the player closes, like the
   * GNOME extension's "Always ON"). Paused from the tray means nothing is shown.
   */
  musicStrip(): boolean {
    return (
      !this.paused &&
      this.music !== null &&
      this.tasks.some((t) => t.id === MUSIC_ID)
    );
  }

  defaultView(): IslandViewName {
    return this.tasks.length === 0 ? "empty" : "overview";
  }
}

export const State = new AppState();
