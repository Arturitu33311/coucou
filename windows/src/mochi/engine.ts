// Mochi — direct port of NotchBuddy/Sources/App/BotEngine.swift to Canvas 2D.
// Same constants, same tweens, same easings, same particles. The only intentional
// difference is the `happy`/`wink` eye arc, which follows the prototype
// (design/prototype/notch-buddy.html, the visual source of truth) — the Swift
// arc angles produce a different shape.

import { Ease, lerp, type EaseFn } from "../core/anim";
import { Sound } from "../core/sound";
import type { BotEmoteName, BotStateName } from "../core/layout";
import type { Outfit } from "./outfits";
import { drawOutfitBehind, drawOutfitFront, mochiH } from "./wardrobe";

// ── Types ─────────────────────────────────────────────────────────────────────

export type EyeShape =
  | "pill" | "wide" | "dot" | "line" | "flat" | "happy" | "closed"
  | "spiral" | "heart" | "star" | "tired" | "wink" | "cup"
  // The Mac's own eyes for the newer emotes (BotEngine.swift EyeShape).
  | "drowsy" | "sad" | "angry" | "halfLid" | "smug" | "scared" | "curious" | "confused" | "shy"
  | "sparkle" | "laugh" | "teary" | "determined" | "wobbly";

export type BadgeKind = "dots" | "bang" | "question" | "dot";

export interface Badge {
  kind: BadgeKind;
  color: RGB;
}

export type RGB = readonly [number, number, number]; // components 0…1

export type TweenKey = readonly [target: number, durationMs: number, ease: EaseFn];

interface Tween {
  prop: PropKey;
  keys: TweenKey[];
  index: number;
  from: number;
  startMs: number;
  onComplete?: () => void;
}

type PropKey =
  | "yaw" | "pitch" | "roll" | "tilt" | "open" | "sx" | "sy"
  | "oy" | "ox" | "tint" | "morph" | "hands" | "blush" | "es" | "badgeS" | "outfitPresence";

interface BotStateCfg {
  color: RGB;
  tint: number;
  eye: EyeShape;
  badge: Badge | null;
  bounces: boolean;
  scans: boolean;
  breathes: boolean;
  zz: boolean;
  sweat: boolean;
  look: readonly [number, number] | null;
  tilt: number;
}

interface Particle {
  type: "heart" | "star" | "spark" | "sweat" | "z" | "tear" | "question" | "anger" | "puff";
  x: number; y: number; vx: number; vy: number;
  age: number; life: number; rot: number; size: number;
}

// ── Constants (MochiConst / PISTES.mochi) ─────────────────────────────────────

const EYE_W = 0.25;
const EYE_H = 0.27;
const EYE_SP = 0.37;
const EYE_P = -0.12;
const BASE_TOP: RGB = [0.929, 0.929, 0.937]; // #EDEDEF
const BASE_BOTTOM: RGB = [0.769, 0.773, 0.792]; // #C4C5CA
const INK = "rgb(26,20,18)"; // #1A1412
const MINI_INK = "rgb(16,19,26)"; // #10131A

const C = {
  idle: [0.902, 0.914, 0.933] as RGB,
  working: [0.231, 0.62, 1] as RGB,
  thinking: [0.545, 0.361, 0.965] as RGB,
  searching: [0.388, 0.396, 0.949] as RGB,
  approval: [0.961, 0.647, 0.141] as RGB,
  question: [0.133, 0.827, 0.933] as RGB,
  error: [0.957, 0.314, 0.369] as RGB,
  finished: [0.204, 0.831, 0.6] as RGB,
  ratelimit: [0.984, 0.573, 0.235] as RGB,
  sleeping: [0.58, 0.635, 0.722] as RGB,
  dizzy: [0.957, 0.447, 0.714] as RGB,
};

const base = {
  bounces: false, scans: false, breathes: false, zz: false, sweat: false,
  look: null, tilt: 0,
};

export const BOT_STATES: Record<BotStateName, BotStateCfg> = {
  idle: { ...base, color: C.idle, tint: 0, eye: "pill", badge: null },
  working: { ...base, color: C.working, tint: 0.72, eye: "pill", badge: { kind: "dots", color: C.working } },
  thinking: { ...base, color: C.thinking, tint: 0.72, eye: "pill", badge: { kind: "dots", color: C.thinking }, look: [0.55, 0.55] },
  searching: { ...base, color: C.searching, tint: 0.72, eye: "pill", badge: { kind: "dots", color: C.searching }, scans: true },
  approval: { ...base, color: C.approval, tint: 0.78, eye: "wide", badge: { kind: "bang", color: C.approval }, bounces: true },
  question: { ...base, color: C.question, tint: 0.75, eye: "pill", badge: { kind: "question", color: C.question }, tilt: 0.17 },
  error: { ...base, color: C.error, tint: 0.78, eye: "flat", badge: { kind: "dot", color: C.error } },
  finished: { ...base, color: C.finished, tint: 0.35, eye: "happy", badge: { kind: "dot", color: C.finished } },
  ratelimit: { ...base, color: C.ratelimit, tint: 0.72, eye: "tired", badge: { kind: "dot", color: C.ratelimit }, sweat: true },
  sleeping: { ...base, color: C.sleeping, tint: 0.32, eye: "closed", badge: null, breathes: true, zz: true },
  dizzy: { ...base, color: C.dizzy, tint: 0.7, eye: "spiral", badge: null },
};

/** State → sound, as in BotStateCfg.sound. */
export const STATE_SOUND: Partial<Record<BotStateName, string>> = {
  working: "work", thinking: "think", searching: "search", approval: "approval",
  question: "question", error: "error", finished: "finish", ratelimit: "rate",
  sleeping: "sleep", dizzy: "dizzy",
};

/** One dance sway, in seconds: shared with the phone's Mochi (Mochi.kt `beat`). */
const DANCE_BEAT_S = 1.4;

const EMOTE_EYE: Record<BotEmoteName, EyeShape> = {
  love: "heart", surprised: "dot", proud: "star", wink: "wink",
  yawn: "tired", happy: "happy", annoyed: "line", dance: "happy",
  sleepy: "drowsy", confused: "confused", excited: "sparkle", shy: "shy", sad: "sad",
  curious: "curious", bored: "halfLid", scared: "scared", grumpy: "angry", smug: "smug",
  laughing: "laugh", crying: "teary", relieved: "closed", determined: "determined", flustered: "wobbly",
};

/** Hold time (ms) of an emote's "hold" key: never zero or negative, so a short emote cannot stall a tween. */
const holdMs = (duration: number, minus: number) => Math.max(0.016, duration - minus) * 1000;

// ── Small helpers ─────────────────────────────────────────────────────────────

const now = () => performance.now() / 1000;

export function hexToRGB(hex: string): RGB {
  const h = hex.replace("#", "");
  const v = parseInt(h, 16);
  return [((v >> 16) & 255) / 255, ((v >> 8) & 255) / 255, (v & 255) / 255];
}

const rgba = (c: RGB, a = 1) =>
  `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${a})`;

const mix3 = (a: RGB, b: RGB, t: number): RGB => [
  lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t),
];

function roundRectPath(x: CanvasRenderingContext2D, X: number, Y: number, W: number, H: number, R: number) {
  const r = Math.max(0, Math.min(R, W / 2, H / 2));
  x.beginPath();
  x.moveTo(X + r, Y);
  x.arcTo(X + W, Y, X + W, Y + H, r);
  x.arcTo(X + W, Y + H, X, Y + H, r);
  x.arcTo(X, Y + H, X, Y, r);
  x.arcTo(X, Y, X + W, Y, r);
  x.closePath();
}

function heartPath(x: CanvasRenderingContext2D, s: number) {
  x.beginPath();
  x.moveTo(0, s * 0.38);
  x.bezierCurveTo(-s * 1.05, -s * 0.15, -s * 0.5, -s * 0.95, 0, -s * 0.38);
  x.bezierCurveTo(s * 0.5, -s * 0.95, s * 1.05, -s * 0.15, 0, s * 0.38);
  x.closePath();
}

function starPath(x: CanvasRenderingContext2D, ro: number, ri: number) {
  x.beginPath();
  for (let i = 0; i < 10; i++) {
    const r = i % 2 ? ri : ro;
    const a = -Math.PI / 2 + (i * Math.PI) / 5;
    x.lineTo(Math.cos(a) * r, Math.sin(a) * r);
  }
  x.closePath();
}

const FONT = `system-ui, "Segoe UI Variable Text", "Segoe UI", sans-serif`;

// ── Engine ────────────────────────────────────────────────────────────────────

export class BotEngine {
  isMini = false;
  /** Solid body colour for mini bots / integration pills (null = Mochi gradient). */
  bodyColor: RGB | null = null;

  // Animated state (BotEngine `s`)
  yaw = 0; pitch = 0; roll = 0; tilt = 0; open = 1;
  sx = 1; sy = 1; oy = 0; ox = 0;
  tint = 0; morph = 0; hands = 0; blush = 0; es = 1; badgeS = 0;

  // Targets
  tgYaw = 0; tgPitch = 0; tgTilt = 0; tgSy = 1; tgSx = 1; tgEs = 1;

  /** What he is wearing now (see outfits.ts, wardrobe.ts); never drawn on mini bots. */
  outfit: Outfit | null = null;
  /** 0 = off, 1 = on; animated by setOutfit. */
  outfitPresence = 0;
  private outfitTarget: Outfit | null = null;
  /** Turns of the roll in progress (the hats fly off for as long as it lasts). */
  rollTurns = 1;
  /** Hat and pompom lag: -1…1 sideways and up/down, a spring on his head's motion. */
  physDx = 0;
  physDy = 0;
  private physVx = 0;
  private physVy = 0;
  private prevYaw = 0;
  private prevOy = 0;
  private prevRoll = 0;

  /** Extra canvas height above the body so hearts can fly out without clipping. */
  particleOverhang = 0;

  // Mouth spring (fraction of R)
  slotH = 0; slotHTarget = 0; slotHVel = 0; isChewing = false;

  col: RGB = C.idle;
  colT: RGB = C.idle;

  state: BotStateName = "idle";
  cfg: BotStateCfg = BOT_STATES.idle;

  eyeOverride: EyeShape | null = null;
  eyeOverrideUntil = 0;
  permanentEye: EyeShape | null = null;
  permanentEmote: BotEmoteName | null = null;
  miniNextBehavior = 0;
  /** Dictation is on: he leans toward it. */
  listening = false;
  /** Until when he keeps watching the pill after news (seconds, `now()` clock). */
  glanceUntil = 0;

  badge: Badge | null = null;
  private badgeKey = "none";
  private badgeToken = 0;

  private tweens = new Map<PropKey, Tween>();
  private locks = new Set<PropKey>();
  private particles: Particle[] = [];

  lookX = 0;
  lookY = 0;

  lastTime = now();
  private t0 = now() - Math.random() * 5;
  private nextBlink = now() + 1.5 + Math.random() * 2;
  waveUntil = 0;
  waveStart = 0;
  private greetToken = 0;
  private lastAmbient = 0;
  private slapTimes: number[] = [];
  private miniLookTarget = { x: 0, y: 0 };
  private miniLookNextTime = 0;

  /** Fired when three slaps land inside 1.7 s (→ dizzy + confused view). */
  onDizzy: (() => void) | null = null;

  // ── Public API ──────────────────────────────────────────────────────────────

  setState(next: BotStateName, force = false) {
    if (this.state === next && !force) return;
    const prev = this.state;
    this.state = next;
    this.cfg = BOT_STATES[next];
    this.colT = this.cfg.color;
    if (!this.locks.has("tint")) this.tint = this.cfg.tint;
    if (!this.locks.has("tilt")) this.tgTilt = this.cfg.tilt;
    this.setBadge(this.cfg.badge);

    switch (next) {
      case "finished":
        this.doRoll(950, 1);
        setTimeout(() => this.emit("spark", 5), 500);
        break;
      case "error":
        this.anim("ox", [
          [0.08, 50, Ease.out], [-0.08, 70, Ease.inOut],
          [0.05, 70, Ease.inOut], [0, 90, Ease.out],
        ]);
        break;
      case "approval":
        this.anim("oy", [[-0.2, 150, Ease.out], [0, 300, Ease.back]]);
        break;
      case "dizzy":
        this.doRoll(1300, 2);
        break;
      case "question":
        this.blink();
        break;
      case "ratelimit":
        this.emit("sweat", 1);
        break;
      default:
        if (prev !== "idle" || next !== "idle") this.blink();
    }
  }

  setBadge(b: Badge | null) {
    const key = b ? `${b.kind}-${b.color.join(",")}` : "none";
    if (key === this.badgeKey) return;
    this.badgeKey = key;
    const tok = ++this.badgeToken;
    this.anim("badgeS", [[0, 90, Ease.inOut]]);
    setTimeout(() => {
      if (tok !== this.badgeToken) return;
      this.badge = b;
      if (b) this.anim("badgeS", [[1, 280, Ease.back]]);
    }, 100);
  }

  blink() {
    if (this.locks.has("open")) return;
    this.anim("open", [[0.06, 70, Ease.inOut], [1, 130, Ease.out]]);
  }

  squash() {
    this.anim("sy", [[0.78, 70, Ease.out], [1.1, 130, Ease.out], [1, 170, Ease.inOut]]);
    this.anim("sx", [[1.16, 70, Ease.out], [0.95, 130, Ease.out], [1, 170, Ease.inOut]]);
  }

  /** Mailbox swallow — opens the slot, chews, then closes. */
  gulp() {
    this.slotHTarget = 0.42;
    setTimeout(() => {
      this.slotHTarget = 0;
      this.isChewing = true;
      setTimeout(() => { this.isChewing = false; }, 800);
    }, 460);
    this.anim("sy", [[0.78, 80, Ease.out], [1.18, 130, Ease.out], [1, 220, Ease.back]]);
    this.anim("sx", [[1.28, 80, Ease.out], [0.92, 130, Ease.out], [1, 220, Ease.back]]);
    this.blink();
  }

  slap() {
    this.interruptGreet();
    if (this.state === "dizzy") return;
    const t = now();
    this.slapTimes = this.slapTimes.filter((s) => t - s < 1.7);
    this.slapTimes.push(t);
    Sound.play("slap");
    this.squash();
    if (this.slapTimes.length >= 3) {
      this.slapTimes = [];
      this.onDizzy?.();
    } else {
      this.eyeOverride = "line";
      this.eyeOverrideUntil = t + 0.8;
      setTimeout(() => Sound.play("annoyed"), 60);
    }
  }

  /** Puts an outfit on or takes it off, with the Mac's timings (exit 180 ms, enter 350 ms). */
  setOutfit(next: Outfit | null, animated = true) {
    if (next === this.outfitTarget) return;
    this.outfitTarget = next;
    this.tweens.delete("outfitPresence");
    this.locks.delete("outfitPresence");
    if (!animated) {
      this.outfit = next;
      this.outfitPresence = next ? 1 : 0;
    } else if (!next) {
      this.anim("outfitPresence", [[0, 180, Ease.inOut]], () => { this.outfit = null; });
    } else if (!this.outfit) {
      this.outfit = next;
      this.outfitPresence = 0;
      this.anim("outfitPresence", [[1, 350, Ease.inOut]], () => this.squash());
    } else {
      this.anim("outfitPresence", [[0, 180, Ease.inOut]], () => {
        this.outfit = next;
        this.anim("outfitPresence", [[1, 350, Ease.inOut]], () => this.squash());
      });
    }
  }

  /** Is an outfit on him (so the whole body turns rigidly when he rolls)? */
  get wearing(): boolean {
    return !!this.outfit && !this.isMini && this.outfitPresence > 0.05;
  }

  doRoll(durationMs: number, turns: number) {
    this.rollTurns = turns;
    this.roll = 0;
    this.anim("roll", [[Math.PI * 2 * turns, durationMs, Ease.inOut]], () => { this.roll = 0; });
  }

  /** Peek wave — the "coucou". Timings from BotEngine.greet(). */
  greet() {
    const t = now();
    const tok = ++this.greetToken;
    this.waveStart = t + 0.45;
    this.waveUntil = t + 1.55;

    this.eyeOverride = "happy";
    this.eyeOverrideUntil = t + 2.0;
    this.anim("oy", [[-0.06, 220, Ease.out], [0.0, 220, Ease.back]]);

    setTimeout(() => {
      if (this.greetToken !== tok) return;
      this.anim("hands", [[1, 280, Ease.out]]);
      this.anim("sy", [[0.95, 100, Ease.out], [1.0, 260, Ease.back]]);
      this.anim("sx", [[1.04, 100, Ease.out], [1.0, 260, Ease.back]]);
      Sound.play("greet");
    }, 250);

    setTimeout(() => { if (this.greetToken === tok) this.blink(); }, 550);
    setTimeout(() => { if (this.greetToken === tok) this.blink(); }, 1500);
    setTimeout(() => {
      if (this.greetToken !== tok) return;
      this.waveUntil = 0;
      this.anim("hands", [[0, 200, Ease.inOut]]);
    }, 1550);
    setTimeout(() => {
      if (this.greetToken !== tok) return;
      this.eyeOverride = "happy";
      this.eyeOverrideUntil = now() + 0.3;
    }, 1750);
  }

  interruptGreet() {
    if (this.hands <= 0.01 && now() >= this.waveUntil) return;
    this.greetToken++;
    this.waveUntil = 0;
    this.waveStart = 0;
    this.anim("hands", [[0, 150, Ease.inOut]]);
  }

  setListening(on: boolean) {
    this.listening = on;
  }

  /** Watches the pill for `seconds` (news arrived): the phone's Mochi does the same. */
  glance(seconds = 4) {
    this.glanceUntil = now() + seconds;
  }

  setPermanentEmote(emote: BotEmoteName | null) {
    this.permanentEmote = emote;
    if (emote === "wink") {
      this.miniNextBehavior = now() + 0.8 + Math.random() * 1.7;
      return;
    }
    this.permanentEye = emote ? EMOTE_EYE[emote] : null;
    if (this.permanentEye) {
      this.eyeOverride = this.permanentEye;
      this.eyeOverrideUntil = Number.POSITIVE_INFINITY;
    } else if (this.eyeOverrideUntil === Number.POSITIVE_INFINITY) {
      this.eyeOverride = null;
      this.eyeOverrideUntil = 0;
    }
    this.miniNextBehavior = now() + 0.8 + Math.random() * 1.7;
  }

  triggerEmote(emote: BotEmoteName, duration = 1.8) {
    const t = now();
    this.eyeOverride = EMOTE_EYE[emote];
    this.eyeOverrideUntil = t + duration;

    switch (emote) {
      case "love":
        this.anim("blush", [
          [1, 300, Ease.out], [1, (duration - 0.6) * 1000, Ease.lin], [0, 300, Ease.inOut],
        ]);
        this.emit("heart", 4);
        this.anim("oy", [[-0.1, 160, Ease.out], [0, 300, Ease.back]]);
        break;
      case "surprised":
        this.anim("oy", [[-0.3, 140, Ease.out], [0, 380, Ease.back]]);
        this.anim("es", [[1.25, 120, Ease.out], [1, 500, Ease.inOut]]);
        break;
      case "proud":
        this.emit("star", 5);
        this.anim("tilt", [
          [-0.14, 220, Ease.out], [-0.14, (duration - 0.5) * 1000, Ease.lin], [0, 280, Ease.inOut],
        ]);
        this.anim("blush", [
          [0.7, 250, Ease.out], [0.7, (duration - 0.5) * 1000, Ease.lin], [0, 300, Ease.inOut],
        ]);
        break;
      case "wink":
        this.anim("tilt", [
          [0.12, 160, Ease.out], [0.12, (duration - 0.4) * 1000, Ease.lin], [0, 240, Ease.inOut],
        ]);
        break;
      case "yawn":
        this.anim("sy", [[1.12, 500, Ease.inOut], [1, 500, Ease.inOut]]);
        this.anim("sx", [[0.94, 500, Ease.inOut], [1, 500, Ease.inOut]]);
        setTimeout(() => { this.eyeOverride = "closed"; this.emit("z", 2); }, 700);
        break;
      case "happy":
        this.anim("blush", [[0.6, 200, Ease.out], [0, 600, Ease.inOut]]);
        break;
      case "annoyed":
        this.eyeOverride = "line";
        this.eyeOverrideUntil = t + 0.8;
        setTimeout(() => Sound.play("annoyed"), 60);
        break;

      // ── The Mac's newer fifteen (BotEngine.swift triggerEmote), same keys and timings ──
      case "sleepy":
        // Sinks slowly… then jolts awake with a blink, and sinks again.
        this.anim("oy", [
          [0.09, 700, Ease.inOut], [-0.05, 90, Ease.out],
          [0.06, holdMs(duration, 0.79), Ease.inOut], [0, 300, Ease.back],
        ]);
        this.anim("tilt", [
          [0.12, 700, Ease.inOut], [0, 120, Ease.out],
          [0.08, holdMs(duration, 0.82), Ease.inOut], [0, 300, Ease.inOut],
        ]);
        setTimeout(() => { this.blink(); this.emit("z", 2); }, 700);
        break;
      case "confused":
        // Puzzled tilt one way, then the other, with a floating "?".
        this.anim("tilt", [
          [-0.18, 280, Ease.inOut], [0.16, 460, Ease.inOut],
          [0.16, holdMs(duration, 1.1), Ease.lin], [0, 300, Ease.out],
        ]);
        this.anim("yaw", [[0.25, 300, Ease.inOut], [-0.2, 460, Ease.inOut], [0, 400, Ease.inOut]]);
        this.emitAt("question", 0.5, -0.55, { vx: 0.05, vy: -0.25, life: 1.4, size: 0.2 });
        break;
      case "excited":
        // Double hop with stretch, stars and sparks.
        this.squash();
        this.emit("spark", 6);
        this.emit("star", 2);
        this.anim("oy", [[-0.3, 110, Ease.out], [0.02, 150, Ease.inOut], [-0.24, 110, Ease.out], [0, 200, Ease.back]]);
        this.anim("sy", [[1.14, 110, Ease.out], [0.86, 150, Ease.inOut], [1.1, 110, Ease.out], [1, 200, Ease.back]]);
        break;
      case "shy":
        // Looks down and away, shrinks a little, blushes.
        this.anim("yaw", [[-0.55, 260, Ease.out], [-0.55, holdMs(duration, 0.6), Ease.lin], [0, 340, Ease.inOut]]);
        this.anim("pitch", [[0.3, 260, Ease.out], [0.3, holdMs(duration, 0.6), Ease.lin], [0, 340, Ease.inOut]]);
        this.anim("blush", [[1, 260, Ease.out], [1, holdMs(duration, 0.6), Ease.lin], [0, 340, Ease.inOut]]);
        this.anim("sx", [[0.95, 260, Ease.out], [0.95, holdMs(duration, 0.6), Ease.lin], [1, 340, Ease.back]]);
        break;
      case "sad":
        // Droops, squishes, looks down; a single tear.
        this.anim("oy", [[0.12, 380, Ease.inOut], [0.12, holdMs(duration, 0.8), Ease.lin], [0, 420, Ease.back]]);
        this.anim("sy", [[0.92, 380, Ease.inOut], [0.92, holdMs(duration, 0.8), Ease.lin], [1, 420, Ease.back]]);
        this.anim("pitch", [[0.28, 380, Ease.inOut], [0.28, holdMs(duration, 0.8), Ease.lin], [0, 420, Ease.inOut]]);
        this.emitAt("tear", -0.3, 0.12, { vx: -0.08, vy: 0.4, life: 1.0, size: 0.12, delay: 0.45 });
        break;
      case "curious":
        // Leans in, head cocked, eyes widen.
        this.anim("tilt", [[0.16, 240, Ease.out], [0.16, holdMs(duration, 0.55), Ease.lin], [0, 300, Ease.inOut]]);
        this.anim("es", [[1.15, 200, Ease.out], [1.15, holdMs(duration, 0.5), Ease.lin], [1, 300, Ease.inOut]]);
        this.anim("sy", [[1.06, 240, Ease.out], [1, 400, Ease.back]]);
        break;
      case "bored":
        // Long sigh: deflate, a puff of air, slow refill.
        this.anim("oy", [[0.08, 500, Ease.inOut], [0.08, holdMs(duration, 1.0), Ease.lin], [0, 500, Ease.inOut]]);
        this.anim("sy", [[0.9, 500, Ease.inOut], [0.93, holdMs(duration, 1.0), Ease.lin], [1, 500, Ease.inOut]]);
        this.anim("sx", [[1.05, 500, Ease.inOut], [1.03, holdMs(duration, 1.0), Ease.lin], [1, 500, Ease.inOut]]);
        setTimeout(() => this.emitPuff(), 350);
        break;
      case "scared":
        // Jumps back, shivers, cold sweat.
        this.anim("ox", [
          [-0.1, 50, Ease.out], [0.1, 60, Ease.inOut], [-0.08, 55, Ease.inOut],
          [0.06, 55, Ease.inOut], [-0.04, 55, Ease.inOut], [0, 90, Ease.out],
        ]);
        this.anim("oy", [[-0.14, 90, Ease.out], [0, 260, Ease.back]]);
        this.anim("sy", [[1.12, 90, Ease.out], [0.92, 160, Ease.inOut], [1, 200, Ease.back]]);
        this.emit("sweat", 2);
        break;
      case "grumpy":
        // Puffs up, holds it, huffs out.
        this.anim("sx", [[1.08, 140, Ease.out], [1.08, holdMs(duration, 0.4), Ease.lin], [1, 220, Ease.inOut]]);
        this.anim("sy", [[0.93, 140, Ease.out], [0.93, holdMs(duration, 0.4), Ease.lin], [1, 220, Ease.inOut]]);
        this.anim("yaw", [[0.35, 200, Ease.out], [0.35, holdMs(duration, 0.45), Ease.lin], [0, 250, Ease.inOut]]);
        this.emitAnger();
        setTimeout(() => this.emitPuff(), Math.max(0.2, duration - 0.35) * 1000);
        break;
      case "smug":
        // Chin up, slow tilt back, a little satisfied glow.
        this.anim("tilt", [[-0.12, 260, Ease.out], [-0.12, holdMs(duration, 0.56), Ease.lin], [0, 300, Ease.inOut]]);
        this.anim("oy", [[-0.06, 260, Ease.out], [-0.06, holdMs(duration, 0.56), Ease.lin], [0, 300, Ease.back]]);
        this.anim("pitch", [[-0.2, 260, Ease.out], [-0.2, holdMs(duration, 0.56), Ease.lin], [0, 300, Ease.inOut]]);
        this.anim("blush", [[0.4, 260, Ease.out], [0, holdMs(duration, 0.26), Ease.inOut]]);
        break;
      case "laughing": {
        // Shakes with laughter: rapid little bounces, body wobble.
        const bounces: TweenKey[] = [];
        const n = Math.max(3, Math.floor(duration / 0.16));
        for (let i = 0; i < n; i++) {
          const amp = 0.1 * (1 - (i / n) * 0.6);
          bounces.push([-amp, 70, Ease.out], [0, 90, Ease.inOut]);
        }
        this.anim("oy", bounces);
        this.anim("tilt", [[-0.1, 140, Ease.out], [0.08, holdMs(duration, 0.4), Ease.inOut], [0, 260, Ease.inOut]]);
        this.anim("blush", [[0.6, 200, Ease.out], [0.6, holdMs(duration, 0.5), Ease.lin], [0, 300, Ease.inOut]]);
        break;
      }
      case "crying": {
        // Sobs: hitching squashes and a stream of tears.
        const sobs: TweenKey[] = [];
        const n = Math.max(2, Math.floor(duration / 0.45));
        for (let i = 0; i < n; i++) sobs.push([0.9, 120, Ease.out], [1.04, 160, Ease.inOut], [0.96, 170, Ease.inOut]);
        sobs.push([1, 200, Ease.back]);
        this.anim("sy", sobs);
        this.anim("pitch", [[0.2, 300, Ease.out], [0.2, holdMs(duration, 0.6), Ease.lin], [0, 300, Ease.inOut]]);
        this.emitTears(Math.max(4, Math.floor(duration * 4)));
        break;
      }
      case "relieved":
        // Big exhale: rises a little, then settles lower and softer.
        this.anim("sy", [[1.08, 300, Ease.out], [0.92, 500, Ease.inOut], [0.95, holdMs(duration, 1.1), Ease.lin], [1, 300, Ease.back]]);
        this.anim("sx", [[0.96, 300, Ease.out], [1.06, 500, Ease.inOut], [1.03, holdMs(duration, 1.1), Ease.lin], [1, 300, Ease.back]]);
        this.anim("blush", [[0.4, 400, Ease.out], [0, holdMs(duration, 0.4), Ease.inOut]]);
        setTimeout(() => this.emitPuff(), 350);
        break;
      case "determined":
        // Leans forward, plants itself, a burst of sparks.
        this.anim("pitch", [[-0.15, 200, Ease.out], [-0.15, holdMs(duration, 0.45), Ease.lin], [0, 250, Ease.inOut]]);
        this.anim("sy", [[0.88, 120, Ease.out], [1.06, 160, Ease.out], [1.02, holdMs(duration, 0.53), Ease.lin], [1, 250, Ease.inOut]]);
        this.anim("sx", [[1.1, 120, Ease.out], [0.97, 160, Ease.out], [1, 300, Ease.inOut]]);
        setTimeout(() => this.emit("spark", 4), 150);
        break;
      case "flustered": {
        // Flails: fast sideways jitter, blush, sweat.
        const jitter: TweenKey[] = [];
        const n = Math.max(4, Math.floor(duration / 0.12));
        for (let i = 0; i < n; i++) jitter.push([i % 2 === 0 ? 0.05 : -0.05, 60, Ease.inOut]);
        jitter.push([0, 80, Ease.out]);
        this.anim("ox", jitter);
        this.anim("blush", [[1, 150, Ease.out], [1, holdMs(duration, 0.45), Ease.lin], [0, 300, Ease.inOut]]);
        this.emit("sweat", 2);
        break;
      }
    }
  }

  /** One particle at a fixed spot (body-relative units, same as emit()). */
  emitAt(
    type: Particle["type"], x: number, y: number,
    o: { vx?: number; vy?: number; life?: number; size?: number; delay?: number } = {},
  ) {
    this.particles.push({
      type, x, y, vx: o.vx ?? 0, vy: o.vy ?? 0,
      age: -(o.delay ?? 0), life: o.life ?? 1.0,
      rot: Math.random() * Math.PI * 2, size: o.size ?? 0.16,
    });
  }

  /** Tears roll out of both eyes and fall. */
  emitTears(count: number) {
    for (let i = 0; i < count; i++) {
      const sd = i % 2 === 0 ? -1 : 1;
      this.emitAt("tear", sd * 0.3, 0.12, {
        vx: sd * (0.08 + Math.random() * 0.1), vy: 0.45 + Math.random() * 0.2,
        life: 0.9, size: 0.12, delay: i * 0.18,
      });
    }
  }

  /** A sigh cloud drifting out from the side of the face. */
  emitPuff() {
    const side = Math.random() < 0.5 ? 1 : -1;
    this.emitAt("puff", side * 0.5, 0.22, { vx: side * 0.35, vy: -0.12, life: 1.1, size: 0.14 });
  }

  /** A "vein" mark that pops at the top corner of the head. */
  emitAnger() {
    this.emitAt("anger", 0.55, -0.5, { vy: -0.04, life: 0.9, size: 0.2 });
  }

  emit(type: Particle["type"], count: number) {
    for (let i = 0; i < count; i++) {
      const isZ = type === "z";
      this.particles.push({
        type,
        x: (Math.random() - 0.5) * 0.9 + (isZ ? 0.55 : 0),
        y: -0.7 - Math.random() * 0.2,
        vx: (Math.random() - 0.5) * 0.35 + (isZ ? 0.18 : 0),
        vy: -(0.45 + Math.random() * 0.35),
        age: -i * 0.14,
        life: 1.3 + Math.random() * 0.5,
        rot: Math.random() * Math.PI * 2,
        size: 0.15 + Math.random() * 0.08,
      });
    }
  }

  animateMorph(target: number, durationMs?: number) {
    const dur = durationMs ?? (target > 0.5 ? 550 : 650);
    this.anim("morph", [[target, dur, Ease.inOut]]);
  }

  resetMorph() {
    this.tweens.delete("morph");
    this.locks.delete("morph");
    this.morph = 0;
  }

  /** True while anything is still moving — lets the island stop its RAF loop. */
  get busy(): boolean {
    return (
      this.tweens.size > 0 ||
      this.particles.length > 0 ||
      this.cfg.bounces || this.cfg.scans || this.cfg.breathes || this.cfg.zz || this.cfg.sweat ||
      this.isMini ||
      Math.abs(this.tgYaw - this.yaw) > 0.002 ||
      Math.abs(this.tgPitch - this.pitch) > 0.002 ||
      Math.abs(this.tgTilt - this.tilt) > 0.002 ||
      Math.abs(this.tgSy - this.sy) > 0.002 ||
      Math.abs(this.tgSx - this.sx) > 0.002 ||
      Math.abs(this.tgEs - this.es) > 0.002 ||
      this.slotH > 0.001 || Math.abs(this.slotHVel) > 0.001 ||
      Math.abs(this.col[0] - this.colT[0]) > 0.003 ||
      Math.abs(this.col[1] - this.colT[1]) > 0.003 ||
      Math.abs(this.col[2] - this.colT[2]) > 0.003
    );
  }

  // ── Tweens ──────────────────────────────────────────────────────────────────

  anim(prop: PropKey, keys: TweenKey[], onComplete?: () => void) {
    this.tweens.set(prop, {
      prop, keys, index: 0, from: this[prop], startMs: performance.now(), onComplete,
    });
    this.locks.add(prop);
  }

  // ── Update ──────────────────────────────────────────────────────────────────

  update(dt: number) {
    const n = now();
    const nowMs = performance.now();

    for (const tw of [...this.tweens.values()]) {
      const k = tw.keys[tw.index];
      const p = Math.min(1, Math.max(0, (nowMs - tw.startMs) / k[1]));
      this[tw.prop] = tw.from + (k[0] - tw.from) * k[2](p);
      if (p >= 1) {
        tw.from = k[0];
        tw.index += 1;
        tw.startMs = nowMs;
        if (tw.index >= tw.keys.length) {
          this.tweens.delete(tw.prop);
          this.locks.delete(tw.prop);
          tw.onComplete?.();
        }
      }
    }

    const t = n - this.t0;
    let ty = this.lookX * 0.62;
    let tp = this.lookY * 0.5;

    if (this.cfg.look) {
      ty = ty * 0.35 + this.cfg.look[0] * 0.55;
      tp = tp * 0.3 + this.cfg.look[1] * 0.5;
    }
    if (this.cfg.scans) {
      ty = Math.sin(t * 2.6) * 0.6;
      tp = -0.06;
    }
    if (this.state === "sleeping") { ty = 0; tp = -0.14; }
    if (this.state === "dizzy") { ty = Math.sin(t * 9) * 0.25; }

    // Mini bots never follow the mouse — they wander.
    if (this.isMini && !this.cfg.look && !this.cfg.scans && this.state !== "sleeping" && this.state !== "dizzy") {
      if (n > this.miniLookNextTime) {
        this.miniLookTarget = {
          x: -0.88 + Math.random() * 1.76,
          y: -0.55 + Math.random() * 1.0,
        };
        this.miniLookNextTime = n + 0.5 + Math.random() * 1.5;
      }
      ty = this.miniLookTarget.x * 0.62;
      tp = this.miniLookTarget.y * 0.5;
    }

    // Listening (dictation on) leans toward the pill that says so; a glance follows news for a few
    // seconds. Both are what the phone's Mochi does (MochiMood.Listening / Glance).
    if ((this.listening || n < this.glanceUntil) && !this.cfg.scans && this.state !== "sleeping" && this.state !== "dizzy") {
      ty = this.listening ? 0.35 : 0.5;
      tp = -0.04;
    }

    this.tgYaw = ty;
    this.tgPitch = tp;
    this.tgTilt = this.cfg.tilt;

    if (n > this.waveStart && n < this.waveUntil) {
      const wt = n - this.waveStart;
      this.tgTilt = -0.06 + Math.sin(2 * Math.PI * 1.2 * wt) * 0.07;
    }

    // The dance: one sway every 1.4 s, hopping on each half-beat. Same tempo as the phone's Mochi,
    // so a song makes both of them move together.
    const dancing = this.permanentEmote === "dance";
    const danceWave = dancing ? Math.sin((t / DANCE_BEAT_S) * 2 * Math.PI) : 0;
    if (dancing && !this.locks.has("tilt")) this.tgTilt = danceWave * 0.15;
    const bounce = dancing
      ? -Math.abs(danceWave) * 0.09
      : this.cfg.bounces ? -Math.abs(Math.sin(t * 5.2)) * 0.07 : 0;
    const kGen = 1 - Math.pow(0.0008, dt);
    if (!this.locks.has("oy")) this.oy += (bounce - this.oy) * kGen;

    if (this.cfg.breathes) {
      const amp = this.isMini ? 0.07 : 0.035;
      this.tgSy = 1 + Math.sin(t * 1.8) * amp;
      this.tgSx = 1 - Math.sin(t * 1.8) * amp * 0.57;
    } else if (this.isMini) {
      this.tgSy = 1 + Math.sin(t * 2.2) * 0.04;
      this.tgSx = 1 - Math.sin(t * 2.2) * 0.02;
    } else {
      this.tgSy = 1;
      this.tgSx = 1;
    }

    if (this.isMini && n > this.miniNextBehavior) this.doMiniBehaviorLoop();

    const kLook = 1 - Math.pow(0.0025, dt);
    if (!this.locks.has("yaw")) this.yaw += (this.tgYaw - this.yaw) * kLook;
    if (!this.locks.has("pitch")) this.pitch += (this.tgPitch - this.pitch) * kLook;
    if (!this.locks.has("tilt")) this.tilt += (this.tgTilt - this.tilt) * kGen;
    if (!this.locks.has("sy")) this.sy += (this.tgSy - this.sy) * kGen;
    if (!this.locks.has("sx")) this.sx += (this.tgSx - this.sx) * kGen;
    if (!this.locks.has("es")) this.es += (this.tgEs - this.es) * kGen;

    this.col = mix3(this.col, this.colT, 1 - Math.pow(0.002, dt));

    if (n > this.nextBlink) {
      if (this.state !== "sleeping" && this.state !== "dizzy") {
        this.blink();
        if (Math.random() < 0.22) setTimeout(() => this.blink(), 230);
      }
      this.nextBlink = n + 2.2 + Math.random() * 3.2;
    }

    if (this.eyeOverride && n > this.eyeOverrideUntil) {
      this.eyeOverride = this.permanentEye;
      if (this.permanentEye) this.eyeOverrideUntil = Number.POSITIVE_INFINITY;
    }

    if (n - this.lastAmbient > 1.3) {
      this.lastAmbient = n;
      if (this.cfg.zz) this.emit("z", 1);
      if (!this.isMini && this.cfg.sweat && Math.random() < 0.5) this.emit("sweat", 1);
    }

    for (const p of this.particles) p.age += dt;
    this.particles = this.particles.filter((p) => p.age < p.life);

    // Mouth slot spring — ω₀ = 2π/0.25, ζ = 0.6
    const omega = (2 * Math.PI) / 0.25;
    const zeta = 0.6;
    const acc = omega * omega * (this.slotHTarget - this.slotH) - 2 * zeta * omega * this.slotHVel;
    this.slotHVel += acc * dt;
    this.slotH = Math.max(0, this.slotH + this.slotHVel * dt);

    // Hat and pompom lag: a spring pulled by his head's motion (and, while he rolls with an outfit
    // on, by the roll's speed, which flings the hat outwards).
    if (dt > 0 && !this.isMini) {
      const yawVel = (this.yaw - this.prevYaw) / dt;
      const oyVel = (this.oy - this.prevOy) / dt;
      const rollVel = (this.roll - this.prevRoll) / dt;
      const centrifugal = this.wearing ? rollVel * 0.18 : 0;
      const tDx = Math.max(-1, Math.min(1, -yawVel * 0.35 - this.tilt * 2 + centrifugal));
      const tDy = Math.max(-1, Math.min(1, oyVel * 0.5));
      const stiff = 60, damp = 9;
      this.physVx += (stiff * (tDx - this.physDx) - damp * this.physVx) * dt;
      this.physVy += (stiff * (tDy - this.physDy) - damp * this.physVy) * dt;
      this.physDx += this.physVx * dt;
      this.physDy += this.physVy * dt;
    }
    this.prevYaw = this.yaw;
    this.prevOy = this.oy;
    this.prevRoll = this.roll;

    this.lastTime = n;
  }

  private doMiniBehaviorLoop() {
    const n = now();
    switch (this.permanentEmote) {
      case "happy":
        if (this.locks.has("oy")) { this.miniNextBehavior = n + 0.4; return; }
        this.anim("oy", [[-0.3, 120, Ease.out], [0.03, 200, Ease.inOut], [0, 160, Ease.back]]);
        this.anim("sy", [[0.82, 80, Ease.out], [1.18, 130, Ease.out], [0.88, 160, Ease.inOut], [1, 200, Ease.back]]);
        this.anim("sx", [[1.15, 80, Ease.out], [0.88, 130, Ease.out], [1.06, 160, Ease.inOut], [1, 200, Ease.back]]);
        this.miniNextBehavior = n + 2.2 + Math.random() * 1.2;
        break;
      case "annoyed":
        if (this.locks.has("yaw")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("yaw", [
          [-0.65, 50, Ease.out], [0.65, 90, Ease.inOut], [-0.5, 80, Ease.inOut],
          [0.4, 75, Ease.inOut], [-0.2, 70, Ease.inOut], [0, 140, Ease.out],
        ]);
        this.miniNextBehavior = n + 3.0 + Math.random() * 2.5;
        break;
      case "wink":
        this.eyeOverride = "wink";
        this.eyeOverrideUntil = n + 0.55;
        this.anim("tilt", [[0.13, 100, Ease.out], [0.13, 320, Ease.lin], [0, 200, Ease.inOut]]);
        this.miniNextBehavior = n + 2.2 + Math.random() * 2.0;
        break;
      case "love":
        this.emit("heart", 2);
        this.anim("tilt", [[-0.1, 180, Ease.out], [0.1, 340, Ease.inOut], [0, 220, Ease.inOut]]);
        this.miniNextBehavior = n + 2.6 + Math.random() * 1.5;
        break;
      // The Mac's newer fifteen, as they behave on a mini bot (BotEngine.swift doMiniBehaviorLoop).
      case "sleepy":
        if (this.locks.has("oy")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("oy", [[0.08, 480, Ease.inOut], [0, 420, Ease.inOut]]);
        this.emit("z", 1);
        this.miniNextBehavior = n + 3.2 + Math.random() * 1.8;
        break;
      case "confused":
        if (this.locks.has("tilt")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("tilt", [[-0.14, 260, Ease.inOut], [0.14, 420, Ease.inOut], [0, 260, Ease.out]]);
        if (Math.random() < 0.5) this.emitAt("question", 0.5, -0.55, { vy: -0.25, life: 1.2, size: 0.2 });
        this.miniNextBehavior = n + 2.8 + Math.random() * 2.0;
        break;
      case "excited":
        if (this.locks.has("oy")) { this.miniNextBehavior = n + 0.4; return; }
        this.squash();
        this.anim("oy", [[-0.18, 100, Ease.out], [0, 180, Ease.back]]);
        this.miniNextBehavior = n + 1.8 + Math.random() * 1.0;
        break;
      case "shy":
        this.anim("yaw", [[-0.4, 260, Ease.out], [-0.4, 300, Ease.lin], [0, 280, Ease.inOut]]);
        this.anim("blush", [[0.5, 260, Ease.out], [0, 500, Ease.inOut]]);
        this.miniNextBehavior = n + 3.0 + Math.random() * 2.0;
        break;
      case "sad":
        if (this.locks.has("oy")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("oy", [[0.1, 500, Ease.inOut], [0, 600, Ease.inOut]]);
        if (Math.random() < 0.35) this.emitAt("tear", -0.3, 0.12, { vx: -0.08, vy: 0.4, life: 1.0, size: 0.12, delay: 0.3 });
        this.miniNextBehavior = n + 3.6 + Math.random() * 2.2;
        break;
      case "curious":
        if (this.locks.has("tilt")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("tilt", [[0.12, 220, Ease.out], [0.12, 380, Ease.lin], [0, 240, Ease.inOut]]);
        this.anim("es", [[1.1, 220, Ease.out], [1, 400, Ease.inOut]]);
        this.miniNextBehavior = n + 2.4 + Math.random() * 1.6;
        break;
      case "bored":
        if (this.locks.has("sy")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("sy", [[0.94, 600, Ease.inOut], [1, 700, Ease.inOut]]);
        this.emitPuff();
        this.miniNextBehavior = n + 3.8 + Math.random() * 2.4;
        break;
      case "scared":
        if (this.locks.has("ox")) { this.miniNextBehavior = n + 0.4; return; }
        this.anim("ox", [[0.05, 45, Ease.inOut], [-0.05, 60, Ease.inOut], [0.03, 55, Ease.inOut], [0, 70, Ease.out]]);
        this.emit("sweat", 1);
        this.miniNextBehavior = n + 1.8 + Math.random() * 1.4;
        break;
      case "grumpy":
        if (this.locks.has("sx")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("sx", [[1.05, 90, Ease.out], [1, 220, Ease.inOut]]);
        this.anim("sy", [[0.95, 90, Ease.out], [1, 220, Ease.inOut]]);
        if (Math.random() < 0.5) this.emitAnger(); else this.emitPuff();
        this.miniNextBehavior = n + 3.2 + Math.random() * 2.0;
        break;
      case "smug":
        this.anim("tilt", [[-0.1, 300, Ease.out], [-0.1, 400, Ease.lin], [0, 260, Ease.inOut]]);
        this.anim("oy", [[-0.04, 300, Ease.out], [0, 400, Ease.back]]);
        this.miniNextBehavior = n + 2.8 + Math.random() * 2.0;
        break;
      case "laughing":
        if (this.locks.has("oy")) { this.miniNextBehavior = n + 0.4; return; }
        this.anim("oy", [
          [-0.08, 70, Ease.out], [0, 90, Ease.inOut], [-0.07, 70, Ease.out],
          [0, 90, Ease.inOut], [-0.05, 70, Ease.out], [0, 120, Ease.back],
        ]);
        this.miniNextBehavior = n + 1.6 + Math.random() * 1.4;
        break;
      case "crying":
        if (this.locks.has("sy")) { this.miniNextBehavior = n + 0.4; return; }
        this.anim("sy", [[0.9, 120, Ease.out], [1.04, 160, Ease.inOut], [1, 200, Ease.back]]);
        this.emitTears(2);
        this.miniNextBehavior = n + 1.4 + Math.random() * 1.0;
        break;
      case "relieved":
        if (this.locks.has("sy")) { this.miniNextBehavior = n + 0.5; return; }
        this.anim("sy", [[1.05, 400, Ease.inOut], [0.95, 600, Ease.inOut], [1, 500, Ease.inOut]]);
        setTimeout(() => this.emitPuff(), 450);
        this.miniNextBehavior = n + 3.4 + Math.random() * 2.0;
        break;
      case "determined":
        if (this.locks.has("sy")) { this.miniNextBehavior = n + 0.4; return; }
        this.anim("sy", [[0.9, 100, Ease.out], [1.05, 150, Ease.out], [1, 200, Ease.inOut]]);
        this.miniNextBehavior = n + 2.6 + Math.random() * 1.6;
        break;
      case "flustered":
        if (this.locks.has("ox")) { this.miniNextBehavior = n + 0.4; return; }
        this.anim("ox", [[0.04, 55, Ease.inOut], [-0.04, 55, Ease.inOut], [0.04, 55, Ease.inOut], [-0.03, 55, Ease.inOut], [0, 70, Ease.out]]);
        this.anim("blush", [[0.8, 150, Ease.out], [0, 700, Ease.inOut]]);
        this.miniNextBehavior = n + 2.0 + Math.random() * 1.5;
        break;
      default:
        this.miniNextBehavior = n + 3.0 + Math.random() * 2.0;
    }
  }

  // ── Draw ────────────────────────────────────────────────────────────────────

  /**
   * Draws hands, body, blush, eyes, mouth, badge and particles into a canvas of
   * `w`×`h` CSS pixels (the caller has already applied the DPR transform).
   */
  draw(x: CanvasRenderingContext2D, W: number, H: number) {
    const R = W * 0.3;
    const rx = R * 1.14;
    const ry = R * 0.88;
    const cx = W / 2 + this.ox * R;
    const cy = H / 2 + this.particleOverhang / 2 + this.oy * R + R * 0.06;

    // With an outfit on, a roll turns the whole body and its accessories rigidly; the badge and
    // the particles stay upright (drawn after).
    const rigid = this.wearing && Math.abs(this.roll) > 0.001;
    if (rigid) {
      x.save();
      x.translate(cx, cy);
      x.rotate(this.roll);
      x.translate(-cx, -cy);
    }
    const wardrobe = this.wearing && this.outfit
      ? {
          outfit: this.outfit,
          H: mochiH(R, this.yaw, this.pitch, this.physDx, this.physDy, this.outfitPresence > 0.05 ? 0 : this.roll),
          cx, cy, tilt: this.tilt, sx: this.sx, sy: this.sy, morph: this.morph,
          presence: this.outfitPresence, rollTurns: this.rollTurns, t: now() - this.t0,
        }
      : null;

    this.drawHandsBehind(x, R, rx, ry, cx, cy);
    if (wardrobe) drawOutfitBehind(x, wardrobe);

    x.save();
    x.translate(cx, cy);
    if (this.tilt !== 0) x.rotate(this.tilt);
    x.scale(this.sx, this.sy);

    const body = this.bodyPath(rx, ry, R);
    this.drawBody(x, body, R, rx, ry);

    const blushVal = Math.max(this.blush, this.tint * 0.5) * (1 - this.morph);
    if (blushVal > 0.01) {
      x.save();
      x.clip(body);
      const yOffset = Math.sin(this.yaw) * rx * 0.8;
      x.fillStyle = `rgba(255,120,150,${0.5 * blushVal})`;
      for (const sd of [-1, 1]) {
        x.beginPath();
        x.ellipse(sd * rx * 0.55 + yOffset, ry * 0.2, R * 0.17, R * 0.1, 0, 0, Math.PI * 2);
        x.fill();
      }
      x.restore();
    }

    this.drawEyes(x, body, R, rx, ry);
    if (this.morph > 0.05) this.drawMouth(x, body, R);

    x.restore();
    if (wardrobe) drawOutfitFront(x, wardrobe);
    if (rigid) x.restore();

    if (this.badge && this.badgeS > 0.01 && this.morph < 0.25) {
      this.drawBadge(x, this.badge, R, cx, cy);
    }
    this.drawParticles(x, R, cx, cy);
  }

  private bodyPath(rx: number, ry: number, R: number): Path2D {
    const n = 72;
    const expN = 2.0 / 2.7;
    const tw = R * 1.0;
    const th = R * 0.94;
    const tr = R * 0.42;
    const p = new Path2D();
    const m = this.morph;
    for (let i = 0; i <= n; i++) {
      const a = (i / n) * Math.PI * 2;
      const ca = Math.cos(a);
      const sa = Math.sin(a);
      const px0 = rx * (ca >= 0 ? Math.pow(ca, expN) : -Math.pow(-ca, expN));
      const py0 = ry * (sa >= 0 ? Math.pow(sa, expN) : -Math.pow(-sa, expN));
      let px = px0;
      let py = py0;
      if (m >= 0.005) {
        const rr = rrPoint(ca, sa, tw, th, tr);
        px = lerp(px0, rr.x, m);
        py = lerp(py0, rr.y, m);
      }
      if (i === 0) p.moveTo(px, py);
      else p.lineTo(px, py);
    }
    p.closePath();
    return p;
  }

  private drawBody(x: CanvasRenderingContext2D, body: Path2D, R: number, rx: number, ry: number) {
    // The pumpkin outfit recolours the whole body, whatever colour the island gave him.
    const pumpkin = this.outfit === "pumpkin" && !this.isMini && this.outfitPresence > 0.05;
    if (this.bodyColor && !pumpkin) {
      // Mini bots: flat solid fill — no gradient, no reflection, no highlight
      x.fillStyle = rgba(this.bodyColor, 1);
      x.fill(body);
      return;
    }
    const g = x.createLinearGradient(rx * 0.7, -ry * 0.85, -rx * 0.8, ry * 0.9);
    g.addColorStop(0, pumpkin ? "#FFA94D" : rgba(BASE_TOP));
    g.addColorStop(1, pumpkin ? "#E8590C" : rgba(BASE_BOTTOM));
    x.fillStyle = g;
    x.fill(body);

    const effectiveTint = this.tint * (1 - this.morph);
    if (effectiveTint > 0.01) {
      const tg = x.createLinearGradient(0, ry, 0, -ry);
      tg.addColorStop(0, rgba(this.col, 0.72 * effectiveTint));
      tg.addColorStop(1, rgba(this.col, 0));
      x.fillStyle = tg;
      x.fill(body);
    }

    const sh = x.createRadialGradient(0, 0, R * 0.15, 0, 0, R * 1.25);
    sh.addColorStop(0, "rgba(0,0,0,0)");
    sh.addColorStop(0.6, "rgba(0,0,0,0)");
    sh.addColorStop(1, "rgba(0,0,0,0.2)");
    x.fillStyle = sh;
    x.fill(body);

    const hl = x.createRadialGradient(rx * 0.34, -ry * 0.46, 0, rx * 0.34, -ry * 0.46, R * 0.42);
    hl.addColorStop(0, "rgba(255,255,255,0.55)");
    hl.addColorStop(1, "rgba(255,255,255,0)");
    x.fillStyle = hl;
    x.fill(body);
  }

  private drawEyes(x: CanvasRenderingContext2D, body: Path2D, R: number, rx: number, ry: number) {
    let shape: EyeShape = this.eyeOverride ?? this.cfg.eye;
    if (this.morph > 0.5) {
      if (this.isChewing) shape = "happy";
      else if (this.slotHTarget > 0.05 || this.slotH > 0.1) shape = "cup";
    }

    x.save();
    x.clip(body);
    const ink = this.isMini ? MINI_INK : INK;
    x.fillStyle = ink;
    x.strokeStyle = ink;

    for (const sd of [-1, 1]) {
      const eyeYaw = sd * EYE_SP + this.yaw;
      // With an outfit on, the whole body turns instead (see draw), so the eyes add no roll.
      let eyePitch = EYE_P + this.pitch + (this.wearing ? 0 : this.roll);
      eyePitch = (((eyePitch + Math.PI) % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2) - Math.PI;
      const cp = Math.cos(eyePitch);
      if (Math.cos(eyeYaw) * cp <= 0.04) continue;

      const ex = Math.sin(eyeYaw) * cp * rx;
      const ey = -Math.sin(eyePitch) * ry + (this.morph > 0 ? ry * 0.14 * this.morph : 0);
      const fx = lerp(Math.max(0.18, Math.cos(eyeYaw)), 1, this.morph * 0.7);
      const fy = lerp(Math.max(0.18, cp), 1, this.morph * 0.7);
      const eyeMult = this.isMini ? 1.9 : 1.0;
      const ew = R * EYE_W * this.es * eyeMult;
      const eh = R * EYE_H * this.es * eyeMult;

      x.save();
      x.translate(ex, ey);
      x.scale(fx, fy);
      this.drawEyeShape(x, shape, ew, eh, sd, ink);
      x.restore();
    }
    x.restore();
  }

  private drawEyeShape(
    x: CanvasRenderingContext2D, shape: EyeShape,
    w: number, h: number, sd: number, ink: string,
  ) {
    const t = now();
    switch (shape) {
      case "wide":
        this.drawEyeShape(x, "pill", w * 1.16, h * 1.12, sd, ink);
        break;
      case "pill": {
        const hh = Math.max(h * this.open, w * 0.3);
        roundRectPath(x, -w / 2, -hh / 2, w, hh, Math.min(w / 2, hh / 2));
        x.fill();
        break;
      }
      case "dot":
        x.beginPath();
        x.arc(0, 0, w * 0.45, 0, Math.PI * 2);
        x.fill();
        break;
      case "line":
        x.rotate(-sd * 0.2);
        roundRectPath(x, -w * 0.78, -w * 0.21, w * 1.56, w * 0.42, w * 0.21);
        x.fill();
        break;
      case "flat":
        roundRectPath(x, -w * 0.72, -w * 0.2, w * 1.44, w * 0.4, w * 0.2);
        x.fill();
        break;
      case "happy":
        x.lineWidth = w * 0.5;
        x.lineCap = "round";
        x.beginPath();
        x.arc(0, h * 0.18, w * 0.82, Math.PI * 1.12, Math.PI * 1.88);
        x.stroke();
        break;
      case "closed":
        x.lineWidth = w * 0.36;
        x.lineCap = "round";
        x.beginPath();
        x.arc(0, -h * 0.08, w * 0.78, Math.PI * 0.15, Math.PI * 0.85);
        x.stroke();
        break;
      case "spiral": {
        x.lineWidth = w * 0.22;
        x.lineCap = "round";
        x.beginPath();
        for (let a = 0; a < 4.4 * Math.PI; a += 0.2) {
          const r = w * 0.06 + a * w * 0.058;
          const aa = a + t * 9 * sd;
          const px = Math.cos(aa) * r;
          const py = Math.sin(aa) * r;
          if (a === 0) x.moveTo(px, py);
          else x.lineTo(px, py);
        }
        x.stroke();
        break;
      }
      case "heart":
        x.fillStyle = "#FF4D6D";
        heartPath(x, w * 1.2);
        x.fill();
        x.fillStyle = ink;
        break;
      case "star":
        x.fillStyle = "#F7B32B";
        x.rotate(t * 1.5 * sd);
        starPath(x, w * 1.05, w * 0.46);
        x.fill();
        x.fillStyle = ink;
        break;
      case "tired":
        roundRectPath(x, -w / 2, -h * 0.02, w, h * 0.38, w / 2);
        x.fill();
        roundRectPath(x, -w * 0.62, -h * 0.1, w * 1.24, w * 0.22, w * 0.11);
        x.fill();
        break;
      case "wink":
        if (sd < 0) {
          const hh = Math.max(h * this.open, w * 0.3);
          roundRectPath(x, -w / 2, -hh / 2, w, hh, Math.min(w / 2, hh / 2));
          x.fill();
        } else {
          x.lineWidth = w * 0.5;
          x.lineCap = "round";
          x.beginPath();
          x.arc(0, h * 0.18, w * 0.82, Math.PI * 1.12, Math.PI * 1.88);
          x.stroke();
        }
        break;
      // ── The Mac's own eyes for the newer emotes (BotEngine.swift drawEyeShape) ──
      case "drowsy": {
        // Heavy lid that sinks and recovers, like fighting sleep.
        const hh = Math.max(h * this.open, w * 0.3);
        const lidY = -hh / 2 + hh * (0.48 + 0.16 * Math.sin(t * 1.3));
        x.save();
        this.lidClip(x, sd, w, lidY, lidY);
        roundRectPath(x, -w / 2, -hh / 2, w, hh, Math.min(w / 2, hh / 2));
        x.fill();
        x.restore();
        this.eyeLine(x, -w * 0.62, lidY, w * 0.62, lidY, w * 0.16);
        break;
      }
      case "sad": {
        // Outer corners droop, glossy highlight.
        const hh = Math.max(h * 0.88 * this.open, w * 0.3);
        x.save();
        this.lidClip(x, sd, w, -hh / 2 + hh * 0.08, -hh / 2 + hh * 0.55);
        roundRectPath(x, -w / 2, -hh / 2, w, hh, Math.min(w / 2, hh / 2));
        x.fill();
        x.fillStyle = "rgba(255,255,255,0.85)";
        x.beginPath();
        x.arc(-sd * w * 0.14, -hh * 0.08, w * 0.12, 0, Math.PI * 2);
        x.fill();
        x.restore();
        break;
      }
      case "angry": {
        // Steep inward V: scowl without brows.
        const hh = Math.max(h * 0.9 * this.open, w * 0.3);
        x.save();
        this.lidClip(x, sd, w, -hh / 2 + hh * 0.62, -hh / 2 + hh * 0.08);
        roundRectPath(x, -w * 0.525, -hh / 2, w * 1.05, hh, Math.min(w * 0.525, hh / 2));
        x.fill();
        x.restore();
        break;
      }
      case "halfLid": {
        // Flat, unimpressed lids at half height.
        const hh = Math.max(h * this.open, w * 0.3);
        const lidY = -hh / 2 + hh * 0.5;
        x.save();
        this.lidClip(x, sd, w, lidY, lidY);
        roundRectPath(x, -w * 0.525, -hh / 2, w * 1.05, hh, Math.min(w * 0.525, hh / 2));
        x.fill();
        x.restore();
        this.eyeLine(x, -w * 0.6, lidY, w * 0.6, lidY, w * 0.14);
        break;
      }
      case "smug": {
        // Lazy lids + side-eye (both eyes glance the same way).
        const hh = Math.max(h * this.open, w * 0.3);
        x.translate(w * 0.14, 0);
        const inner = -hh / 2 + hh * 0.4, outer = -hh / 2 + hh * 0.58;
        x.save();
        this.lidClip(x, sd, w, inner, outer);
        roundRectPath(x, -w / 2, -hh / 2, w, hh, Math.min(w / 2, hh / 2));
        x.fill();
        x.restore();
        this.eyeLine(x, -sd * w * 0.6, inner - (outer - inner) * 0.1, sd * w * 0.6, outer + (outer - inner) * 0.1, w * 0.14);
        break;
      }
      case "scared": {
        // Tall trembling eyes with a small catch-light.
        x.translate(Math.sin(t * 42) * w * 0.05, 0);
        const hh = Math.max(h * 1.3 * this.open, w * 0.3);
        roundRectPath(x, -w * 0.475, -hh / 2, w * 0.95, hh, Math.min(w * 0.475, hh / 2));
        x.fill();
        if (this.open > 0.5) {
          x.fillStyle = "rgba(255,255,255,0.9)";
          x.beginPath();
          x.arc(-w * 0.15, -hh * 0.32 + w * 0.15, w * 0.15, 0, Math.PI * 2);
          x.fill();
          x.fillStyle = ink;
        }
        break;
      }
      case "curious": {
        // One eye bigger and raised: the "hm?" look.
        const k = sd > 0 ? 1.22 : 0.88;
        if (sd > 0) x.translate(0, -h * 0.07);
        const hh = Math.max(h * k * this.open, w * 0.3);
        roundRectPath(x, -(w * k) / 2, -hh / 2, w * k, hh, Math.min((w * k) / 2, hh / 2));
        x.fill();
        break;
      }
      case "confused":
        // "o -": one round eye, one tilted dash.
        if (sd < 0) {
          x.beginPath();
          x.arc(0, 0, w * 0.42, 0, Math.PI * 2);
          x.fill();
        } else {
          x.rotate(0.22);
          roundRectPath(x, -w * 0.66, -w * 0.18, w * 1.32, w * 0.36, w * 0.18);
          x.fill();
        }
        break;
      case "shy": {
        // Small eyes, gaze down and to the side.
        x.translate(-w * 0.16, h * 0.2);
        const hh = Math.max(w * 0.6 * this.open, w * 0.18);
        x.beginPath();
        x.ellipse(0, -w * 0.3 * this.open + hh / 2, w * 0.28, hh / 2, 0, 0, Math.PI * 2);
        x.fill();
        break;
      }
      case "sparkle": {
        // Big glossy eyes with twinkling highlights.
        const hh = Math.max(h * 1.12 * this.open, w * 0.3);
        roundRectPath(x, -w * 0.54, -hh / 2, w * 1.08, hh, Math.min(w * 0.54, hh / 2));
        x.fill();
        if (this.open > 0.5) {
          const tw = 1 + 0.14 * Math.sin(t * 6 + sd);
          const r1 = w * 0.2 * tw, r2 = w * 0.09 * (2 - tw);
          x.fillStyle = "#fff";
          x.beginPath();
          x.arc(-w * 0.14, -hh * 0.22, r1, 0, Math.PI * 2);
          x.fill();
          x.fillStyle = "rgba(255,255,255,0.9)";
          x.beginPath();
          x.arc(w * 0.16, hh * 0.18, r2, 0, Math.PI * 2);
          x.fill();
          x.fillStyle = ink;
        }
        break;
      }
      case "laugh":
        // "> <" squeezed shut.
        x.lineWidth = w * 0.32;
        x.lineCap = "round";
        x.lineJoin = "round";
        x.beginPath();
        x.moveTo(sd * w * 0.45, -h * 0.32);
        x.lineTo(-sd * w * 0.35, 0);
        x.lineTo(sd * w * 0.45, h * 0.32);
        x.stroke();
        break;
      case "teary": {
        // Eyes welling up: a wobbling water line inside the pill.
        const hh = Math.max(h * this.open, w * 0.3);
        x.save();
        roundRectPath(x, -w / 2, -hh / 2, w, hh, Math.min(w / 2, hh / 2));
        x.fill();
        x.clip();
        const waterY = hh * 0.05 + Math.sin(t * 5 + sd) * h * 0.04;
        x.fillStyle = "rgba(124,199,255,0.85)";
        x.beginPath();
        x.moveTo(-w, waterY);
        x.quadraticCurveTo(0, waterY - h * 0.1, w, waterY);
        x.lineTo(w, hh);
        x.lineTo(-w, hh);
        x.closePath();
        x.fill();
        x.fillStyle = "rgba(255,255,255,0.9)";
        x.beginPath();
        x.arc(-w * 0.16, -hh * 0.34 + w * 0.14, w * 0.14, 0, Math.PI * 2);
        x.fill();
        x.restore();
        break;
      }
      case "determined": {
        // Focused: lowered inner lid + a firm brow.
        const hh = Math.max(h * 0.95 * this.open, w * 0.3);
        x.save();
        this.lidClip(x, sd, w, -hh / 2 + hh * 0.36, -hh / 2 + hh * 0.1);
        roundRectPath(x, -w / 2, -hh / 2, w, hh, Math.min(w / 2, hh / 2));
        x.fill();
        x.restore();
        this.eyeLine(x, -sd * w * 0.55, -hh / 2 - h * 0.06, sd * w * 0.62, -hh / 2 - h * 0.32, w * 0.22);
        break;
      }
      case "wobbly": {
        // "~" flustered squiggle.
        x.lineWidth = w * 0.26;
        x.lineCap = "round";
        x.lineJoin = "round";
        x.beginPath();
        const steps = 16;
        for (let i = 0; i <= steps; i++) {
          const u = i / steps;
          const px = -w * 0.7 + u * w * 1.4;
          const py = Math.sin(u * Math.PI * 2 + t * 8 * sd) * h * 0.12;
          if (i === 0) x.moveTo(px, py);
          else x.lineTo(px, py);
        }
        x.stroke();
        break;
      }
      case "cup": {
        // Flat top, rounded bottom corners (U shape) — used while the box is open
        const hh = Math.max(h * this.open, w * 0.3);
        const cr = Math.min(w / 2, hh / 2);
        x.beginPath();
        x.moveTo(-w / 2, -hh / 2);
        x.lineTo(w / 2, -hh / 2);
        x.lineTo(w / 2, hh / 2 - cr);
        x.quadraticCurveTo(w / 2, hh / 2, w / 2 - cr, hh / 2);
        x.lineTo(-w / 2 + cr, hh / 2);
        x.quadraticCurveTo(-w / 2, hh / 2, -w / 2, hh / 2 - cr);
        x.closePath();
        x.fill();
        break;
      }
    }
  }

  /**
   * Clips to the region under an eyelid line running from the eye's inner corner (toward the face
   * centre) to its outer corner. y grows downward, so a larger y is a lower lid. Wrap in save/restore.
   */
  private lidClip(x: CanvasRenderingContext2D, sd: number, w: number, innerY: number, outerY: number) {
    const d = outerY - innerY;
    x.beginPath();
    x.moveTo(-sd * w * 1.5, innerY - 0.25 * d);
    x.lineTo(sd * w * 1.5, outerY + 0.25 * d);
    x.lineTo(sd * w * 1.5, w * 4);
    x.lineTo(-sd * w * 1.5, w * 4);
    x.closePath();
    x.clip();
  }

  /** A round-capped stroke in the eye's ink. */
  private eyeLine(x: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number, width: number) {
    x.lineWidth = width;
    x.lineCap = "round";
    x.beginPath();
    x.moveTo(x0, y0);
    x.lineTo(x1, y1);
    x.stroke();
  }

  /** Mailbox slot: dark pill cut into the box face, with rim and lip highlights. */
  private drawMouth(x: CanvasRenderingContext2D, body: Path2D, R: number) {
    const m = this.morph;
    const hW = R * 1.8 * m;
    const hH = this.slotH * R * m;
    const hX = -hW / 2;
    const boxTop = -R * (0.88 + 0.06 * m);
    const hY = boxTop + R * 0.08 * m;

    x.save();
    x.clip(body);

    x.strokeStyle = `rgba(255,255,255,${0.55 * m})`;
    x.lineWidth = 1;
    x.lineCap = "round";
    x.beginPath();
    x.moveTo(-R * 0.9 * m, boxTop + 1);
    x.lineTo(R * 0.9 * m, boxTop + 1);
    x.stroke();

    if (hH > 0.8) {
      const hR = Math.min(hW / 2, hH / 2);
      const g = x.createLinearGradient(0, hY, 0, hY + hH);
      g.addColorStop(0, "rgb(7,8,10)");
      g.addColorStop(1, "rgb(16,19,26)");
      roundRectPath(x, hX, hY, hW, hH, hR);
      x.fillStyle = g;
      x.fill();
      if (hH > 4) {
        const lipR = Math.min(hR, (hW - 2) / 2);
        x.strokeStyle = `rgba(255,255,255,${0.28 * m})`;
        x.beginPath();
        x.moveTo(hX + lipR, hY + hH - 0.5);
        x.lineTo(hX + hW - lipR, hY + hH - 0.5);
        x.stroke();
      }
    }
    x.restore();
  }

  /** Hands sit behind the body — drawn before it, in world coordinates. */
  private drawHandsBehind(
    x: CanvasRenderingContext2D,
    R: number, rx: number, ry: number, cx: number, cy: number,
  ) {
    if (this.hands <= 0.01 || this.isMini) return;
    if (R <= 14) return; // meaningless at compact/peek sizes

    const n = now();
    const bodyH = 2 * ry;
    const hew = 0.3 * ry * this.hands;
    const heh = 0.26 * ry * this.hands;
    const hwB = rx * this.sx;
    const hhB = ry * this.sy;
    const isWaving = n >= this.waveStart && this.waveStart > 0 && n < this.waveUntil;

    for (const sd of [-1, 1]) {
      let localX: number;
      let localY: number;
      let handRot = 0;

      if (sd > 0 && isWaving) {
        const wt = n - this.waveStart;
        const rise = Math.min(1, wt / 0.18);
        const riseEased = 1 - Math.pow(1 - rise, 3);
        const restX = hwB * 1.08;
        const restY = hhB * 0.7;
        const oscX = Math.cos(13 * wt) * 0.06 * bodyH;
        const oscY = -Math.sin(13 * wt) * 0.14 * bodyH;
        const waveX = hwB * 1.1 + oscX;
        const waveY = -hhB * 0.15 + oscY;
        localX = restX + (waveX - restX) * riseEased;
        localY = restY + (waveY - restY) * riseEased;
        handRot = (-0.5 + Math.sin(13 * wt) * 0.35) * riseEased;
      } else if (sd < 0 && isWaving) {
        const wt = n - this.waveStart;
        localX = -hwB * 1.08;
        localY = hhB * 0.7 + Math.sin(6 * wt) * 0.04 * bodyH;
      } else {
        localX = sd * hwB * 1.08;
        localY = hhB * 0.7;
      }

      const cosT = Math.cos(this.tilt);
      const sinT = Math.sin(this.tilt);
      const worldX = cx + cosT * localX - sinT * localY;
      const worldY = cy + sinT * localX + cosT * localY;

      x.save();
      x.translate(worldX, worldY);
      if (handRot !== 0) x.rotate(handRot);
      const g = x.createLinearGradient(hew * 0.7, -heh * 0.85, -hew * 0.8, heh * 0.9);
      if (this.bodyColor) {
        g.addColorStop(0, rgba(mix3(this.bodyColor, [1, 1, 1], 0.35)));
        g.addColorStop(1, rgba(this.bodyColor));
      } else {
        g.addColorStop(0, rgba(BASE_TOP));
        g.addColorStop(1, rgba(BASE_BOTTOM));
      }
      x.beginPath();
      x.ellipse(0, 0, hew, heh, 0, 0, Math.PI * 2);
      x.fillStyle = g;
      x.fill();
      x.strokeStyle = "rgba(0,0,0,0.08)";
      x.lineWidth = 1;
      x.stroke();
      x.restore();
    }
  }

  private drawBadge(x: CanvasRenderingContext2D, badge: Badge, R: number, cx: number, cy: number) {
    const bs = this.badgeS * (this.isMini ? 1.25 : 1);
    const bx = cx - R * 0.72 * this.sx;
    const by = cy - R * 0.72 * this.sy;
    const t = now();

    x.save();
    x.translate(bx, by);
    x.scale(bs, bs);
    const col = rgba(badge.color);

    if (badge.kind === "dots") {
      if (this.isMini) {
        const phase = (t * 2.4) % 1;
        const dotR = R * 0.22 * (1 + 0.25 * Math.sin(phase * Math.PI * 2));
        x.fillStyle = "#000";
        x.beginPath();
        x.arc(0, 0, R * 0.2, 0, Math.PI * 2);
        x.fill();
        x.fillStyle = col;
        x.beginPath();
        x.arc(0, 0, dotR, 0, Math.PI * 2);
        x.fill();
      } else {
        const pw = R * 0.72;
        const ph = R * 0.36;
        roundRectPath(x, -pw / 2, -ph / 2, pw, ph, ph / 2);
        x.fillStyle = col;
        x.fill();
        for (let i = 0; i < 3; i++) {
          const phase = (((t * 2.4 - i * 0.22) % 1) + 1) % 1;
          const dotR = R * 0.055 * (1 + 0.4 * Math.max(0, Math.sin(phase * Math.PI * 2)));
          x.fillStyle = "#fff";
          x.beginPath();
          x.arc((i - 1) * R * 0.18, 0, dotR, 0, Math.PI * 2);
          x.fill();
        }
      }
    } else if (badge.kind === "bang" || badge.kind === "question") {
      x.fillStyle = "#000";
      x.beginPath();
      x.arc(0, 0, R * 0.3, 0, Math.PI * 2);
      x.fill();
      x.fillStyle = col;
      x.beginPath();
      x.arc(0, 0, R * 0.23, 0, Math.PI * 2);
      x.fill();
      if (!this.isMini) {
        x.fillStyle = "#fff";
        x.font = `900 ${R * 0.32}px ${FONT}`;
        x.textAlign = "center";
        x.textBaseline = "middle";
        x.fillText(badge.kind === "bang" ? "!" : "?", 0, R * 0.02);
      }
    } else {
      x.fillStyle = "#000";
      x.beginPath();
      x.arc(0, 0, R * 0.2, 0, Math.PI * 2);
      x.fill();
      x.fillStyle = col;
      x.beginPath();
      x.arc(0, 0, R * 0.135, 0, Math.PI * 2);
      x.fill();
    }
    x.restore();
  }

  private drawParticles(x: CanvasRenderingContext2D, R: number, cx: number, cy: number) {
    for (const p of this.particles) {
      if (p.age <= 0) continue;
      const k = p.age / p.life;
      const a = k < 0.2 ? k / 0.2 : 1 - (k - 0.2) / 0.8;
      const px = cx + (p.x + p.vx * p.age) * R * 1.3;
      const py = cy + (p.y + p.vy * p.age) * R * 1.3;
      const sz = R * p.size * (1 + k * 0.4);

      x.save();
      x.translate(px, py);
      x.globalAlpha = Math.min(1, Math.max(0, a));
      switch (p.type) {
        case "heart":
          x.rotate(Math.sin(p.age * 6) * 0.3);
          x.fillStyle = "#FF4D6D";
          heartPath(x, sz);
          x.fill();
          break;
        case "star":
          x.rotate(p.rot + p.age * 2);
          x.fillStyle = "#F7B32B";
          starPath(x, sz, sz * 0.45);
          x.fill();
          break;
        case "spark":
          x.rotate(p.rot);
          x.fillStyle = "#fff";
          starPath(x, sz * 0.8, sz * 0.18);
          x.fill();
          break;
        case "sweat":
          x.fillStyle = "#7CC7FF";
          x.beginPath();
          x.moveTo(0, -sz);
          x.quadraticCurveTo(sz * 0.8, sz * 0.2, 0, sz * 0.6);
          x.quadraticCurveTo(-sz * 0.8, sz * 0.2, 0, -sz);
          x.fill();
          break;
        case "z":
          x.fillStyle = "rgb(209,219,235)";
          x.font = `700 ${sz * 1.9}px ${FONT}`;
          x.textAlign = "center";
          x.textBaseline = "middle";
          x.fillText("z", 0, 0);
          break;
        case "tear":
          x.fillStyle = "rgba(124,199,255,0.9)";
          x.beginPath();
          x.moveTo(0, -sz * 0.9);
          x.quadraticCurveTo(sz * 0.7, sz * 0.25, 0, sz * 0.55);
          x.quadraticCurveTo(-sz * 0.7, sz * 0.25, 0, -sz * 0.9);
          x.fill();
          x.fillStyle = "rgba(255,255,255,0.7)";
          x.beginPath();
          x.ellipse(-sz * 0.13, sz * 0.08, sz * 0.09, sz * 0.13, 0, 0, Math.PI * 2);
          x.fill();
          break;
        case "question":
          x.rotate(Math.sin(p.age * 4) * 0.2);
          x.fillStyle = "#38BDF8";
          x.font = `800 ${sz * 2.1}px ${FONT}`;
          x.textAlign = "center";
          x.textBaseline = "middle";
          x.fillText("?", 0, 0);
          break;
        case "anger": {
          // Popping "vein" mark: four bent strokes around a gap.
          const pop = 1 + 0.25 * Math.sin(Math.min(1, k * 4) * Math.PI);
          x.scale(pop, pop);
          x.strokeStyle = "#F4505E";
          x.lineWidth = sz * 0.2;
          x.lineCap = "round";
          for (let q = 0; q < 4; q++) {
            x.save();
            x.rotate((q * Math.PI) / 2 + Math.PI / 4);
            x.beginPath();
            x.moveTo(sz * 0.22, -sz * 0.62);
            x.quadraticCurveTo(sz * 0.2, -sz * 0.2, sz * 0.62, -sz * 0.22);
            x.stroke();
            x.restore();
          }
          break;
        }
        case "puff":
          // Small sigh cloud.
          x.fillStyle = "rgba(219,230,242,0.85)";
          for (const [dx, dy, r] of [[-0.35, 0.05, 0.42], [0.1, -0.18, 0.5], [0.45, 0.08, 0.36]] as const) {
            x.beginPath();
            x.arc(dx * sz, dy * sz, r * sz, 0, Math.PI * 2);
            x.fill();
          }
          break;
      }
      x.restore();
    }
  }
}

/** Ray → rounded-rect boundary intersection, for the mailbox morph. */
function rrPoint(ca: number, sa: number, W: number, H: number, cr: number): { x: number; y: number } {
  const eps = 1e-6;
  const kx = ca >= 0 ? 1 : -1;
  const ky = sa >= 0 ? 1 : -1;
  const cx = kx * (W - cr);
  const cy = ky * (H - cr);

  const dot = ca * cx + sa * cy;
  const disc = dot * dot - (cx * cx + cy * cy - cr * cr);
  if (disc >= 0) {
    const t = dot + Math.sqrt(disc);
    if (t > eps) {
      const px = ca * t;
      const py = sa * t;
      if (Math.abs(px) >= W - cr - eps && Math.abs(py) >= H - cr - eps) return { x: px, y: py };
    }
  }
  if (Math.abs(sa) > eps) {
    const t = (ky * H) / sa;
    if (t > eps) {
      const px = ca * t;
      if (Math.abs(px) <= W - cr + eps) return { x: px, y: ky * H };
    }
  }
  if (Math.abs(ca) > eps) {
    const t = (kx * W) / ca;
    if (t > eps) {
      const py = sa * t;
      if (Math.abs(py) <= H - cr + eps) return { x: kx * W, y: py };
    }
  }
  return { x: kx * W, y: ky * H };
}
