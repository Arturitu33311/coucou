// The island: DOM shell, sizing animation, Mochi placement, mouse handling.
// Mirrors IslandRootView.swift + IslandWindowController.swift.

import { Tracked, Spring, clamp } from "../core/anim";
import { Bridge, IS_TAURI, onDragDrop } from "../core/bridge";
import {
  EXPANDED_CORNER, EXPANDED_W, NOTCH_W, PANEL_H, PANEL_W,
  ROUNDED_CORNER, VIEW_LAYOUTS, botGlowColor, botGlowOpacity, botPosition, chatPromptHeight,
  islandSize,
  type IslandMode, type IslandViewName,
} from "../core/layout";
import { Sound } from "../core/sound";
import { INTEGRATION_AGENTS, JINX_ID, State } from "../core/state";
import { answerJinxApproval } from "./jinx";
import { BotEngine, hexToRGB } from "../mochi/engine";
import { Greeting } from "../mochi/greeting";
import { createMiniBot, pruneMiniBots, syncMiniBotStates, tickMiniBots } from "../mochi/minibots";
import { UploadCanvas } from "../upload/canvas";
import { USC, UploadSeq } from "../upload/sequence";
import { buildHeader, buildViews, type ViewActions, type ViewHost } from "../views/views";
import { buildMusicStrip, type MusicHost } from "../views/music";
import { setBarsWanted } from "./music";
import { h } from "../views/dom";
import { IslandStateMachine } from "./fsm";
import { Dodger, toBoxes, type Box } from "./dodge";
import { hubTyping, syncHubRuntime } from "../hub/hub";

const BOT_OVERHANG = 40;
/** Seconds the island stays open after the pointer leaves it, with the Music pill. */
const MUSIC_LEAVE_S = 0.3;
/** How long a note (see above) stays up with the pointer elsewhere. */
const NOTE_LINGER_S = 6;
/** With "open on hover": how long the pointer rests on the minimised island before it opens. */
const HOVER_OPEN_S = 0.9;
/** Frame interval while only the minimised Music pill animates (≈ 12 fps)… */
const STRIP_FRAME_MS = 80;
/** …and with the real-time bars, which need to keep up with the music (≈ 30 fps). */
const STRIP_FRAME_LIVE_MS = 33;
/** Same margin as the Rust hit test (src-tauri/src/island.rs). */
const HIT_MARGIN = 14;

/** The three views the drop sequence owns; leaving them stops the engine. */
const UPLOAD_VIEWS: ReadonlySet<IslandViewName> = new Set(["upload", "uploading", "choose"]);

/** Seconds between the drop and the moment the progress bar starts filling. */
const PRE_PROGRESS = USC.T_PROG_START - USC.T_DROP;

const modeOrder = (m: IslandMode) => (m === "hidden" ? 0 : m === "compact" ? 1 : 2);

export class Island {
  readonly fsm = new IslandStateMachine();

  private root: HTMLElement;
  private islandEl!: HTMLElement;
  private clipEl!: HTMLElement;
  private contentEl!: HTMLElement;
  private viewsEl!: HTMLElement;
  private botCanvas!: HTMLCanvasElement;
  private botGlow!: HTMLElement;
  private greetingCanvas!: HTMLCanvasElement;
  private miniGrid!: HTMLElement;
  /** The Music pill shown on the minimised island. */
  private strip!: MusicHost;
  private countdown!: HTMLElement;
  private wakeStrip!: HTMLElement;

  private header!: ViewHost;
  private views!: Map<IslandViewName, ViewHost>;
  private uploadCanvas!: UploadCanvas;

  private width = new Tracked(NOTCH_W);
  private height = new Tracked(0);
  private radius = new Tracked(ROUNDED_CORNER);
  /** How far the island has stepped aside for the buttons of a window behind it (see dodge.ts). */
  private dodge = new Tracked(0);
  private dodgeTarget = 0;
  private dodger = new Dodger();
  private dodgeTimer: number | null = null;
  private lastWantsKeys = false;
  private zones: Box[] = [];
  private pointer: { x: number; y: number } | null = null;
  private botCx = new Spring(46);
  private botCy = new Spring(16);
  private botSize = new Spring(10);

  private engine = new BotEngine();
  private greeting = new Greeting();

  private running = false;
  private lastFrame = 0;
  private dirty = true;
  private canvasPx = 0;

  // Rust starts the window at full size so the launch greeting has room.
  private collapsed = false;
  private collapseTimer: number | null = null;
  private wasInIsland = false;
  /** Last shape handed to Rust for the click-through test. */
  private pushedRect = { x: -1, y: -1, w: -1, h: -1 };
  private homeCollapseAt: number | null = null;

  // Bot hover → love (IslandWindowController.botHoverIn)
  private botHovering = false;
  private botHoverTimer: number | null = null;
  private lastLoveTime = 0;
  private botHoverStart = { x: 0, y: 0 };

  private confusedRecovery: number | null = null;
  private prevViewBeforeConfused: IslandViewName = "overview";
  private lastSyncedView: IslandViewName | null = null;

  /** Drop sequence bookkeeping: last tick played, and whether the ✓ has fired. */
  private uploadTens = 0;
  private uploadDone = false;

  constructor(root: HTMLElement) {
    this.root = root;
    this.build();
    this.wireFsm();
    this.wireInput();
    this.engine.onDizzy = () => this.handleDizzy();
    this.greeting.onComplete = () => this.fsm.greetComplete();
    State.subscribe(() => {
      this.dirty = true;
      this.ensureRunning();
    });
  }

  // ── DOM ─────────────────────────────────────────────────────────────────────

  private build() {
    const actions: ViewActions = {
      setView: (v) => this.setView(v),
      collapse: () => this.collapse(),
      setFocus: (id) => {
        State.setFocus(id);
        Sound.play("blip");
      },
      openTerminal: () => {
        const cwd = State.focusTask?.sessionCwd ?? null;
        void Bridge.openInVSCode(cwd);
      },
      // The ↗ button — same targets as openAgentTarget() on macOS.
      openTarget: () => {
        const task = State.focusTask;
        if (!task) return;
        const urls: Record<string, string> = {
          integration_resend: "https://resend.com/emails",
          integration_vercel: "https://vercel.com/dashboard",
          integration_github: "https://github.com",
          integration_stripe: "https://dashboard.stripe.com/payments",
          integration_notion: "https://notion.so",
          integration_calcom: "https://app.cal.com/bookings",
        };
        if (task.id === "integration_claude") void Bridge.openInVSCode(task.sessionCwd ?? null);
        else if (task.id === "integration_music") this.setView("music");
        else if (task.id === JINX_ID) actions.talkToJinx();
        else if (task.id === "integration_n8n") void Bridge.openN8n();
        else if (urls[task.id]) void Bridge.openUrl(urls[task.id]);
      },
      openUrl: (url) => {
        if (url) void Bridge.openUrl(url);
      },
      decide: (d) => {
        const req = State.pendingApproval;
        void Bridge.log(`decide ${d} req=${req?.requestId ?? "none"}`);
        if (!req) return;
        Sound.play(d === "deny" ? "blip" : "approve");
        // Jinx's requests go back to Hermes; everything else is Claude Code's relay.
        const agent = req.jinxRun ? JINX_ID : "integration_claude";
        if (req.jinxRun) answerJinxApproval(d === "allow");
        else void Bridge.approvalDecision(req.requestId, d);
        State.pendingApproval = null;
        State.isPinned = false;
        this.fsm.pinned = false;
        State.updateTask(agent, "working");
        State.setPillBadge(agent, null);
        this.setView(State.defaultView());
      },
      talkToJinx: () => {
        State.chatTarget = "jinx";
        this.setView("prompt");
      },
      stopJinx: () => {
        void Bridge.jinxStop();
      },
      submitQuestion: () => {
        const req = State.pendingQuestion;
        if (!req) return;
        // Each question's text is the key of its answer: a label for a single-select
        // question, the list of labels for a multi-select one.
        const answers: Record<string, string | string[]> = {};
        req.questions.forEach((q, i) => {
          const picked = req.answers[i];
          if (picked && picked.length > 0) answers[q.question] = q.multiSelect ? picked : picked[0];
        });
        void Bridge.log(`question answered req=${req.requestId}`);
        Sound.play("approve");
        void Bridge.questionAnswer(req.requestId, answers);
        this.closeQuestion();
      },
      questionToTerminal: () => {
        const req = State.pendingQuestion;
        if (!req) return;
        void Bridge.log(`question → terminal req=${req.requestId}`);
        Sound.play("blip");
        void Bridge.approvalDecline(req.requestId);
        this.closeQuestion();
      },
      toggleSound: () => {
        State.settings.soundEnabled = !State.settings.soundEnabled;
        Sound.setEnabled(State.settings.soundEnabled);
        void Bridge.saveSettings(State.settings);
        State.notify();
      },
      setVolume: (v) => {
        State.settings.soundVolume = v;
        Sound.setVolume(v);
        void Bridge.saveSettings(State.settings);
        State.notify();
      },
      setAutoClose: (s) => {
        State.settings.autoCloseInterval = s;
        this.fsm.homeToPetitDelay = s;
        void Bridge.saveSettings(State.settings);
        State.notify();
      },
      openSettingsWindow: () => void Bridge.openSettingsWindow(),
      blip: () => Sound.play("blip"),
    };

    this.wakeStrip = h("div", { id: "wake-strip" });
    this.botGlow = h("div", { id: "bot-glow" });
    this.botCanvas = h("canvas", { id: "bot-canvas" });
    this.greetingCanvas = h("canvas", { id: "greeting-canvas" });
    this.miniGrid = h("div", { id: "mini-grid" });
    this.countdown = h("div", { id: "countdown" });

    this.header = buildHeader(actions);
    this.views = buildViews(actions, () => this.animateGeometry(false));
    this.viewsEl = h("div", { id: "views" });
    for (const v of this.views.values()) this.viewsEl.append(v.el);
    this.contentEl = h("div", { id: "content" }, this.header.el, this.viewsEl);

    // The drop sequence draws the card, the bar and its own Mochi. It sits under
    // the header, which stays visible on top of it exactly as on macOS.
    this.uploadCanvas = new UploadCanvas({
      ask: () => {
        State.promptContext = State.droppedFile
          ? { kind: "file", name: State.droppedFile.name, path: State.droppedFile.path }
          : null;
        State.chatTarget = "mochi";
        this.setView("prompt");
      },
      askJinx: () => {
        State.promptContext = State.droppedFile
          ? { kind: "file", name: State.droppedFile.name, path: State.droppedFile.path }
          : null;
        State.chatTarget = "jinx";
        this.setView("prompt");
      },
      toCloud: (dest) => void this.sendToCloud(dest),
      cancel: () => this.setView(State.defaultView()),
    });

    this.clipEl = h(
      "div",
      { id: "island-clip" },
      this.greetingCanvas,
      this.uploadCanvas.el,
      this.contentEl,
    );
    // Same inputs as the GNOME extension's pill (its defaults): click plays or
    // pauses, double-click / right-click open the full view, middle-click raises
    // the player, the wheel changes track.
    this.strip = buildMusicStrip({
      toggle: () => void Bridge.musicControl("playpause"),
      next: () => void Bridge.musicControl("next"),
      previous: () => void Bridge.musicControl("previous"),
      raise: () => void Bridge.musicControl("raise"),
      open: () => this.alert("music"),
    });
    this.islandEl = h(
      "div",
      { id: "island" },
      this.clipEl,
      this.botGlow,
      this.botCanvas,
      this.strip.el,
      this.miniGrid,
      this.countdown,
    );

    const dpr = Math.min(2, window.devicePixelRatio || 1);
    this.greetingCanvas.width = Math.round(EXPANDED_W * dpr);
    this.greetingCanvas.height = Math.round(150 * dpr);
    this.greetingCanvas.style.width = `${EXPANDED_W}px`;
    this.greetingCanvas.style.height = "150px";

    this.root.append(this.wakeStrip, this.islandEl);
    this.applyGeometry();
  }

  // ── FSM ─────────────────────────────────────────────────────────────────────

  private wireFsm() {
    this.fsm.homeToPetitDelay = State.settings.autoCloseInterval;
    // With the Music pill showing a track, the minimised island stays up.
    this.fsm.keepCompact = () => State.musicStrip();
    this.fsm.onTransition = (from, to) => {
      switch (to) {
        case "hidden":
          this.setMode("hidden");
          break;
        case "petit":
          if (from === "coucou") this.greeting.interrupt();
          else if (from === "hidden") Sound.play("peek");
          this.setMode("compact");
          if (from === "coucou") State.view = State.defaultView();
          if (!this.wasInIsland) this.fsm.mouseLeft();
          break;
        case "home":
          this.expand(State.defaultView());
          if (!this.wasInIsland) {
            this.applyLeaveDelay();
            this.fsm.mouseLeft();
          }
          break;
        case "coucou":
          this.expand("greeting");
          this.greeting.start();
          break;
      }
      State.notify();
    };
  }

  launch() {
    this.fsm.launch();
  }

  // ── Mode / view ─────────────────────────────────────────────────────────────

  private setMode(mode: IslandMode) {
    const prev = State.mode;
    if (mode === prev) return;
    State.mode = mode;
    if (mode === "expanded") Sound.play("open");
    if (prev === "expanded") {
      Sound.play("close");
      State.isPinned = false;
      void Bridge.focusWindow(false);
    }
    if (mode !== "expanded") {
      this.engine.resetMorph();
      // Nothing can be seen of the sequence once the island is shut, and leaving
      // it running would keep the frame loop awake — the island must cost
      // nothing while hidden.
      UploadSeq.deactivate();
    }
    this.updateWindowCollapsed();
    this.animateGeometry(modeOrder(mode) < modeOrder(prev));
    State.notify();
  }

  /** True while the drop sequence owns the island body. */
  private get uploadActive(): boolean {
    return State.mode === "expanded" && UploadSeq.isActive && UPLOAD_VIEWS.has(State.view);
  }

  /** Navigating out of the drop flow ends the sequence, as on macOS. */
  private stopSequenceIfLeaving(view: IslandViewName) {
    if (UploadSeq.isActive && !UPLOAD_VIEWS.has(view)) UploadSeq.deactivate();
  }

  expand(view: IslandViewName) {
    this.stopSequenceIfLeaving(view);
    State.view = view;
    if (State.mode !== "expanded") this.setMode("expanded");
    else this.animateGeometry(false);
    State.lastActivity = performance.now();
    this.homeCollapseAt = null;
    State.notify();
  }

  setView(view: IslandViewName) {
    this.stopSequenceIfLeaving(view);
    if (State.mode !== "expanded") {
      this.fsm.forceHome();
      State.view = view;
      this.animateGeometry(false);
      State.notify();
      return;
    }
    const grew = VIEW_LAYOUTS[view].height >= VIEW_LAYOUTS[State.view].height;
    State.view = view;
    State.lastActivity = performance.now();
    this.animateGeometry(!grew);
    State.notify();
  }

  /** The question card is done, whichever way: back to work, keyboard given back. */
  private closeQuestion() {
    State.pendingQuestion = null;
    State.isPinned = false;
    this.fsm.pinned = false;
    void Bridge.focusWindow(false);
    State.updateTask("integration_claude", "working");
    State.setPillBadge("integration_claude", null);
    this.setView(State.defaultView());
  }

  collapse() {
    State.isPinned = false;
    this.fsm.pinned = false;
    // Drive the state machine rather than the mode: setting the mode behind its
    // back left it thinking the island was still open, and a click on the compact
    // island then did nothing — the island could never be reopened.
    this.fsm.forcePetit();
  }

  /** Alert from the hook server: open on this view. Pinned alerts never auto-close. */
  alert(view: IslandViewName) {
    this.fsm.pinned = State.isPinned;
    this.fsm.forceHome();
    this.expand(view);
    // A message that arrives while the pointer is elsewhere has no "pointer left" to start its
    // timer: start it now, so it is read and then folds away.
    if (view === "note" && !this.wasInIsland && !State.isPinned) {
      this.fsm.homeToPetitDelay = NOTE_LINGER_S;
      this.fsm.mouseLeft();
      this.homeCollapseAt = performance.now() + NOTE_LINGER_S * 1000;
    }
  }

  /**
   * Opened from the keyboard (the toggle shortcut): the pointer is elsewhere, so the short
   * "pointer left" delay of hover-opening would fold it away at once. It keeps the user's
   * auto-close time instead.
   */
  openFromKeyboard() {
    this.alert(State.defaultView());
    if (!this.wasInIsland && !State.isPinned) {
      this.fsm.homeToPetitDelay = State.settings.autoCloseInterval;
      this.fsm.mouseLeft();
      this.homeCollapseAt = performance.now() + State.settings.autoCloseInterval * 1000;
    }
  }

  reveal() {
    this.fsm.reveal();
  }

  /**
   * The Music pill gained or lost a track, or was switched on or off: resize the
   * minimised island to carry it (or to drop it), bring it up if it was hidden,
   * and let it retract again once nothing keeps it there.
   */
  refreshMusicStrip() {
    if (State.mode === "compact") this.animateGeometry(false);
    if (State.musicStrip()) {
      if (State.mode === "hidden") this.fsm.reveal();
    } else {
      this.fsm.settle();
    }
    this.ensureRunning();
    State.notify();
  }

  /** An alert stopped waiting for an answer: let the island auto-close again. */
  dropPin() {
    this.fsm.pinned = false;
  }

  // ── File drop ───────────────────────────────────────────────────────────────

  private onDragDrop(e: { type: string; paths?: string[] }) {
    if (e.type !== "over") void Bridge.log(`drag ${e.type} ${e.paths?.length ?? 0} file(s)`);
    if (State.paused) return;
    switch (e.type) {
      case "enter":
      case "over": {
        if (State.fileDragOver) return;
        State.fileDragOver = true;
        // In a conversation the file belongs to it: the chat stays on screen and takes the file on drop.
        if (this.dropGoesToChat()) {
          this.engine.animateMorph(1);
          break;
        }
        this.engine.animateMorph(1);
        // enterZone must run before the island expands, so the sequence is
        // already active by the time the view becomes `upload`.
        UploadSeq.enterZone(State.mouseInIsland.x, State.mouseInIsland.y);
        this.alert("upload");
        // Waking the island from minimised goes through the home view, which ends the sequence
        // (leaving the drop flow): start it again now that the upload view is up.
        if (!UploadSeq.isActive) UploadSeq.enterZone(State.mouseInIsland.x, State.mouseInIsland.y);
        break;
      }
      case "leave": {
        if (!State.fileDragOver) return;
        State.fileDragOver = false;
        this.engine.animateMorph(0);
        // The island deliberately stays open: the drag session is still alive.
        UploadSeq.exitZone();
        State.notify();
        break;
      }
      case "drop": {
        State.fileDragOver = false;
        const path = e.paths?.[0];
        if (!path) {
          this.engine.animateMorph(0);
          if (!this.dropGoesToChat()) this.setView(State.defaultView());
          return;
        }
        if (this.dropGoesToChat()) this.attachToChat(path);
        else this.swallow(path);
        break;
      }
    }
  }

  /** Straight to the cloud, no model involved: the same upload as the Hub's Shelf. */
  private cloudBusy = false;
  private async sendToCloud(dest: "school" | "drive") {
    const file = State.droppedFile;
    if (!file || this.cloudBusy) return;
    this.cloudBusy = true;
    State.uploadStatus = dest === "school" ? "Sending to the server, then to OneDrive…" : "Uploading to Google Drive…";
    State.notify();
    try {
      const folder = (dest === "school" ? State.settings.schoolFolder : State.settings.driveFolder) || "";
      const r = await Bridge.shelfUpload(file.path, dest, folder);
      State.uploadStatus = `✓ ${r.location}`;
      Sound.play("approve");
    } catch (e) {
      const m = String(e).replace(/^Error:\s*/, "");
      State.uploadStatus = (m === "reauth" ? "The school sign-in expired: sign in again from the Hub, Shelf." : m).slice(0, 70);
      Sound.play("error");
    } finally {
      this.cloudBusy = false;
      State.notify();
    }
  }

  /** The chat with an agent (Claude Code, Jinx, a new session) is open: a dropped file goes into it. */
  private dropGoesToChat(): boolean {
    return State.mode === "expanded" && State.view === "prompt" && State.chatTarget !== "mochi";
  }

  /** Puts the file in the open conversation: it rides with the next message, as with the Shelf's Jinx/Claude buttons. */
  private attachToChat(path: string) {
    const name = path.split(/[\\/]/).pop() || "file";
    State.droppedFile = { name, path };
    State.promptContext = { kind: "file", name, path };
    State.jinxFilePath = null;
    State.agentFilePath = null;
    this.engine.animateMorph(0);
    this.engine.triggerEmote("happy");
    Sound.play("approve");
    State.notify();
    void Bridge.ingestFile(path)
      .then((file) => {
        State.droppedFile = { name: file.name, path: file.path };
        State.promptContext = { kind: "file", name: file.name, path: file.path };
        State.notify();
      })
      .catch((err) => {
        State.droppedFile = null;
        State.promptContext = null;
        State.noteMessage = String(err).replace(/^Error:\s*/, "");
        Sound.play("error");
        this.setView("note");
        window.setTimeout(() => this.setView("prompt"), 2400);
      });
  }

  /**
   * Mochi eats the file. Nothing here waits on the file system: the copy into
   * the inbox runs in the background and swaps the path in when it lands, so a
   * slow disk can never stall the animation — same as FileDropHandler on macOS.
   */
  private swallow(path: string) {
    const name = path.split(/[\\/]/).pop() || "file";
    State.droppedFile = { name, path };
    State.promptContext = { kind: "file", name, path };
    State.chatHistory = [];
    State.jinxHistory = [];
    State.jinxFilePath = null;
    void Bridge.chatReset();
    State.uploadStatus = "";

    UploadSeq.performDrop(State.uploadDuration);
    this.uploadTens = 0;
    this.uploadDone = false;

    this.engine.gulp();
    Sound.play("approve");
    this.engine.triggerEmote("happy");
    this.engine.animateMorph(0);

    State.uploadProgress = 0;
    this.setView("uploading");
    this.ensureRunning();

    void Bridge.ingestFile(path)
      .then((file) => {
        State.droppedFile = { name: file.name, path: file.path };
        State.promptContext = { kind: "file", name: file.name, path: file.path };
        State.notify();
      })
      .catch((err) => {
        UploadSeq.deactivate();
        State.noteMessage = String(err).replace(/^Error:\s*/, "");
        this.engine.animateMorph(0);
        this.setView("note");
        Sound.play("error");
        window.setTimeout(() => this.setView(State.defaultView()), 2400);
      });
  }

  /**
   * Sounds and view changes hung off the canvas timeline: a `tick` every 10 %,
   * the ✓ chime when the bar completes, then `choose` once Mochi has grown back.
   */
  private stepSequence() {
    const since = UploadSeq.sinceDrop();
    if (since == null) return;
    const dur = State.uploadDuration;
    const p = Math.max(0, Math.min(1, (since - PRE_PROGRESS) / dur));

    const tens = Math.floor(p * 10);
    if (tens > this.uploadTens && tens < 10) {
      this.uploadTens = tens;
      Sound.play("tick");
    }

    if (!this.uploadDone && since >= PRE_PROGRESS + dur) {
      this.uploadDone = true;
      Sound.play("approve");
      this.engine.triggerEmote("happy");
    }
    // The extra second is the grow-back, after which the choose card is up.
    if (since >= PRE_PROGRESS + dur + 1 && State.view === "uploading") {
      this.setView("choose");
    }
  }

  // ── Geometry ────────────────────────────────────────────────────────────────

  private targetSize(): { w: number; h: number; r: number } {
    const { w, h } = islandSize(State.mode, State.view, State.chatHistory.length, State.musicStrip());
    const r = State.mode === "expanded" ? EXPANDED_CORNER : ROUNDED_CORNER;
    return { w, h, r };
  }

  private animateGeometry(shrinking: boolean) {
    const { w, h, r } = this.targetSize();
    if (shrinking) {
      this.width.curveTowards(w);
      this.height.curveTowards(h);
      this.radius.curveTowards(r);
    } else {
      this.width.springTo(w);
      this.height.springTo(h);
      this.radius.springTo(r);
    }
    this.ensureRunning();
  }

  private applyGeometry() {
    const w = this.width.value;
    const hh = this.height.value;
    const r = this.radius.value;
    this.islandEl.style.width = `${w}px`;
    this.islandEl.style.height = `${hh}px`;
    this.islandEl.style.borderRadius = `0 0 ${r}px ${r}px`;
    const shift = this.dodgeNow(w);
    this.islandEl.style.transform = `translateX(calc(-50% + ${shift}px))`;
    // These follow the island as it resizes, so they belong here rather than in
    // the state-driven DOM sync.
    this.miniGrid.style.left = `${w - 40 - 14.5}px`;
    this.miniGrid.style.top = `${hh / 2 - 14.5}px`;
    // The Music pill sits between Mochi (left) and the mini grid (right).
    const stripH = Math.max(0, Math.min(30, hh - 8));
    this.strip.el.style.left = "58px";
    this.strip.el.style.width = `${Math.max(0, w - 58 - 68)}px`;
    this.strip.el.style.height = `${stripH}px`;
    this.strip.el.style.top = `${(hh - stripH) / 2}px`;
    this.greetingCanvas.style.left = `${(w - EXPANDED_W) / 2}px`;
    this.uploadCanvas.el.style.left = `${(w - EXPANDED_W) / 2}px`;

    // The mouse area goes to where the island is headed, not where it is: a click on the
    // button it stepped away from must reach the button at once.
    const rect = { x: (PANEL_W - w) / 2 + this.dodgeEnd(w), y: 0, w, h: hh };
    const p = this.pushedRect;
    if (Math.abs(p.x - rect.x) > 0.5 || Math.abs(p.w - rect.w) > 0.5 || Math.abs(p.h - rect.h) > 0.5) {
      this.pushedRect = rect;
      void Bridge.setIslandRect(rect.x, rect.y, rect.w, rect.h);
    }
  }

  /** Island rect in window coordinates (origin top-left of the window). */
  private islandRect(): { x: number; y: number; w: number; h: number } {
    const w = this.width.value;
    const hh = this.height.value;
    return { x: (PANEL_W - w) / 2 + this.dodgeNow(w), y: 0, w, h: hh };
  }

  /** The step aside is never more than the window has room for at this width. */
  private dodgeNow(w: number): number {
    const room = (PANEL_W - w) / 2;
    return clamp(this.dodge.value, -room, room);
  }

  private dodgeEnd(w: number): number {
    const room = (PANEL_W - w) / 2;
    return clamp(this.dodgeTarget, -room, room);
  }

  /** The areas where other windows' buttons are (Linux/X11, from Rust). */
  setObstacles(zones: number[][]) {
    const next = toBoxes(zones);
    // The same areas again (they are repeated every couple of seconds): nothing to do.
    if (JSON.stringify(next) === JSON.stringify(this.zones)) return;
    this.zones = next;
    this.evaluateDodge(performance.now());
  }

  /**
   * Decides, from the pointer and the buttons behind the island, whether it steps
   * aside. Only while minimised: an open island is in use and stays where it is.
   */
  private evaluateDodge(now: number) {
    if (!State.settings.dodgeWindows || State.mode === "hidden") {
      this.dodger.reset();
      this.setDodgeTarget(0);
      return;
    }
    if (State.mode !== "compact") return;
    const w = this.width.value;
    const rest: Box = { x: (PANEL_W - w) / 2, y: 0, w, h: this.height.value };
    this.setDodgeTarget(this.dodger.update(now, this.pointer, rest, this.zones, (PANEL_W - w) / 2));
    // The pointer resting is silence, not an event: look again when the waiting is over.
    if (this.dodgeTimer != null) window.clearTimeout(this.dodgeTimer);
    const wait = this.dodger.pending(now);
    this.dodgeTimer = wait == null ? null : window.setTimeout(() => {
      this.dodgeTimer = null;
      this.evaluateDodge(performance.now());
    }, wait);
  }

  private setDodgeTarget(target: number) {
    if (target === this.dodgeTarget) return;
    void Bridge.log(`dodge ${this.dodgeTarget} -> ${target}`);
    this.dodgeTarget = target;
    this.dodge.curveTowards(target, 170);
    this.ensureRunning();
  }

  // ── Window collapse (hidden → tiny wake strip, zero polling) ────────────────

  private updateWindowCollapsed() {
    if (this.collapseTimer != null) {
      window.clearTimeout(this.collapseTimer);
      this.collapseTimer = null;
    }
    if (State.mode === "hidden") {
      // Let the island finish retracting, then drop the window to the wake strip:
      // from there the OS delivers no cursor events, so nothing polls at all.
      this.collapseTimer = window.setTimeout(() => {
        this.collapseTimer = null;
        if (State.mode !== "hidden") return;
        this.collapsed = true;
        void Bridge.setCollapsed(true);
      }, 420);
    } else if (this.collapsed) {
      // Grow the window back before the island animates open.
      this.collapsed = false;
      void Bridge.setCollapsed(false);
    }
  }

  // ── Input ───────────────────────────────────────────────────────────────────

  private wireInput() {
    // The wake strip is the only thing the OS can hit while the island is hidden.
    this.wakeStrip.addEventListener("mouseenter", () => {
      Sound.resume();
      this.applyLeaveDelay();
      if (State.mode === "hidden") this.fsm.mouseEntered();
    });

    // Using the minimised pill (wheel, click) must not open the menu under the pointer.
    this.islandEl.addEventListener("wheel", () => this.fsm.cancelHoverOpen(), { capture: true, passive: true });
    this.islandEl.addEventListener("mousedown", () => this.fsm.cancelHoverOpen(), { capture: true });

    this.islandEl.addEventListener("mousedown", (e) => {
      Sound.resume();
      State.lastActivity = performance.now();
      if (State.mode !== "expanded") {
        this.fsm.click();
        return;
      }
      if (this.isBotHit(e.clientX, e.clientY)) {
        this.cancelBotHover();
        this.engine.slap();
      }
    });

    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && State.mode === "expanded" && !State.isPinned) this.collapse();
      State.lastActivity = performance.now();
    });

    void onDragDrop((e) => this.onDragDrop(e));

    // Outside Tauri (plain browser) drive the cursor from DOM events so the
    // island can be inspected with `npm run dev`.
    if (!IS_TAURI) this.followPageCursor();
  }

  /**
   * Takes the cursor from the page's own mouse events instead of Rust's poll.
   * Used where the OS has no global cursor position (Wayland): the events only
   * fire while the pointer is over the island, so leaving the window is
   * reported as a cursor far away, which is what the poll would have said.
   */
  followPageCursor() {
    window.addEventListener("mousemove", (e) => this.onCursor(e.clientX, e.clientY));
    window.addEventListener("mouseout", (e) => {
      if (e.relatedTarget == null) this.onCursor(-10_000, -10_000);
    });
    // Some window managers (GNOME's, over its own panel) report the pointer
    // leaving the window as this instead of a mouseout with no target.
    document.documentElement.addEventListener("mouseleave", () => this.onCursor(-10_000, -10_000));
  }

  /**
   * How long the open island waits after the pointer leaves before it folds back.
   * With the Music pill it is something you glance at and leave, so it folds back
   * almost at once — from its own view and from the overview; the chat, Settings
   * and the drop views keep the user's auto-close time.
   */
  private applyLeaveDelay() {
    const hover = State.settings.openOnHover;
    const glance = hover || (State.musicStrip() && (State.view === "music" || State.view === "overview"));
    // A short message (a notification, the end of a Pomodoro period) needs time to be read.
    this.fsm.homeToPetitDelay = State.view === "note" ? NOTE_LINGER_S : glance ? MUSIC_LEAVE_S : State.settings.autoCloseInterval;
    this.fsm.hoverOpenDelay = hover ? State.settings.hoverOpenDelay || HOVER_OPEN_S : null;
  }

  /**
   * The pointer elsewhere on screen (X11 only): turns Mochi's head, nothing more.
   * While the pointer is over the island the page's own events are the source.
   */
  onGaze(x: number, y: number) {
    if (State.mode === "hidden") return;
    this.pointer = { x, y };
    this.evaluateDodge(performance.now());
    if (this.wasInIsland) {
      // The page learns the pointer left from a mouseout, which a quick move off the
      // window does not always deliver: the island then stayed open until the pointer
      // came back and left again. The screen-wide position settles it.
      const rect = this.islandRect();
      const inside =
        x >= rect.x - HIT_MARGIN && x <= rect.x + rect.w + HIT_MARGIN &&
        y >= rect.y - HIT_MARGIN && y <= rect.y + rect.h + HIT_MARGIN;
      if (!inside) this.onCursor(x, y);
      return;
    }
    State.mouse = { x, y };
    this.ensureRunning();
  }

  /** Cursor in window-logical coordinates. */
  onCursor(x: number, y: number) {
    State.mouse = { x, y };
    this.pointer = x < -5000 ? null : { x, y };
    this.evaluateDodge(performance.now());
    const rect = this.islandRect();
    State.mouseInIsland = { x: x - rect.x, y: y - rect.y };

    // Windows sends no cursor position with an OLE drag, so the drop sequence is
    // fed from the Win32 cursor poll instead — it runs throughout the drag.
    if (UploadSeq.isActive && !UploadSeq.dropped) {
      UploadSeq.updateCursor(State.mouseInIsland.x, State.mouseInIsland.y);
    }

    const inIsland =
      x >= rect.x - HIT_MARGIN && x <= rect.x + rect.w + HIT_MARGIN &&
      y >= rect.y - HIT_MARGIN && y <= rect.y + rect.h + HIT_MARGIN;

    if (inIsland && !this.wasInIsland) {
      if (this.fsm.state === "coucou") this.greeting.hover();
      this.applyLeaveDelay();
      this.fsm.mouseEntered();
      this.homeCollapseAt = null;
    }
    if (!inIsland && this.wasInIsland) {
      this.applyLeaveDelay();
      this.fsm.mouseLeft();
      if (this.fsm.state === "home" && !State.isPinned) {
        this.homeCollapseAt = performance.now() + this.fsm.homeToPetitDelay * 1000;
      }
    }
    this.wasInIsland = inIsland;

    // Bot hover → love
    const overBot = State.mode === "expanded" && State.stateOverride == null && this.isBotHit(x, y);
    if (overBot && !this.botHovering) this.botHoverIn(x, y);
    if (!overBot && this.botHovering) this.cancelBotHover();
    this.botHovering = overBot;
    if (this.botHovering) {
      const d = Math.hypot(x - this.botHoverStart.x, y - this.botHoverStart.y);
      if (d > 40) {
        this.botHoverStart = { x, y };
        this.scheduleLove();
      }
    }

    this.ensureRunning();
  }

  private isBotHit(x: number, y: number): boolean {
    const rect = this.islandRect();
    const cx = rect.x + this.botCx.value;
    const cy = rect.y + this.botCy.value;
    const radius = this.botSize.value / 2;
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius;
  }

  private botHoverIn(x: number, y: number) {
    if (performance.now() / 1000 - this.lastLoveTime < 6) return;
    this.botHoverStart = { x, y };
    this.engine.blink();
    this.engine.tgEs = 1.08;
    Sound.play("hover");
    this.scheduleLove();
  }

  private scheduleLove() {
    if (this.botHoverTimer != null) window.clearTimeout(this.botHoverTimer);
    this.botHoverTimer = window.setTimeout(() => {
      this.botHoverTimer = null;
      if (!this.botHovering || State.stateOverride != null) return;
      if (performance.now() / 1000 - this.lastLoveTime < 6) return;
      this.lastLoveTime = performance.now() / 1000;
      this.engine.triggerEmote("love");
      Sound.play("love");
    }, 1900);
  }

  private cancelBotHover() {
    if (this.botHoverTimer != null) window.clearTimeout(this.botHoverTimer);
    this.botHoverTimer = null;
    this.engine.tgEs = 1;
  }

  /** Three slaps → dizzy + confused view for 3.3 s, then back. */
  private handleDizzy() {
    this.prevViewBeforeConfused = State.view;
    State.stateOverride = "dizzy";
    this.engine.setState("dizzy");
    Sound.play("dizzy");
    this.alert("confused");
    if (this.confusedRecovery != null) window.clearTimeout(this.confusedRecovery);
    this.confusedRecovery = window.setTimeout(() => {
      this.confusedRecovery = null;
      State.stateOverride = null;
      this.engine.setState(State.effectiveState);
      if (State.view === "confused") {
        const fallback = State.defaultView();
        this.setView(this.prevViewBeforeConfused === "confused" ? fallback : this.prevViewBeforeConfused);
      }
      this.engine.triggerEmote("happy");
    }, 3300);
  }

  // ── Frame loop ──────────────────────────────────────────────────────────────

  ensureRunning() {
    if (this.running) return;
    this.running = true;
    this.lastFrame = performance.now();
    requestAnimationFrame(this.frame);
  }

  private frame = (nowMs: number) => {
    const dt = Math.min(0.05, (nowMs - this.lastFrame) / 1000);
    this.lastFrame = nowMs;

    this.width.step(dt, nowMs);
    this.height.step(dt, nowMs);
    this.radius.step(dt, nowMs);
    this.dodge.step(dt, nowMs);
    this.applyGeometry();

    if (this.dirty) {
      this.dirty = false;
      this.syncDom();
    }

    this.updateBotTargets();
    this.botCx.step(dt);
    this.botCy.step(dt);
    this.botSize.step(dt);

    const greetingActive = State.mode === "expanded" && State.view === "greeting";
    if (greetingActive) {
      const gctx = this.greetingCanvas.getContext("2d");
      if (gctx) {
        const dpr = Math.min(2, window.devicePixelRatio || 1);
        gctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        this.greeting.draw(gctx);
      }
    } else {
      // Kept running even while the drop canvas is up, so the island's own Mochi
      // is already in the right place the moment the canvas fades out.
      this.drawBot(dt);
    }

    const uploadActive = this.uploadActive;
    if (uploadActive) this.uploadCanvas.draw(UploadSeq.frame(), nowMs / 1000);
    this.uploadCanvas.el.classList.toggle("on", uploadActive);
    this.viewsEl.classList.toggle("hidden-by-upload", uploadActive);

    tickMiniBots(dt);
    this.views.get(State.view)?.tick?.(nowMs);
    const stripShown = State.mode === "compact" && State.musicStrip();
    if (stripShown) this.strip.tick(nowMs);
    if (UploadSeq.isActive) this.stepSequence();
    this.updateCountdown(nowMs);

    // Nothing is drawn while the island is hidden, so nothing may keep the loop
    // alive either. This used to read `... || this.engine.busy || State.mode !==
    // "hidden"`, and engine.busy is permanently true for any state with a
    // looping animation — breathing, ratelimit sweat, sleeping z's, the search
    // sweep — so a hidden island went on burning frames in exactly the states it
    // spends most of its life in. Geometry still has to finish retracting.
    const settling =
      this.width.animating || this.height.animating || this.radius.animating || this.dodge.animating;
    const busy = State.mode === "hidden"
      ? settling
      : settling ||
        !this.botCx.settled || !this.botCy.settled || !this.botSize.settled ||
        greetingActive || this.engine.busy || UploadSeq.isActive;

    if (busy) {
      requestAnimationFrame(this.frame);
    } else if ((stripShown || State.mode === "expanded") && State.music?.status === "Playing") {
      // Only the Music pill's bars and its lyric line are moving (the minimised strip,
      // or the music card inside the open menu): that does not need 60 frames a second,
      // and without this the loop stopped and the open menu froze while the mouse rested.
      const live = State.settings.musicVisualizer === "realtime";
      window.setTimeout(() => requestAnimationFrame(this.frame), live ? STRIP_FRAME_LIVE_MS : STRIP_FRAME_MS);
    } else {
      this.running = false;
      Sound.idle();
    }
  };

  private updateBotTargets() {
    const p = botPosition(State.mode, State.view, this.height.value, State.uploadProgress);
    this.botCx.target = p.cx;
    this.botCy.target = p.cy;
    this.botSize.target = p.diameter / 0.6;

    const greetingActive = State.mode === "expanded" && State.view === "greeting";
    // The drop canvas draws its own Mochi; two of them would overlap.
    const visible = p.opacity > 0 && !greetingActive && !this.uploadActive;
    this.botCanvas.style.opacity = visible ? "1" : "0";

    if (State.mode === "expanded" && State.view !== "uploading" && !greetingActive && !this.uploadActive) {
      const d = p.diameter;
      // In the Music view Mochi glows in the cover's colour.
      const color =
        State.view === "music" && State.music ? State.music.accent : botGlowColor(State.effectiveState);
      this.botGlow.style.display = "block";
      this.botGlow.style.width = `${d * 2.2}px`;
      this.botGlow.style.height = `${d * 2.2}px`;
      this.botGlow.style.left = `${this.botCx.value - d * 1.1}px`;
      this.botGlow.style.top = `${this.botCy.value - d * 1.1}px`;
      this.botGlow.style.background = `radial-gradient(circle, ${color} 0%, transparent 62%)`;
      this.botGlow.style.opacity = String(botGlowOpacity(State.effectiveState));
    } else {
      this.botGlow.style.display = "none";
    }
  }

  private drawBot(dt: number) {
    const size = this.botSize.value;
    const w = Math.max(1, Math.round(size));
    const hCss = w + BOT_OVERHANG;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    if (this.canvasPx !== w) {
      this.canvasPx = w;
      this.botCanvas.width = Math.round(w * dpr);
      this.botCanvas.height = Math.round(hCss * dpr);
      this.botCanvas.style.width = `${w}px`;
      this.botCanvas.style.height = `${hCss}px`;
    }
    this.botCanvas.style.left = `${this.botCx.value - w / 2}px`;
    this.botCanvas.style.top = `${this.botCy.value - BOT_OVERHANG / 2 - hCss / 2}px`;

    const ctx = this.botCanvas.getContext("2d");
    if (!ctx) return;

    const focus = State.focusTask;
    // Mochi takes the colour of the pill in focus. The Music pill's colour is the
    // cover's: it also tints him in its full view, and on the minimised island
    // while a track plays and he has nothing else to say (an alert keeps its own).
    let body = focus?.isIntegration ? hexToRGB(focus.color) : null;
    if (State.music && State.tasks.some((t) => t.id === "integration_music")) {
      const quiet = State.effectiveState === "idle" || State.effectiveState === "sleeping";
      if (State.mode === "expanded" && State.view === "music") body = hexToRGB(State.music.accent);
      else if (State.mode === "compact" && State.musicStrip() && quiet) body = hexToRGB(State.music.accent);
    }
    // In the chat he wears the colour of who he is talking to: Claude Code's orange for a
    // session or a new agent, Jinx's green, and plain white for Mochi's own API chat.
    if (State.mode === "expanded" && State.view === "prompt") {
      const who = State.chatTarget === "jinx" ? JINX_ID : State.chatTarget === "mochi" ? null : "integration_claude";
      const proto = who ? INTEGRATION_AGENTS.find((t) => t.id === who) : null;
      body = proto ? hexToRGB(proto.color) : null;
    }
    this.engine.bodyColor = body;
    this.engine.particleOverhang = BOT_OVERHANG;
    this.engine.lookX = this.lookX();
    this.engine.lookY = this.lookY();
    if (this.engine.morph > 0.3) {
      this.engine.slotHTarget = State.fileDragOver ? 0.2 : 0;
    } else {
      this.engine.slotHTarget = 0;
      if (this.engine.morph < 0.05) {
        this.engine.slotH = 0;
        this.engine.slotHVel = 0;
      }
    }
    this.engine.update(dt);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, hCss);
    this.engine.draw(ctx, w, hCss);
  }

  /** BotCanvasView.lookX / lookY — tanh of the distance to the bot. */
  private lookX(): number {
    const rect = this.islandRect();
    const botScreenX = rect.x + this.botCx.value;
    return Math.tanh((State.mouse.x - botScreenX) / 260);
  }

  private lookY(): number {
    return -Math.tanh((State.mouse.y - this.botCy.value) / 200);
  }

  private updateCountdown(nowMs: number) {
    if (State.mode !== "expanded" || State.isPinned || this.homeCollapseAt == null) {
      this.countdown.style.width = "0px";
      return;
    }
    const autoClose = State.settings.autoCloseInterval;
    const windowS = Math.min(10, autoClose * 0.6);
    const remaining = (this.homeCollapseAt - nowMs) / 1000;
    this.countdown.style.width =
      remaining < windowS ? `${Math.max(0, clamp(remaining / windowS, 0, 1) * 160)}px` : "0px";
  }

  // ── DOM sync ────────────────────────────────────────────────────────────────

  private syncDom() {
    const expanded = State.mode === "expanded";
    const greetingActive = expanded && State.view === "greeting";

    this.contentEl.style.opacity = expanded && !greetingActive ? "1" : "0";
    // Looping CSS animations (the Music title's marquee) must not run while the
    // island is shut: nothing is visible, and the hidden island costs nothing.
    this.contentEl.classList.toggle("idle", !expanded);
    this.contentEl.style.pointerEvents = expanded && !greetingActive ? "auto" : "none";
    this.greetingCanvas.style.display = greetingActive ? "block" : "none";

    this.header.sync();
    for (const [name, view] of this.views) {
      const on = name === State.view;
      view.el.classList.toggle("on", on);
      if (on) view.sync();
    }

    // The chat is the only view with a text field, so it is the only time the
    // island is allowed to take keyboard focus.
    // The chat, and a Hub tool with a text field (Notes), are the only times it is.
    const wantsKeys = State.view === "prompt" || (State.view === "tool" && hubTyping());
    if (this.lastSyncedView !== State.view || this.lastWantsKeys !== wantsKeys) {
      const hadKeys = this.lastWantsKeys;
      this.lastSyncedView = State.view;
      this.lastWantsKeys = wantsKeys;
      if (wantsKeys) {
        void Bridge.focusWindow(true);
        const view = State.view;
        window.setTimeout(() => this.views.get(view)?.focus?.(), 120);
      } else if (hadKeys) {
        void Bridge.focusWindow(false);
      }
    }
    // A Hub tool reads its source only while its panel is on screen.
    syncHubRuntime();

    // cava (the real-time bars) runs only while they can be seen and music plays.
    const musicOnScreen =
      (State.mode === "compact" && State.musicStrip()) ||
      (State.mode === "expanded" &&
        (State.view === "music" || (State.view === "overview" && State.focusId === "integration_music")));
    setBarsWanted(
      State.settings.musicVisualizer === "realtime" &&
        State.music?.status === "Playing" &&
        !State.paused &&
        musicOnScreen,
    );

    // The Music pill on the minimised island
    const showStrip = State.mode === "compact" && State.musicStrip();
    this.strip.el.style.opacity = showStrip ? "1" : "0";
    this.strip.el.style.pointerEvents = showStrip ? "auto" : "none";
    if (State.musicStrip()) this.strip.sync();

    // Compact mini grid
    const showGrid = State.mode === "compact";
    this.miniGrid.style.opacity = showGrid ? "1" : "0";
    if (showGrid) {
      // The pill on show is not repeated as a mini Mochi next to it.
      const others = State.otherTasks.filter((t) => !(showStrip && t.id === "integration_music")).slice(0, 4);
      const key = others.map((t) => t.id).join("|");
      if (this.miniGrid.dataset.key !== key) {
        this.miniGrid.dataset.key = key;
        this.miniGrid.replaceChildren();
        for (const t of others) {
          this.miniGrid.append(createMiniBot(t, 13));
        }
        pruneMiniBots();
      }
    }

    syncMiniBotStates(State.tasks);
    this.engine.setState(State.effectiveState);
  }

  /** Applies settings coming from Rust at boot. */
  applySettings() {
    Sound.setEnabled(State.settings.soundEnabled);
    Sound.setVolume(State.settings.soundVolume);
    Sound.setFinish(State.settings.finishSound, State.settings.finishSoundFile);
    this.fsm.homeToPetitDelay = State.settings.autoCloseInterval;
    State.notify();
  }

  get panelSize() {
    return { w: PANEL_W, h: PANEL_H };
  }

  get chatHeight() {
    return chatPromptHeight(State.chatHistory.length);
  }
}
