// Thin wrapper over the Tauri commands/events. Every call is a no-op when the
// page is opened in a plain browser, so the island can be iterated on with
// `npm run dev` alone.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { AgentSession, LimitWindow, LyricLine, MusicTrack, Settings } from "./state";

export const IS_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!IS_TAURI) return null;
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    console.error(`[coucou] ${cmd} failed`, err);
    return null;
  }
}

export interface BootInfo {
  settings: Settings;
  /** Logical screen rect of the monitor the island lives on. */
  screen: { x: number; y: number; width: number; height: number; scale: number };
  version: string;
  hookPath: string;
  /** False where the OS has no global cursor (Wayland): see Island.followPageCursor. */
  cursorPoll: boolean;
}

export const Bridge = {
  boot: () => call<BootInfo>("boot"),

  saveSettings: (settings: Settings) => call<void>("save_settings", { settings }),

  /** Shrink the window down to the invisible wake strip (hidden) or back to full. */
  setCollapsed: (collapsed: boolean) => call<void>("set_collapsed", { collapsed }),

  /**
   * Pushes the island shape in window coordinates. Rust flips click-through from
   * its own cursor poll, so the flag is never a frame behind a click.
   */
  setIslandRect: (x: number, y: number, width: number, height: number) =>
    call<void>("set_island_rect", { x, y, width, height }),

  /** Give the window keyboard focus (chat field) and take it away again. */
  focusWindow: (focused: boolean) => call<void>("focus_window", { focused }),

  reposition: () => call<void>("reposition"),

  openUrl: (url: string) => call<void>("open_url", { url }),

  /** "Open terminal" → opens the folder in VS Code when `code` is on PATH. */
  openInVSCode: (path: string | null) => call<boolean>("open_in_vscode", { path }),

  quit: () => call<void>("quit_app"),

  openSettingsWindow: () => call<void>("open_settings_window"),

  /** Writes to %LOCALAPPDATA%\Coucou\coucou.log, next to the Rust lines. */
  log: (message: string) => call<void>("log_line", { message }),

  // ── Claude Code hooks ─────────────────────────────────────────────────────
  hooksStatus: () => call<HookStatus>("hooks_status"),
  /** Diff to show before anything is written. `install: false` previews removal. */
  hooksPreview: (install: boolean) => callOrThrow<HookPreview>("hooks_preview", { install }),
  /**
   * Writes ~/.claude/settings.json — only ever after an explicit click, and only
   * when the file still matches the preview the user looked at.
   */
  hooksApply: (install: boolean, fingerprint: string) =>
    callOrThrow<string>("hooks_apply", { install, fingerprint }),

  approvalDecision: (requestId: string, decision: "allow" | "deny") =>
    call<void>("approval_decision", { requestId, decision }),
  /** "The card is up" — until this lands the relay only waits a moment. */
  approvalAck: (requestId: string) => call<void>("approval_ack", { requestId }),
  /** "Nobody can act on this" — Claude Code asks in the terminal right away. */
  approvalDecline: (requestId: string) => call<void>("approval_decline", { requestId }),
  /**
   * The answers to an AskUserQuestion card: each question's text → the chosen
   * label, or the list of labels for a multi-select question.
   */
  questionAnswer: (requestId: string, answers: Record<string, string | string[]>) =>
    call<void>("question_answer", { requestId, answers }),

  // ── Music (MPRIS) ─────────────────────────────────────────────────────────
  /** What the last `music` event said, for a page that has just loaded. */
  musicState: () => call<MusicTrack | null>("music_state"),
  /** playpause / next / previous, or seek to `valueMs` from the start of the track. */
  musicControl: (action: "playpause" | "next" | "previous" | "seek" | "raise" | "volume", valueMs?: number) =>
    call<void>("music_control", { action, valueMs: valueMs ?? null }),
  /** The art as a data: URL. Rejects when it is a web URL and online fetching is off. */
  musicArt: (url: string) => callOrThrow<string>("music_art", { url }),
  /** Synchronised lyrics, or null when there are none. Rejects while online fetching is off. */
  musicLyrics: (title: string, artist: string, album: string, durationMs: number) =>
    callOrThrow<LyricLine[] | null>("music_lyrics", { title, artist, album, durationMs }),

  /**
   * Starts or stops `cava` (the real-time visualizer). Resolves to whether it is
   * running: false when it is not installed.
   */
  musicBars: (on: boolean) => call<boolean>("music_bars", { on }),

  // ── Chat, files, secrets ──────────────────────────────────────────────────
  /** One chat turn. The API key and any file bytes never leave Rust. */
  chatSend: (query: string, context: ChatContext | null) =>
    callOrThrow<{ text: string }>("chat_send", { query, context }),
  chatReset: () => call<void>("chat_reset"),
  // Claude Code and OpenCode sessions: the running ones, what was said, a message into one, a new one.
  agentsList: () => callOrThrow<AgentSession[]>("agents_list"),
  rateLimits: () => call<{ five: LimitWindow | null; seven: LimitWindow | null; ts: number } | null>("rate_limits"),
  agentMessages: (sessionId: string, offset: number) =>
    callOrThrow<{ offset: number; messages: { role: "user" | "assistant" | "tool" | "queued" | "dequeued" | "queue-clear"; text: string }[] }>(
      "agent_messages", { sessionId, offset }),
  agentSend: (id: string, text: string) => callOrThrow<void>("agent_send", { id, text }),
  agentStart: (cwd: string, prompt: string, backend: "claude" | "opencode") =>
    callOrThrow<string>("agent_start", { cwd, prompt, backend }),
  agentStop: (id: string) => callOrThrow<void>("agent_stop", { id }),
  calendarEvents: (days: number) => callOrThrow<unknown[]>("calendar_events", { days }),
  jinxTasks: () => callOrThrow<unknown[]>("jinx_tasks"),
  tasksList: () => callOrThrow<import("../hub/tasks").Task[]>("tasks_list"),
  tasksAdd: (text: string, day: number | null, hour: number) =>
    callOrThrow<import("../hub/tasks").Task[]>("tasks_add", { text, day, hour }),
  tasksDone: (id: string, done: boolean) => callOrThrow<import("../hub/tasks").Task[]>("tasks_done", { id, done }),
  tasksSnooze: (id: string, until: number) => callOrThrow<import("../hub/tasks").Task[]>("tasks_snooze", { id, until }),
  tasksSetJinx: (id: string, on: boolean) => callOrThrow<import("../hub/tasks").Task[]>("tasks_set_jinx", { id, on }),
  /** One round with Jinx's pendientes; resolves with our list and what she holds that is not ours. */
  tasksSync: () => callOrThrow<{ tasks: import("../hub/tasks").Task[]; jinx: import("../hub/tasks").JinxRow[]; error: string | null }>("tasks_sync"),
  jinxResolve: (id: string, action: "hecho" | "descartado") => callOrThrow<void>("jinx_resolve", { id, action }),
  tasksNotified: (id: string) => callOrThrow<import("../hub/tasks").Task[]>("tasks_notified", { id }),
  tasksDelete: (id: string) => callOrThrow<import("../hub/tasks").Task[]>("tasks_delete", { id }),
  tasksClearDone: () => callOrThrow<import("../hub/tasks").Task[]>("tasks_clear_done"),
  /** Battery, mains power and internet, as the system last said; null where it cannot say. */
  sysState: () => call<{ battery: number | null; plugged: boolean; charging: boolean; online: boolean } | null>("sys_state"),
  workspacesList: () => callOrThrow<unknown[]>("workspaces_list"),
  workspacesSave: (list: unknown[]) => callOrThrow<unknown[]>("workspaces_save", { list }),
  workspacesApps: () => callOrThrow<unknown[]>("workspaces_apps"),
  workspacesLaunch: (id: string) => callOrThrow<string[]>("workspaces_launch", { id }),
  notesLoad: () => callOrThrow<string>("notes_load"),
  notesSave: (text: string) => callOrThrow<void>("notes_save", { text }),
  shareStatus: () => callOrThrow<{ lastOk: number | null; lastError: string | null }>("share_status"),
  sharePush: (name: string, content: string) => callOrThrow<void>("share_push", { name, content }),
  quickState: () => call<import("../hub/quick").QuickState>("quick_state"),
  quickSet: (what: string, value: number) => callOrThrow<void>("quick_set", { what, value }),
  weatherGet: (city: string) => callOrThrow<unknown>("weather_get", { city }),
  shelfUpload: (path: string, dest: "school" | "drive", folder: string) => callOrThrow<{ location: string; url: string | null }>("shelf_upload", { path, dest, folder }),
  driveState: () => callOrThrow<{ installed: boolean; connected: boolean }>("drive_state"),
  driveConnect: () => callOrThrow<void>("drive_connect"),
  schoolLoginStart: () => callOrThrow<void>("school_login_start"),
  shelfList: () => call<{ name: string; path: string; size: number; ageSecs: number }[]>("shelf_list"),
  shelfRemove: (path: string) => callOrThrow<void>("shelf_remove", { path }),
  shelfOpen: (path: string | null) => callOrThrow<void>("shelf_open", { path }),
  pomodoroLog: (kind: "focus" | "break", seconds: number, completed: boolean) =>
    callOrThrow<void>("pomodoro_log", { kind, seconds, completed }),
  pomodoroStats: () => callOrThrow<import("../hub/pomodoro").PomoStats>("pomodoro_stats"),
  // The Hub: vitals of this computer and of the server (over ssh).
  sysSample: () => call<import("../hub/system").Vitals>("sys_sample"),
  serverSample: () => callOrThrow<import("../hub/system").Vitals>("server_sample"),
  /** An audio file the user chose, as base64. */
  readSound: (path: string) => callOrThrow<string>("read_sound", { path }),
  /** Starts a Jinx run; the answer streams back as `jinx` events. */
  jinxSend: (text: string, context: ChatContext | null) => callOrThrow<void>("jinx_send", { text, context }),
  jinxApprove: (runId: string, requestId: string, choice: "once" | "session" | "always" | "deny") =>
    callOrThrow<void>("jinx_approve", { runId, requestId, choice }),
  jinxStop: () => call<void>("jinx_stop"),
  jinxTest: () => callOrThrow<string>("jinx_test"),
  /** Copies a dropped file into the inbox. */
  ingestFile: (path: string) => callOrThrow<DroppedFile>("ingest_file", { path }),
  /** Only ever tells you whether a key exists — never its value. */
  secretPresent: (key: string) => call<boolean>("secret_present", { key }),
  secretSet: (key: string, value: string) => callOrThrow<void>("secret_set", { key, value }),
  secretClear: (key: string) => callOrThrow<void>("secret_clear", { key }),

  // ── Integrations ──────────────────────────────────────────────────────────
  refreshIntegration: (id: string) => call<void>("refresh_integration", { id }),
  /** Opens the configured n8n instance in the browser. */
  openN8n: () => call<void>("open_n8n"),

  /** Tray → Pause. Stops the integration pollers, not just the island. */
  setPaused: (paused: boolean) => call<void>("set_paused", { paused }),
};

export interface IntegrationUpdate {
  id: string;
  data: Record<string, unknown>;
  error: string | null;
  event: { success: boolean; label: string; detail: string | null } | null;
}

export type ChatContext =
  | { kind: "file"; name: string; path: string }
  | { kind: "window"; appName: string; title: string; url?: string };

export interface DroppedFile {
  name: string;
  path: string;
  size: number;
}

export interface HookStatus {
  installed: boolean;
  /** False when the hooks predate AskUserQuestion support: install again. */
  askInstalled: boolean;
  settingsPath: string;
  hookPath: string;
  hookReady: boolean;
}

export interface HookPreview {
  diff: string;
  backup: string;
  settingsPath: string;
  /** Hand back to hooksApply so only the reviewed diff is ever written. */
  fingerprint: string;
}

/** Same as `call`, but surfaces the error so the UI can show what went wrong. */
async function callOrThrow<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!IS_TAURI) throw new Error("not running inside Coucou");
  return invoke<T>(cmd, args);
}

export type BridgeEvent =
  | { name: "cursor"; payload: { x: number; y: number } }
  | { name: "gaze"; payload: { x: number; y: number } }
  | { name: "obstacles"; payload: number[][] }
  | { name: "jinx"; payload: Record<string, unknown> }
  | { name: "notification"; payload: { app: string; summary: string; body: string } }
  | { name: "school-login"; payload: { kind: string; text: string; ok?: boolean } }
  | { name: "tray"; payload: string }
  | { name: "hook"; payload: Record<string, unknown> }
  | { name: "screen-changed"; payload: null };

export interface DragDropPayload {
  type: "enter" | "over" | "drop" | "leave";
  paths?: string[];
}

/** Files dragged onto the island. Only reaches us when the window takes the mouse. */
export async function onDragDrop(handler: (e: DragDropPayload) => void) {
  if (!IS_TAURI) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => {
    handler(event.payload as DragDropPayload);
  });
}

export async function onEvent<T>(name: string, handler: (payload: T) => void) {
  if (!IS_TAURI) return () => {};
  return listen<T>(name, (e) => handler(e.payload));
}
