// Full-screen ("immersive") mode of the desktop player (desktop.html).
//
// With the mode on, the page hides its chrome (bar, recents, cover flow, search, clock, gauge) and
// only the background (the song's music video, or the page's own scene) and the current lyric line
// stay (immersive.css, keyed on the `immersive` class of #dp). The plan's "Inmersivo" mode
// (docs/superpowers/plans/2026-11-09-reproductor-correcciones.md, section 5).
//
// Enter: the bar's full-screen button, or by itself once nothing has touched the page for IDLE_MS
// while a song plays. Leave: any pointer movement, click, key (Esc included) or wheel; the press
// that leaves is swallowed, so it does not also trigger the control under the pointer.
//
// `Immersive` is the logic, with its clock, timer and effects injected (tests/desktop-player.test.mjs
// drives it with fakes); `buildImmersive` wires it to the page. Idle detection costs one timer: it
// is armed only while a song plays and the page is seen, wakes once per idle period (not per event:
// activity only writes a timestamp) and stops when the page is hidden or the song stops.

import { t } from "../i18n/i18n";

/** How long nothing may touch the page, while a song plays, before full screen starts by itself. */
export const IDLE_MS = 20_000;
/** After entering, pointer jitter (and the synthetic move a layout change fires) is not "activity". */
export const GRACE_MS = 400;
/** A pointer move shorter than this (in px) from where it rested is jitter. */
export const MOVE_PX = 4;
/** After the press that leaves, its release and click are swallowed for this long. */
export const SWALLOW_MS = 600;

export interface ImmersiveHost {
  now(): number;
  setTimer(fn: () => void, ms: number): number;
  clearTimer(id: number): void;
  /** Shows or hides the chrome. */
  apply(on: boolean): void;
  /** A song is playing and the page is seen: idle time may count. */
  playing(): boolean;
  /** Something is loaded (playing or paused): the button may enter. */
  hasTrack(): boolean;
  /** The search box (or any field) has the keyboard: do not take the screen. */
  typing(): boolean;
  /** Called after every change of state (the button's label). */
  changed?(on: boolean): void;
}

export class Immersive {
  on = false;
  private last: number;
  private timer: number | null = null;
  private enteredAt = 0;
  private swallowUntil = 0;
  private x = NaN;
  private y = NaN;

  private host: ImmersiveHost;
  private idleMs: number;

  constructor(host: ImmersiveHost, idleMs = IDLE_MS) {
    this.host = host;
    this.idleMs = idleMs;
    this.last = host.now();
  }

  /** The song, the page's visibility or the keyboard focus changed: arm, disarm or leave. */
  sync() {
    if (this.on) {
      if (!this.host.hasTrack()) this.leave();
      return;
    }
    if (this.host.playing()) this.arm();
    else this.disarm();
  }

  /** The pointer moved to (x, y), in page pixels. True when it left full screen. */
  move(x: number, y: number): boolean {
    if (this.on) {
      const still = this.host.now() - this.enteredAt < GRACE_MS || Math.hypot(x - this.x, y - this.y) < MOVE_PX;
      if (still) return false;
      this.leave();
      return true;
    }
    this.x = x;
    this.y = y;
    this.touch();
    return false;
  }

  /** A press, key or wheel. True when it left full screen: swallow it. */
  press(): boolean {
    if (this.on) {
      this.leave();
      return true;
    }
    this.touch();
    return false;
  }

  /** The press's release and click: true while they must be swallowed. */
  swallow(): boolean {
    return this.host.now() < this.swallowUntil;
  }

  /** The button. */
  toggle() {
    if (this.on) this.leave();
    else if (this.host.hasTrack()) this.enter();
  }

  /** Idle counts from now (activity, or a field that lost the keyboard). */
  touch() {
    this.last = this.host.now();
    this.sync();
  }

  dispose() {
    this.disarm();
  }

  private enter() {
    this.disarm();
    this.on = true;
    this.enteredAt = this.host.now();
    this.host.apply(true);
    this.host.changed?.(true);
  }

  private leave() {
    this.on = false;
    const now = this.host.now();
    this.swallowUntil = now + SWALLOW_MS;
    this.last = now;
    this.host.apply(false);
    this.host.changed?.(false);
    this.sync();
  }

  private arm() {
    if (this.timer != null) return;
    const wait = Math.max(0, this.last + this.idleMs - this.host.now());
    this.timer = this.host.setTimer(() => this.fire(), wait);
  }

  private disarm() {
    if (this.timer != null) this.host.clearTimer(this.timer);
    this.timer = null;
  }

  private fire() {
    this.timer = null;
    if (this.on || !this.host.playing()) return;
    if (this.last + this.idleMs - this.host.now() > 0) return this.arm(); // activity since: wait out the rest
    // A field with the keyboard blocks it; its blur calls touch(), which arms the next period.
    if (!this.host.typing()) this.enter();
  }
}

// ── The page ──────────────────────────────────────────────────────────────────

export interface ImmersiveOptions {
  /** The page's root (#dp): it gets the `immersive` class. */
  root: HTMLElement;
  /** A song is playing (not paused, not idle). */
  isPlaying(): boolean;
  /** A song is loaded. */
  hasTrack(): boolean;
}

export interface ImmersiveView {
  /** The bar's full-screen button; main.ts puts it among the bar's actions. */
  button: HTMLButtonElement;
  sync(): void;
  relabel(): void;
  isOn(): boolean;
}

const EXPAND =
  '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"></path></svg>';

function isField(e: Element | null): boolean {
  if (!e) return false;
  return e.tagName === "INPUT" || e.tagName === "TEXTAREA" || (e as HTMLElement).isContentEditable === true;
}

export function buildImmersive(o: ImmersiveOptions): ImmersiveView {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "dp-bar-btn dp-btn-immersive";
  button.innerHTML = EXPAND;

  const ctl = new Immersive({
    now: () => Date.now(),
    setTimer: (fn, ms) => window.setTimeout(fn, ms),
    clearTimer: (id) => window.clearTimeout(id),
    apply(on) {
      o.root.classList.toggle("immersive", on);
      // a field would keep the keyboard under the hidden chrome
      if (on && isField(document.activeElement)) (document.activeElement as HTMLElement).blur();
    },
    playing: () => o.isPlaying() && !document.hidden,
    hasTrack: o.hasTrack,
    typing: () => isField(document.activeElement),
    changed: () => relabel(),
  });

  function relabel() {
    const label = ctl.on ? t("Exit full screen") : t("Full screen");
    button.setAttribute("aria-label", label);
    button.title = label;
    button.setAttribute("aria-pressed", String(ctl.on));
  }

  // Capture phase on the window: the press that leaves never reaches the control under it.
  const stop = (e: Event) => {
    e.stopPropagation();
    e.preventDefault();
  };
  const onPress = (e: Event) => {
    if (ctl.press()) stop(e);
  };
  const onRelease = (e: Event) => {
    if (ctl.swallow()) stop(e);
  };
  const onMove = (e: MouseEvent) => void ctl.move(e.clientX, e.clientY);
  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("mousemove", onMove, true);
  for (const type of ["pointerdown", "mousedown", "touchstart", "keydown"]) window.addEventListener(type, onPress, true);
  window.addEventListener("wheel", onPress, { capture: true, passive: false });
  for (const type of ["pointerup", "mouseup", "touchend", "click", "dblclick", "contextmenu", "keyup"]) {
    window.addEventListener(type, onRelease, true);
  }
  // a field that lost the keyboard: idle counts again (it may have been what blocked full screen)
  window.addEventListener("focusout", () => ctl.touch());
  document.addEventListener("visibilitychange", () => ctl.sync());
  button.addEventListener("click", () => ctl.toggle());
  relabel();

  return { button, sync: () => ctl.sync(), relabel, isOn: () => ctl.on };
}
