// Mochi gets sleepy after long hours with the agents, and fresh again after a break
// (Mac 0.6, #970, ported). Pure clock: the caller says, every so often, whether an
// agent is working right now; it answers whether Mochi should yawn.

/** A pause this long counts as a break: the day starts over. */
export const BREAK_MS = 10 * 60_000;
/** Hours of agent work, without a break, before Mochi starts to get tired. */
export const TIRED_AFTER_MS = 3 * 3_600_000;
/** Once tired, a yawn at most this often. */
export const YAWN_EVERY_MS = 25 * 60_000;

export class TiredClock {
  /** When the current stretch of work began (null: resting). */
  private since: number | null = null;
  private lastActive = 0;
  private lastYawn = 0;

  /**
   * @param now    current time (ms)
   * @param active an agent is working, thinking or waiting for you right now
   * @returns true when Mochi should yawn now
   */
  update(now: number, active: boolean): boolean {
    if (active) {
      if (this.since === null || now - this.lastActive >= BREAK_MS) {
        this.since = now;
        this.lastYawn = 0;
      }
      this.lastActive = now;
    } else if (this.since !== null && now - this.lastActive >= BREAK_MS) {
      // Rested long enough: fresh again.
      this.since = null;
      this.lastYawn = 0;
    }
    if (this.since === null || !active) return false;
    if (now - this.since < TIRED_AFTER_MS) return false;
    if (this.lastYawn !== 0 && now - this.lastYawn < YAWN_EVERY_MS) return false;
    this.lastYawn = now;
    return true;
  }

  /** Hours worked in the current stretch (0 when resting); for tests and tooltips. */
  hours(now: number): number {
    return this.since === null ? 0 : (now - this.since) / 3_600_000;
  }
}
