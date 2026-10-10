// Is this the device Mochi and Coucou belong on right now? The decision is made in Rust
// (src-tauri/src/presence.rs, shared rules with the phone); the page only draws what it is told.
// Nothing here changes what is waiting: a permission request keeps its timeout and its answer
// while the island is out of sight, the page just does not paint it.

import { IS_TAURI, onEvent } from "./bridge";

export interface PresenceState {
  /** This device is the one in use (true when nobody says otherwise). */
  active: boolean;
  /** The other device is playing something: Mochi dances to it too. */
  peerMusic: boolean;
  /** Something just arrived on the other device: a moment of surprise. */
  peerNews: boolean;
}

export const Presence: PresenceState = { active: true, peerMusic: false, peerNews: false };

const listeners = new Set<(p: PresenceState) => void>();

/** Called on every change of the state; returns an unsubscribe. */
export function onPresence(fn: (p: PresenceState) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

function apply(next: Partial<PresenceState>) {
  const was = { ...Presence };
  Object.assign(Presence, next);
  document.body.classList.toggle("presence-away", !Presence.active);
  if (was.active !== Presence.active || was.peerMusic !== Presence.peerMusic || was.peerNews !== Presence.peerNews) {
    listeners.forEach((fn) => fn(Presence));
  }
}

let lastReported: boolean | null = null;

/** Tells Rust whether this device is playing something (sent to the phone, whose Mochi dances too). */
export async function reportLocalMusic(playing: boolean) {
  if (!IS_TAURI || playing === lastReported) return;
  lastReported = playing;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("presence_local_music", { playing });
  } catch (err) {
    lastReported = null;
    console.error("[coucou] presence_local_music failed", err);
  }
}

/** Something arrived on this device (a notification): the phone's Mochi is startled too. */
export async function reportLocalNews() {
  if (!IS_TAURI) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("presence_local_news");
  } catch (err) {
    console.error("[coucou] presence_local_news failed", err);
  }
}

export async function startPresence() {
  if (!IS_TAURI) return;
  await onEvent<PresenceState>("presence", (p) => apply(p));
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    apply(await invoke<PresenceState>("presence_state"));
  } catch (err) {
    console.error("[coucou] presence_state failed", err);
  }
}
