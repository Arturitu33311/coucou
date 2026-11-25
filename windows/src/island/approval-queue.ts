// Permission requests that arrive while another card is up wait here instead of going back to their terminal
// (the Mac's ApprovalQueue.swift: first in, first out, up to five in all). Pure: the clock is passed in.
//
// A waiting request is only worth showing while its relay still waits: past REQUEST_LIFE_MS the terminal has
// taken the question, so it is dropped (and declined, which the relay ignores by then). A request whose turn is
// over (Stop, a new prompt…) is dropped as well, like the card itself.

/** Five in all, as on the Mac: the one on the card and four waiting. */
export const MAX_WAITING = 4;
/** The relay holds a request ~120 s; Coucou answers within 108 s of its arrival or not at all. */
export const REQUEST_LIFE_MS = 108_000;
/** Below this there is no time left to read a card and decide. */
export const MIN_CARD_MS = 8_000;

export interface Waiting<P> {
  requestId: string;
  pillId: string;
  sessionId: string;
  receivedAt: number;
  /** The original hook payload, handled again when its turn comes. */
  payload: P;
}

export class ApprovalQueue<P> {
  private items: Waiting<P>[] = [];

  get size(): number {
    return this.items.length;
  }

  /** Queues a request; false when the line is full (the caller sends it back to its terminal). */
  push(w: Waiting<P>): boolean {
    if (this.items.some((x) => x.requestId === w.requestId)) return true;
    if (this.items.length >= MAX_WAITING) return false;
    this.items.push(w);
    return true;
  }

  /** The next request still worth a card, and the ones that expired on the way (to decline). */
  next(now: number): { next: Waiting<P> | null; expired: Waiting<P>[] } {
    const expired: Waiting<P>[] = [];
    while (this.items.length) {
      const w = this.items.shift()!;
      if (now - w.receivedAt > REQUEST_LIFE_MS - MIN_CARD_MS) expired.push(w);
      else return { next: w, expired };
    }
    return { next: null, expired };
  }

  /** How long the card of a request that waited can stay up. */
  static cardMs(receivedAt: number, now: number): number {
    return Math.max(MIN_CARD_MS, REQUEST_LIFE_MS + 2_000 - (now - receivedAt));
  }

  /** The turn of that session is over: its waiting requests are moot. Returns them (to decline). */
  dropSession(pillId: string, sessionId: string): Waiting<P>[] {
    const gone = this.items.filter((x) => x.pillId === pillId && x.sessionId === sessionId);
    this.items = this.items.filter((x) => !(x.pillId === pillId && x.sessionId === sessionId));
    return gone;
  }

  /** Settled somewhere else (the phone, the terminal): it no longer waits. */
  remove(requestId: string): boolean {
    const before = this.items.length;
    this.items = this.items.filter((x) => x.requestId !== requestId);
    return this.items.length !== before;
  }

  /** 1-based place of a request in the line (the card is 0), or 0 when it is not waiting. */
  position(requestId: string): number {
    return this.items.findIndex((x) => x.requestId === requestId) + 1;
  }
}
