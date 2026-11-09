// How the desktop player turns its cards towards the centre: the "Continue
// listening" rows and the cover flow's side covers (the mockup's rotateY).
//
// WebKitGTK draws 3D transforms only with accelerated compositing. The launcher
// sets WEBKIT_DISABLE_COMPOSITING_MODE=1 for the whole process (it overrides any
// per-webview hardware-acceleration policy), and without compositing rotateY is
// flattened to a plain horizontal squeeze. So the page asks the engine
// (`has3d`) and, when it cannot, draws a 2D likeness instead: the card narrowed
// as much as the turn would narrow it and slanted so its far edge rises, the way
// a card seen from a little above recedes. Side covers mirror each other.

/** The rows' turn (the mockup's rotateY(22deg)). */
export const ROW_TURN = 22;
/** The rows' own perspective: each row carries it, so no wrapper can flatten it. */
export const ROW_PERSPECTIVE = 900;
/** How much of the turn becomes slant in the 2D likeness (22° → about 4°, 45° → about 8°). */
export const SLANT_PER_DEGREE = 0.18;

/** Whether this engine draws 3D transforms (false in WebKitGTK without compositing). */
export function has3d(win: { matchMedia?: (q: string) => { matches: boolean } } = globalThis as never): boolean {
  const mm = win.matchMedia;
  if (typeof mm !== "function") return true;
  try {
    return mm.call(win, "(-webkit-transform-3d)").matches || mm.call(win, "(transform-3d)").matches;
  } catch {
    return true;
  }
}

/** The 2D likeness of a turn of `deg` (positive: the right edge goes away). */
export function flatTurn(deg: number): { scaleX: number; skewY: number } {
  const scaleX = Math.cos((Math.abs(deg) * Math.PI) / 180);
  // the far edge rises: the right edge for a positive turn, the left one for a negative turn
  const skewY = deg === 0 ? 0 : -deg * SLANT_PER_DEGREE;
  return { scaleX: round(scaleX), skewY: round(skewY) };
}

/**
 * The CSS transform for a card turned by `deg`. In 3D: `perspective` (when the
 * element carries its own), `translateZ(z)` and rotateY. Flat: the 2D likeness.
 */
export function turnTransform(deg: number, flat: boolean, opts: { perspective?: number; z?: number } = {}): string {
  if (flat) {
    if (deg === 0) return "none";
    const f = flatTurn(deg);
    return `scaleX(${f.scaleX}) skewY(${f.skewY}deg)`;
  }
  const parts: string[] = [];
  if (opts.perspective) parts.push(`perspective(${opts.perspective}px)`);
  if (opts.z) parts.push(`translateZ(${opts.z}px)`);
  parts.push(`rotateY(${deg}deg)`);
  return parts.join(" ");
}

function round(n: number): number {
  return Math.round(n * 1000) / 1000 || 0;
}
