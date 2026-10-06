// The island stepping aside for the buttons of the windows behind it.
//
// With the top bar hidden, a window tiled beside the middle of the screen has its
// minimise / maximise / close buttons under a centred island. Moving the island
// away for good would give up the notch; staying put would swallow the click. So it
// reads what the pointer is after:
//
//   * resting on the island's body, or having just been there: it is the island's —
//     it stays where it is, whatever lies behind its edge;
//   * coming to rest on the buttons it covers (from outside the island): they are the
//     target — it steps to the nearest place that clears them, and its mouse area goes
//     with it at once, so the click lands on the button;
//   * once the pointer has left the buttons for a moment, it glides back to the middle.
//
// Pure (no DOM, no clock of its own), so the rules can be tested.

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** How close to a button's area the pointer counts as being on it: on it, not beside it (the
 *  middle of the island, where it is used, can be a few px from the edge of a tiled window). */
const NEAR = 0;
/** Still near the buttons, for the purpose of staying out of the way. */
const NEAR_WIDE = 40;
/** The pointer must rest this long on the buttons (a flyby on its way to the island does not count). */
const DWELL_MS = 80;
/** After being on the island's body, this long before the buttons behind its edge win. */
const ENGAGED_MS = 1500;
/** After leaving the buttons, this long before the island returns to the middle. */
const RELEASE_MS = 500;
/** The island's mouse area reaches this far past its edge (Rust's HIT_MARGIN). */
const INPUT_MARGIN = 14;
/** Clear space left between the island's mouse area and a button once it has stepped aside. */
const MARGIN = 16;

const hit = (a: Box, b: Box) => a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
const within = (px: number, py: number, b: Box, m = 0) =>
  px >= b.x - m && px <= b.x + b.w + m && py >= b.y - m && py <= b.y + b.h + m;
const shifted = (b: Box, d: number): Box => ({ ...b, x: b.x + d });
const grown = (b: Box, m: number): Box => ({ x: b.x - m, y: b.y - m, w: b.w + 2 * m, h: b.h + 2 * m });

export function toBoxes(zones: number[][]): Box[] {
  return zones.map(([x, y, w, h]) => ({ x, y, w, h }));
}

/**
 * The offset, within ±`max`, nearest to 0 at which `island` clears every zone with a
 * little room to spare; null when no such place exists.
 */
export function freeOffset(island: Box, zones: Box[], max: number): number | null {
  const candidates = new Set<number>([0]);
  for (const z of zones) {
    // Rounded to the safe side: a width that is 288.00006 mid-animation must not make the
    // nearest place "touch" by a hair and be thrown out.
    candidates.add(Math.ceil(z.x + z.w + MARGIN - island.x)); // past its right edge
    candidates.add(Math.floor(z.x - MARGIN - (island.x + island.w))); // before its left edge
  }
  const clear = (d: number) => zones.every((z) => !hit(grown(shifted(island, d), MARGIN), z));
  const options = [...candidates].filter((d) => Math.abs(d) <= max && clear(d));
  if (options.length === 0) return null;
  return options.reduce((best, d) => (Math.abs(d) < Math.abs(best) ? d : best));
}

export class Dodger {
  /** Where the island is meant to be, as an offset from the middle. */
  private target = 0;
  private nearSince: number | null = null;
  private engagedUntil = 0;
  private releaseAt: number | null = null;

  /**
   * Milliseconds until something here changes by itself (the pointer has not moved, so
   * nothing calls `update`, yet it has now rested long enough, or been away long enough);
   * null when nothing is waiting.
   */
  pending(now: number): number | null {
    if (this.target === 0 && this.nearSince !== null) return Math.max(0, DWELL_MS - (now - this.nearSince)) + 5;
    if (this.target !== 0 && this.releaseAt !== null) return Math.max(0, this.releaseAt - now) + 5;
    if (this.target === 0 && this.engagedUntil > now) return this.engagedUntil - now + 5;
    return null;
  }

  reset() {
    this.target = 0;
    this.nearSince = null;
    this.engagedUntil = 0;
    this.releaseAt = null;
  }

  /**
   * `rest` is the island where it sits in the middle. Returns the offset it should be
   * at now (0 or a place clear of the buttons it would otherwise cover).
   */
  update(now: number, pointer: { x: number; y: number } | null, rest: Box, zones: Box[], max: number): number {
    // Covered: under the island, or under the mouse area that reaches past it.
    const covered = zones.filter((z) => hit(grown(rest, INPUT_MARGIN), z));
    if (covered.length === 0) {
      this.reset();
      return 0;
    }
    const on = (m: number) => pointer !== null && covered.some((z) => within(pointer.x, pointer.y, z, m));

    if (this.target === 0) {
      const buttons = on(NEAR);
      const body = pointer !== null && !buttons && within(pointer.x, pointer.y, rest);
      if (body) this.engagedUntil = now + ENGAGED_MS;
      if (buttons && now >= this.engagedUntil) {
        this.nearSince ??= now;
        if (now - this.nearSince >= DWELL_MS) {
          const free = freeOffset(rest, zones, max);
          if (free !== null && free !== 0) {
            this.target = free;
            this.releaseAt = null;
          }
        }
      } else {
        this.nearSince = null;
      }
    } else {
      // Stepped aside: it stays out of the way while the pointer is around the buttons,
      // or on the island at its new place, and goes back a moment after it is not.
      const here = pointer !== null && within(pointer.x, pointer.y, shifted(rest, this.target), 4);
      if (on(NEAR_WIDE) || here) {
        this.releaseAt = null;
      } else if (this.releaseAt === null) {
        this.releaseAt = now + RELEASE_MS;
      } else if (now >= this.releaseAt) {
        this.reset();
      }
    }
    return this.target;
  }
}
