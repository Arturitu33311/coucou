// Mochi's read of the room, ported from MochiReactions.kt (Android).
//
// The mobile side watches the notification list (a newcomer surprises him) and the
// unlock broadcast (he says hello). Neither exists on the desktop, so those two
// reactions stay mobile-only; everything else — music, low battery, the emptied
// task list, the night — works here with the same timings, so both Mochis agree
// on how long a mood lasts.
//
// Two channels into the engine (see mochi/engine.ts):
//   * persistent mood → setPermanentEmote (cleared back to null when calm);
//   * celebration     → triggerEmote("happy"), fired once when the list empties.

import type { BotEmoteName } from "../core/layout";
import type { BotEngine } from "./engine";
import { State } from "../core/state";

/** How long the celebration lasts after the list empties. */
export const CELEBRATE_MS = 3_000;
/** At or below this battery percent (off the mains) he is tired — same line as the hairline. */
const LOW_BATTERY = 20;
/** Night (00:00–05:59 local): he naps. */
const NIGHT_START = 0;
const NIGHT_END = 6;

/** What Mochi is reacting to right now. */
export interface MochiScene {
  /** Something is playing: he dances. */
  musicPlaying: boolean;
  /** The battery is low: he is tired. */
  batteryLow: boolean;
  /** Something is due or overdue: he wants attention. */
  attention: boolean;
  /** The task list just emptied: he celebrates (one-shot, not a state). */
  celebrating: boolean;
}

/** The persistent moods, in priority order. */
export type MochiMood = "alert" | "tired" | "dancing" | "sleepy" | "idle";

const MOOD_EMOTE: Record<Exclude<MochiMood, "idle">, BotEmoteName> = {
  alert: "annoyed",
  tired: "yawn",
  dancing: "happy",
  sleepy: "yawn",
};

export function moodOf(scene: MochiScene, hour: number): MochiMood {
  if (scene.attention) return "alert";
  if (scene.batteryLow) return "tired";
  if (scene.musicPlaying) return "dancing";
  if (hour >= NIGHT_START && hour < NIGHT_END) return "sleepy";
  return "idle";
}

export function emoteOf(mood: MochiMood): BotEmoteName | null {
  return mood === "idle" ? null : MOOD_EMOTE[mood];
}

/**
 * The celebration latch: the open count dropping to zero opens it, time closes
 * it. Kept in one place so every caller shares the same "previous count".
 */
export class CelebrationLatch {
  private until = 0;
  private previousOpen = -1;

  /** Returns true on the beat the list empties (fire the one-shot then). */
  update(open: number, now: number): boolean {
    let fired = false;
    if (this.previousOpen >= 0 && this.previousOpen > 0 && open === 0) {
      this.until = now + CELEBRATE_MS;
      fired = true;
    }
    this.previousOpen = open;
    return fired;
  }

  showing(now: number): boolean {
    return this.until > now;
  }
}

export function isNight(date = new Date()): boolean {
  const h = date.getHours();
  return h >= NIGHT_START && h < NIGHT_END;
}

// ── Live wiring ─────────────────────────────────────────────────────────────

const latch = new CelebrationLatch();
let lastMood: MochiMood | null = null;
let timer: ReturnType<typeof setInterval> | null = null;

/** Read from a hub task-list counter (injected so this file stays UI-free). */
export type OpenCounter = () => number;

function readScene(dueTasks: number): MochiScene {
  const now = Date.now();
  const bat = State.sys.battery;
  return {
    musicPlaying: State.music?.status === "Playing",
    batteryLow: bat != null && !State.sys.plugged && bat <= LOW_BATTERY,
    attention: dueTasks > 0,
    celebrating: latch.showing(now),
  };
}

/**
 * Watch the room once a second and drive the engine. Edge-triggered: the
 * persistent emote is only touched when the mood changes, the celebration only
 * on the beat the list empties. Stops itself when the callback is gone; call
 * once at startup.
 */
export function startSceneSync(engine: BotEngine, openCount: OpenCounter, dueCount: OpenCounter) {
  if (timer) return;
  const beat = () => {
    const now = Date.now();
    const open = openCount();
    if (latch.update(open, now)) engine.triggerEmote("happy", CELEBRATE_MS / 1000);
    const mood = moodOf(readScene(dueCount()), new Date().getHours());
    if (mood !== lastMood) {
      lastMood = mood;
      engine.setPermanentEmote(emoteOf(mood));
    }
  };
  timer = setInterval(beat, 1000);
  beat();
}

export function stopSceneSync() {
  if (timer) clearInterval(timer);
  timer = null;
  lastMood = null;
}
