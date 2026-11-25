// The sessions living behind each pill (the Mac's PillSessionRegistry.swift). Several Claude Code sessions share
// one pill; without this, one session's Stop marked the pill finished (and an external agent's pill went away)
// while a sibling was still mid-turn. Pure: the clock is passed in.
//
// - A Stop finishes the pill only when no sibling session is mid-turn.
// - A SessionEnd clears or removes the pill only when it was the last live session.
// - A session silent for 30 min mid-turn is no longer counted as working (it was interrupted or crashed), and one
//   silent for 12 h is forgotten.

/** "pill": the pill itself finishes or ends; "sessionOnly": only that session did, a sibling carries on. */
export type Scope = "pill" | "sessionOnly";

export const TURN_SILENCE_MS = 30 * 60_000;
export const FORGET_MS = 12 * 3600_000;

interface Live {
  working: boolean;
  lastSeen: number;
}

export class SessionRegistry {
  private pills = new Map<string, Map<string, Live>>();

  private sweep(pillId: string, now: number): Map<string, Live> {
    const sessions = this.pills.get(pillId) ?? new Map<string, Live>();
    for (const [id, s] of sessions) {
      if (now - s.lastSeen >= FORGET_MS) sessions.delete(id);
      else if (s.working && now - s.lastSeen >= TURN_SILENCE_MS) s.working = false;
    }
    if (sessions.size) this.pills.set(pillId, sessions);
    else this.pills.delete(pillId);
    return sessions;
  }

  /** An event from `sessionId` behind `pillId`; `working`: it means the session is mid-turn. */
  note(pillId: string, sessionId: string, working: boolean, now: number): void {
    if (!sessionId) return;
    const sessions = this.sweep(pillId, now);
    const s = sessions.get(sessionId);
    if (s) {
      s.working = working || s.working;
      s.lastSeen = now;
    } else {
      sessions.set(sessionId, { working, lastSeen: now });
    }
    this.pills.set(pillId, sessions);
  }

  /** The session's turn is over (Stop, an error, an interrupt). */
  endTurn(pillId: string, sessionId: string, now: number): Scope {
    const sessions = this.sweep(pillId, now);
    const s = sessions.get(sessionId);
    if (s) {
      s.working = false;
      s.lastSeen = now;
    }
    for (const [id, other] of sessions) if (id !== sessionId && other.working) return "sessionOnly";
    return "pill";
  }

  /** The session is gone (SessionEnd). */
  endSession(pillId: string, sessionId: string, now: number): Scope {
    const sessions = this.sweep(pillId, now);
    sessions.delete(sessionId);
    if (!sessions.size) {
      this.pills.delete(pillId);
      return "pill";
    }
    return "sessionOnly";
  }

  /** Live sessions behind the pill, and how many are mid-turn. */
  count(pillId: string, now: number): { live: number; working: number } {
    const sessions = this.sweep(pillId, now);
    let working = 0;
    for (const s of sessions.values()) if (s.working) working++;
    return { live: sessions.size, working };
  }

  clear(): void {
    this.pills.clear();
  }
}
