// App state — mirror of AppState.swift (the parts the island needs).

import type { BotEmoteName, BotStateName, IslandMode, IslandViewName } from "./layout";
import type { EyeShape } from "../mochi/engine";
import {
  DEFAULT_MAIN_PILL, HOST_OS, availablePills, orderPills, pillDefinition, sanitizeDeclared,
  toggleDeclared, type HostOs, type PillDefinition,
} from "./pills";
import type { CodexPlanUsage, PlanUsage } from "./plan";
import type { ProviderId } from "./providers";
import type { FileDiff } from "./diff";
import type { Bindings } from "./shortcuts";
import { DEFAULT_OUTFIT, type WornOutfit as Outfit } from "../mochi/outfits";
import { pillColor } from "./pill-colors";
import { Spotify, musicPlaying } from "./spotify";

export type AgentSource = "claudeCode" | "n8n" | "agent";
export type PillBadge = "approval" | "finished" | "error";

export interface AgentTask {
  id: string;
  name: string;
  color: string;
  state: BotStateName;
  stepIndex: number;
  steps: string[];
  /**
   * Position of the newest step in the whole session. `steps` is capped, so
   * `stepIndex` stops moving once it is full; this keeps counting.
   */
  stepSeq?: number;
  source: AgentSource;
  isIntegration: boolean;
  emote?: BotEmoteName | null;
  miniEye?: EyeShape | null;
  pillBadge?: PillBadge | null;
  sessionCwd?: string | null;
  /** Claude Code's session, so "Open terminal" can find the window it runs in. */
  sessionId?: string | null;
  /** Claude's final message after Stop, one line; cleared when a new turn starts. */
  finalLine?: string | null;
}

export interface ApprovalInfo {
  requestId: string;
  sessionId: string;
  /** The pill the request belongs to: VS Code's or Cursor's (Claude Code), or an agent's (Codex…). */
  pillId: string;
  tool: string;
  command: string;
  /** Set when Jinx (Hermes) is the one asking: the run to answer. */
  jinxRun?: string;
  /** Set when Claude Code is asking a question rather than for a permission (upstream's PermissionRequest path). */
  questions?: AskedQuestion[];
}

/** One question of an AskUserQuestion call that came as a PermissionRequest (upstream). */
export interface AskedQuestion {
  question: string;
  options: { label: string; description: string }[];
  multiSelect: boolean;
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

/** Each CLI's own Mochi: the colour he wears for that backend's sessions. */
export const BACKEND_MOCHI = {
  claude: { color: "#D97757", name: "Claude Code" },
  opencode: { color: "#22C55E", name: "OpenCode" },
} as const;

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

/** A running agent session: Claude Code (as `claude agents` lists it) or OpenCode. */
export interface AgentSession {
  id: string;
  name: string;
  cwd: string;
  /** "claude" | "opencode" (Claude Code kinds pass through untouched). */
  kind: string;
  status: string;
  /** working | done | blocked (OpenCode only reports working | done). */
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

/** A fresh, idle task for a catalog pill, in the colour the user gave it if any. */
function taskFor(def: PillDefinition, colors: unknown, name = def.name): AgentTask {
  return {
    id: def.id, name, color: pillColor(def.id, def.color, colors), state: "idle", stepIndex: 0, steps: [],
    source: def.source, isIntegration: true,
  };
}

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
  /** Hovering the island opens it all the way (off: hovering only peeks). */
  openOnHover: boolean;
  absenceInterval: number;
  /** Declared pills next to the main one (at most 4), in the order they were added. */
  activeIntegrations: string[];
  /** The always-on workspace pill: VS Code, Cursor, Codex or Antigravity. */
  mainPill: string;
  /** "primary", "cursor", or `at:<x>,<y>` for one display (logical origin). */
  screen: string;
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
  /** Seconds the pointer rests on the minimised island before it opens (open on hover). */
  hoverOpenDelay: number;
  /** Island position across the top: px from the centre, positive = right. */
  islandOffsetX: number;
  /** The island steps aside when the pointer goes for the buttons of a window behind it. */
  dodgeWindows: boolean;
  /** The sound for "an agent finished": a built-in name, "none", or "file" (finishSoundFile). */
  finishSound: string;
  finishSoundFile: string;
  /** The Hub's Server tab reads this machine over ssh ("" = none). */
  serverHost: string;
  serverServices: string;
  /** Keep Jinx in step with the Hub (notes, calendar, timer pushed to the server). */
  jinxShare: boolean;
  /** Where uploads go: the rclone remote and folder for Google Drive, the school OneDrive folder. */
  driveRemote: string;
  driveFolder: string;
  /** Google account whose Drive GNOME mounts (Online Accounts); empty = the first. */
  driveAccount: string;
  schoolFolder: string;
  /** The Weather tab asks open-meteo.com for these cities (off until turned on). */
  weatherOn: boolean;
  weatherCity: string;
  weatherCity2: string;
  /** Show other programs' notifications in the island (reads their content: off until turned on). */
  notificationPeek: boolean;
  /** Let a phone follow Claude Code and answer its requests, through ssh (off until turned on). */
  remoteEnabled: boolean;
  /** A dot on the island, with the program's name, while something records the microphone. */
  micIndicator: boolean;
  /** The Hub's Clipboard keeps what was copied (memory only; pins on disk). Off until turned on. */
  clipboardHistory: boolean;
  /** The server's downloads and backups in the pill and Today (polls over ssh). Off until turned on. */
  activitiesOn: boolean;
  /** Show the Claude plan pill (5 h and weekly limits) in the island's header. */
  showPlanInNotch: boolean;
  /** Coucou's status line relay is installed in Claude Code's settings. */
  planRelayInstalled: boolean;
  /** Show the Codex plan pill in the island's header. */
  showCodexPlanInNotch: boolean;
  /** Who the chat talks to (see core/providers.ts); picked in the chat view. */
  chatProvider: ProviderId;
  /** The model picked for each provider other than Anthropic, by provider id. */
  chatModels: Record<string, string>;
  /** Model server addresses once connected; empty means not connected. */
  ollamaUrl: string;
  lmstudioUrl: string;
  customUrl: string;
  /** Global shortcuts the user changed, by action id (see core/shortcuts.ts). */
  shortcuts: Bindings;
  /**
   * Mochi's outfit: "auto" (dresses for the season), "none" or an outfit id.
   * Same raw values as the Mac's "mochiOutfit"; read it through parseOutfit.
   */
  mochiOutfit: string;
  /**
   * A colour of the user's own for a pill's Mochi, by pill ID ("#RRGGBB").
   * Empty means the catalog's colours; read it through core/pill-colors.ts.
   * Same key and values as the Mac's "pillColors".
   */
  pillColors: Record<string, string>;
  /**
   * Interface language: "" follows the system (when Coucou has its language,
   * else English), or one of src/i18n's ten codes ("fr", "pt-BR", "zh-Hans"…).
   */
  language: string;
  /**
   * The phone link for Coucou for Android is on. Rust owns it (only the switch in
   * Settings → Android phone turns it on): whatever the page sends back is ignored.
   */
  phoneLink?: boolean;
  /** The phone may chat through this computer's API keys. Rust owns it too. */
  phoneChat?: boolean;
  /** The phone may see session details (steps, last line, folder name, colour). Rust owns it too. */
  phoneDetails?: boolean;
  /** Which models the phone may use, "provider/model". Rust owns it too. */
  phoneChatModels?: string[];
  /** Mochi on the desktop. Rust owns it: whatever the page sends back is ignored. */
  desktopMochi?: {
    onDesktop: boolean;
    spot: { x: number; y: number; space: string } | null;
  };
}

export const DEFAULT_SETTINGS: Settings = {
  soundEnabled: true,
  soundVolume: 0.12,
  autoCloseInterval: 15,
  openOnHover: false,
  absenceInterval: 180,
  activeIntegrations: [
    "integration_resend", "integration_n8n", "integration_vercel", "integration_github",
  ],
  mainPill: DEFAULT_MAIN_PILL,
  screen: "primary",
  autostart: false,
  hooksInstalled: false,
  model: "claude-opus-5",
  musicOnline: false,
  musicVisualizer: "realtime",
  musicLyricsLanguage: "any",
  musicKeep: true,
  musicScroll: "track",
  hoverOpenDelay: 0.9,
  islandOffsetX: 0,
  dodgeWindows: true,
  finishSound: "finish",
  finishSoundFile: "",
  serverHost: "",
  serverServices: "hermes-gateway,ollama,docker,tailscaled,smbd",
  jinxShare: true,
  driveRemote: "gdrive",
  driveFolder: "Coucou",
  driveAccount: "",
  schoolFolder: "Por clasificar",
  weatherOn: false,
  weatherCity: "",
  weatherCity2: "",
  notificationPeek: false,
  remoteEnabled: false,
  micIndicator: true,
  clipboardHistory: false,
  activitiesOn: false,
  showPlanInNotch: false,
  planRelayInstalled: false,
  showCodexPlanInNotch: false,
  chatProvider: "anthropic",
  chatModels: {},
  ollamaUrl: "",
  lmstudioUrl: "",
  customUrl: "",
  shortcuts: {},
  mochiOutfit: DEFAULT_OUTFIT,
  pillColors: {},
  language: "",
};

type Listener = () => void;

/** What the minimised island's pill can be showing, most important first. */
export type PillKind = "dictation" | "reminder" | "flash" | "pomodoro" | "activities";

/** Something the server is busy with: a download, a backup that is late (activities.rs). */
export interface Activity {
  id: string;
  app: string;
  kind: "download" | "backup" | "other";
  title: string;
  progress: number | null;
  eta: number | null;
  state: "active" | "queued" | "done" | "error";
  detail: string;
}

/**
 * The activities that hold the pill: running or waiting. One that went wrong is not here — a late
 * backup would keep the island up for days; it is said once (a flash) and stays as a row in Today.
 */
export function pillActivities(list: Activity[]): Activity[] {
  return list.filter((a) => a.state === "active" || a.state === "queued");
}

/** A few seconds of news in the pill ("Plugged in", "Back online"). */
export interface Flash {
  icon: string;
  text: string;
  tone: "" | "ok" | "warn" | "crit";
}

/** A paired or plugged device with a battery (headphones, mouse, game controller…). */
export interface Peripheral {
  id: string;
  name: string;
  kind: string;
  percent: number;
  charging: boolean;
}

/** The battery, the mains and the internet, as the system last said. */
export interface SysInfo {
  battery: number | null;
  plugged: boolean;
  charging: boolean;
  online: boolean;
}

/** Live diffs kept per pill (oldest dropped first) — same cap as macOS. */
export const MAX_DIFFS_PER_PILL = 50;
/** A pill's diffs are forgotten after an hour without a new one, as on macOS. */
export const DIFF_TTL_MS = 3_600_000;

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
  /** What the cloud upload is doing, shown on the drop card in place of its question. */
  uploadStatus = "";

  promptContext: PromptContext | null = null;
  droppedFile: { name: string; path: string } | null = null;
  noteMessage: string | null = null;
  // ── The status pill (it shares the Music pill's place on the minimised island) ──
  sys: SysInfo = { battery: null, plugged: false, charging: false, online: true };
  /** Handy: recording, then a moment of transcribing. */
  dictation: "off" | "listening" | "transcribing" = "off";
  dictationSince = 0;
  /** Reminders that have gone off and wait for a click. */
  reminders: { id: string; title: string }[] = [];
  flash: Flash | null = null;
  /** The Pomodoro's period, kept here by the Pomodoro so the pill can follow it. */
  pomodoroPhase: "idle" | "focus" | "break" = "idle";
  pomodoroRunning = false;
  searchResult: SearchResult | null = null;
  chatHistory: ChatMessage[] = [];
  /** Who the chat talks to: Mochi (Claude's API) or Jinx (Hermes). Each keeps its own thread. */
  chatTarget: "mochi" | "jinx" | "agent" | "new" = "mochi";
  /** Which backend a new agent starts on — and which Mochi answers while it is picked. */
  chatBackend: "claude" | "opencode" = "claude";
  /** The running agent sessions (Claude Code and OpenCode), the selected one, and what has been read of each. */
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
  /**
   * The user clicked inside the Hub's panel: the island takes the keyboard (for ← → and 1–9) until
   * the pointer leaves, the view changes or the island closes. Never taken on hover.
   */
  hubEngaged = false;
  /** The Hub's sub-tab in front (remembered). */
  hubTab: string = (() => {
    try {
      return window.localStorage.getItem("coucou.hubTab") || "today";
    } catch {
      return "today";
    }
  })();
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
  /** The pill that was in front when the card came up; it comes back after. */
  focusBeforeApproval: string | null = null;

  integrations: Record<string, IntegrationInfo> = {};

  /** Claude's 5 h / weekly limits, from the status line (null until the first call). */
  planUsage: PlanUsage | null = null;
  /** Codex's limits, from `codex app-server` (null until it has answered). */
  codexPlanUsage: CodexPlanUsage | null = null;
  /** A plan card is open in place of the overview's left card. */
  showingPlanDetail = false;
  /** Which one: the Codex card rather than Claude's. */
  planDetailIsCodex = false;
  /** Per-pill file diffs, in order of reception. Steps carry their ids. */
  sessionDiffs = new Map<string, FileDiff[]>();
  private sessionDiffTimers = new Map<string, number>();
  /** Never reset, so an id can never point at a newer diff than the one tapped. */
  private nextDiffId = 0;
  /** Ctrl+↑ / Ctrl+↓: the highlighted row of the card's list (AppState.cardSelection). */
  cardSelection: number | null = null;
  /** Rows in the list on screen, 0 when there is none (AppState.cardItemCount). */
  cardItemCount = 0;
  /**
   * Mochi is out of the island — on the desktop, flying, or being dragged
   * there — so the island's own Mochi is hidden (AppState.mochiOnDesktop).
   */
  mochiOnDesktop = false;

  /** Outfit shown on Mochi while the pointer rests on a wardrobe button. */
  wardrobePreview: Outfit | null = null;

  lastActivity = performance.now();

  settings: Settings = { ...DEFAULT_SETTINGS };

  /** What the server is busy with, as it last said (island/activities.ts). */
  activities: Activity[] = [];

  /** Devices with a battery, as the system last said (peripherals.rs). */
  peripherals: Peripheral[] = [];

  /** The programs recording the microphone right now (names only; Handy and Coucou's bars excluded). */
  micApps: string[] = [];

  /** Is the microphone indicator on show (something records and the user has not turned it off)? */
  micOn(): boolean {
    return !this.paused && this.settings.micIndicator !== false && this.micApps.length > 0;
  }

  /** Which pills this build offers depends on it (Claude Desktop is Windows only). */
  os: HostOs = HOST_OS;

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

  /** Spotify plays on a declared pill: Mochi dances (Linux; never on Windows yet). */
  get spotifyPlaying(): boolean {
    return musicPlaying(Spotify.state, sanitizeDeclared(this.settings, this.os).activeIntegrations);
  }

  get otherTasks(): AgentTask[] {
    return this.tasks.filter((t) => t.id !== this.focusId);
  }

  setFocus(id: string) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    this.focusId = id;
    this.showingPlanDetail = false;
    t.pillBadge = null;
    this.notify();
  }

  /**
   * A permission card or a question comes up: its pill comes to the front, and
   * the pill that was there is remembered (HookServer.focusBeforeApproval).
   */
  beginApproval(info: ApprovalInfo) {
    this.pendingApproval = info;
    this.isPinned = true;
    if (this.focusBeforeApproval == null) this.focusBeforeApproval = this.focusId;
    this.setFocus(info.pillId);
  }

  /**
   * The card has its answer, or is withdrawn: the session carries on, and the
   * pill you were on comes back — unless you moved to another one meanwhile.
   */
  endApproval() {
    const req = this.pendingApproval;
    if (!req) return;
    this.pendingApproval = null;
    this.isPinned = false;
    this.updateTask(req.pillId, "working");
    this.setPillBadge(req.pillId, null);
    const previous = this.focusBeforeApproval;
    this.focusBeforeApproval = null;
    if (previous && this.focusId === req.pillId && this.tasks.some((t) => t.id === previous)) {
      this.focusId = previous;
    }
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
    const newest = t.stepSeq ?? t.steps.length - 1;
    t.steps.push(step);
    if (t.steps.length > 20) t.steps.shift();
    t.stepIndex = t.steps.length - 1;
    t.stepSeq = newest + 1;
    this.notify();
  }

  setPillBadge(id: string, badge: PillBadge | null) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    t.pillBadge = badge;
    this.notify();
  }

  /** Stores a diff for a pill and returns its id (for the ticker step). */
  appendSessionDiff(pillId: string, diff: FileDiff): number {
    const id = this.nextDiffId++;
    const list = this.sessionDiffs.get(pillId) ?? [];
    list.push({ ...diff, id });
    while (list.length > MAX_DIFFS_PER_PILL) list.shift();
    this.sessionDiffs.set(pillId, list);
    // One timer per pill, re-armed on every diff — nothing polls.
    const prev = this.sessionDiffTimers.get(pillId);
    if (prev != null) window.clearTimeout(prev);
    this.sessionDiffTimers.set(
      pillId,
      window.setTimeout(() => this.clearSessionDiffs(pillId), DIFF_TTL_MS),
    );
    return id;
  }

  findDiff(pillId: string, id: number): FileDiff | null {
    return this.sessionDiffs.get(pillId)?.find((d) => d.id === id) ?? null;
  }

  clearSessionDiffs(pillId: string) {
    const timer = this.sessionDiffTimers.get(pillId);
    if (timer != null) window.clearTimeout(timer);
    this.sessionDiffTimers.delete(pillId);
    this.sessionDiffs.delete(pillId);
  }

  /** The always-on workspace pill, once the setting has been checked. */
  get mainPillId(): string {
    return sanitizeDeclared(this.settings, this.os).mainPill;
  }

  /**
   * True for a pill that stays when its session ends: the main pill and the
   * declared ones go back to idle instead of going away.
   */
  isKept(id: string): boolean {
    const d = sanitizeDeclared(this.settings, this.os);
    return id === d.mainPill || d.activeIntegrations.includes(id);
  }

  /**
   * Loads the catalog pills: the main pill always, the declared ones, and none
   * of the others — a pill that is mid-session stays until its session ends.
   * Safe to call any number of times. AppState.loadIntegrationTasks on macOS.
   */
  loadIntegrationTasks() {
    const d = sanitizeDeclared(this.settings, this.os);
    this.settings.mainPill = d.mainPill;
    this.settings.activeIntegrations = d.activeIntegrations;
    for (const def of availablePills(this.os)) {
      const shouldLoad = def.id === d.mainPill || d.activeIntegrations.includes(def.id);
      const idx = this.tasks.findIndex((t) => t.id === def.id);
      if (shouldLoad && idx < 0) this.tasks.push(taskFor(def, this.settings.pillColors));
      const busy = idx >= 0 && (this.tasks[idx].state !== "idle" || this.tasks[idx].steps.length > 0);
      if (!shouldLoad && idx >= 0 && !busy) this.tasks.splice(idx, 1);
    }
    // A colour picked in Settings reaches the pills that are already up.
    for (const t of this.tasks) {
      const def = pillDefinition(t.id);
      if (def) t.color = pillColor(def.id, def.color, this.settings.pillColors);
    }
    this.tasks = orderPills(this.tasks, d.mainPill);
    if (!this.focusId || !this.tasks.some((t) => t.id === this.focusId)) this.focusId = d.mainPill;
    this.notify();
  }

  /**
   * A session is over. The main and declared pills are put back as they were;
   * any other pill goes away (AppState.removeTask on macOS).
   */
  removeTask(id: string) {
    const idx = this.tasks.findIndex((t) => t.id === id);
    if (idx < 0) return;
    if (this.isKept(id)) {
      const t = this.tasks[idx];
      t.state = "idle";
      t.steps = [];
      t.stepIndex = 0;
      delete t.stepSeq;
      t.pillBadge = null;
      t.finalLine = null;
      const def = pillDefinition(id);
      if (def) t.name = def.name;
      this.clearSessionDiffs(id);
      this.notify();
      return;
    }
    this.tasks.splice(idx, 1);
    this.clearSessionDiffs(id);
    if (this.focusId === id) this.focusId = this.tasks[0]?.id ?? this.mainPillId;
    this.notify();
  }

  /**
   * Creates the pill of a tagged agent on its first event; no-op if it exists.
   * Inserted right after the main pill so it is in the visible slice(0,4). A
   * catalog agent wears its catalog colour, as on macOS.
   */
  upsertExternalAgent(id: string, name: string, color: string) {
    if (this.tasks.some((t) => t.id === id)) return;
    const def = pillDefinition(id);
    this.insertAfterMain({
      id, name, color: def ? pillColor(def.id, def.color, this.settings.pillColors) : color,
      state: "idle", stepIndex: 0, steps: [],
      source: "agent", isIntegration: false,
    });
  }

  /**
   * The pill a Claude Code session belongs to (VS Code or Cursor). It is made
   * for the session when it is neither the main pill nor declared, as
   * upsertWorkspaceTask does on macOS.
   */
  upsertWorkspacePill(id: string, name: string, cwd: string): AgentTask | null {
    let t = this.tasks.find((x) => x.id === id);
    if (!t) {
      const def = pillDefinition(id);
      if (!def) return null;
      t = taskFor(def, this.settings.pillColors, name);
      this.insertAfterMain(t);
    }
    t.name = name;
    if (cwd) t.sessionCwd = cwd;
    return t;
  }

  private insertAfterMain(t: AgentTask) {
    const at = this.tasks.findIndex((x) => x.id === this.mainPillId) + 1;
    this.tasks.splice(at, 0, t);
    if (!this.focusId) this.focusId = t.id;
    this.notify();
  }

  /** Declares or undeclares a pill (max 4 next to the main one). */
  toggleIntegration(id: string) {
    const next = toggleDeclared(sanitizeDeclared(this.settings, this.os), id, this.os);
    if (!next) return;
    this.settings.activeIntegrations = next;
    if (!next.includes(id) && this.focusId === id) this.focusId = this.mainPillId;
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

  /** What the pill shows now: a recording, a reminder, a bit of news, or the Pomodoro running. */
  pillKind(): PillKind | null {
    if (this.paused) return null;
    if (this.dictation !== "off") return "dictation";
    if (this.reminders.length > 0) return "reminder";
    if (this.flash) return "flash";
    if (this.pomodoroPhase !== "idle") return "pomodoro";
    // The Music pill is the user's own: downloads never take its place (they ride on it as a chip).
    if (!this.musicStrip() && pillActivities(this.activities).length > 0) return "activities";
    return null;
  }

  /** The minimised island is wide when something is in its pill or the Music pill has a track. */
  stripWanted(): boolean {
    return this.pillKind() !== null || this.musicStrip();
  }

  /**
   * What the island opens on. A card waiting for an answer comes first, so
   * reopening a folded island shows it again (Mac #117, #290).
   */
  defaultView(): IslandViewName {
    if (this.pendingQuestion) return "question";
    if (this.pendingApproval) return this.pendingApproval.questions ? "question" : "approval";
    return this.tasks.length === 0 ? "empty" : "overview";
  }
}

export const State = new AppState();
