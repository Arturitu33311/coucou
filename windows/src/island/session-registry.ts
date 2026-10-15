// Tracks the agent sessions living behind each pill — port of PillSessionRegistry.swift.
// Multiple claude sessions under the same client share a pill (e.g. integration_claude).
// With this registry:
// - A Stop only marks the pill finished when no sibling session is mid-turn.
// - A SessionEnd only drops the card/diffs when the last sibling session ends.
// - The oldest live session owns the name and cwd shown on the card.
// - Silent sessions expire (30 min mid-turn, 12 h overall).

export type TerminalScope = "pill" | "sessionOnly";

export interface SessionDisplay {
  session: string;
  projectName: string;
  cwd: string;
  hostApp?: string | null;
  bundleId?: string;
}

interface RecordedSession {
  projectName: string;
  cwd: string;
  hostApp?: string | null;
  bundleId?: string;
  isWorking: boolean;
  lastSeen: number;
}

interface PillRecord {
  /** Live sessions, oldest first. The first one owns name and cwd. */
  order: string[];
  sessions: Map<string, RecordedSession>;
}

export class PillSessionRegistry {
  /** 12 hours silence -> session considered gone. */
  static readonly maxSilence = 12 * 3600 * 1000;
  /** 30 min mid-turn without events -> session is no longer marked as working. */
  static readonly maxTurnSilence = 30 * 60 * 1000;

  private pills = new Map<string, PillRecord>();

  private getPill(pillId: string): PillRecord {
    let p = this.pills.get(pillId);
    if (!p) {
      p = { order: [], sessions: new Map() };
      this.pills.set(pillId, p);
    }
    return p;
  }

  /**
   * Records the session that sent an event as live behind `pillId`.
   * `working` indicates whether the event means the session is mid-turn (e.g.
   * PreToolUse for most tools).
   */
  note(pillId: string, session: SessionDisplay, working: boolean, timestamp = Date.now()): void {
    const pill = this.getPill(pillId);
    const rec: RecordedSession = {
      projectName: session.projectName,
      cwd: session.cwd,
      hostApp: session.hostApp,
      bundleId: session.bundleId,
      isWorking: working,
      lastSeen: timestamp,
    };

    // If this is an existing session, replace it; otherwise append
    const existingIdx = pill.order.findIndex((id) => id === session.session);
    if (existingIdx >= 0) {
      pill.order.splice(existingIdx, 1, session.session);
    } else {
      pill.order.push(session.session);
    }
    pill.sessions.set(session.session, rec);
    this.sweepPill(pillId, timestamp);
  }

  /** Number of live sessions behind the pill. */
  liveCount(pillId: string): number {
    return this.getPill(pillId).order.length;
  }

  /** Number of sessions currently marked as working. */
  workingCount(pillId: string): number {
    const sessions = this.getPill(pillId).sessions;
    return this.getPill(pillId).order.reduce((sum, id) =>
      sum + (sessions.get(id)?.isWorking ? 1 : 0), 0);
  }

  /**
   * Returns the session that currently owns the pill's name and cwd.
   * Returns undefined if no live sessions.
   */
  displayOwner(pillId: string): SessionDisplay | undefined {
    const pill = this.getPill(pillId);
    const firstId = pill.order[0];
    if (!firstId) return undefined;
    const rec = pill.sessions.get(firstId);
    if (!rec) return undefined;
    return {
      session: firstId,
      projectName: rec.projectName,
      cwd: rec.cwd,
      hostApp: rec.hostApp,
      bundleId: rec.bundleId,
    };
  }

  /** True when `sessionId` is the session that owns the pill's name and cwd. */
  ownsDisplay(pillId: string, sessionId: string): boolean {
    const owner = this.displayOwner(pillId);
    return !!owner && owner.session === sessionId;
  }

  /**
   * Processes a Stop event for the session behind `pillId`.
   * Returns "pill" if the pill should be marked finished, or "sessionOnly"
   * if only the session should be marked finished (other sibling sessions
   * are still mid-turn).
   */
  endTurn(pillId: string, sessionId: string, timestamp = Date.now()): TerminalScope {
    const pill = this.getPill(pillId);
    const session = pill.sessions.get(sessionId);
    if (!session) return "pill"; // Session already gone

    // Mark the session as no longer working
    session.isWorking = false;
    session.lastSeen = timestamp;

    // Check if any other sessions are still working
    const stillWorking = pill.order.some((id) =>
      pill.sessions.get(id)?.isWorking === true);

    // Sweep stale sessions
    this.sweepPill(pillId, timestamp);

    return stillWorking ? "sessionOnly" : "pill";
  }

  /**
   * Processes a SessionEnd event for the session behind `pillId`.
   * Returns "pill" if the pill should be cleared, or "sessionOnly"
   * if other sessions remain alive.
   */
  endSession(pillId: string, sessionId: string, timestamp = Date.now()): TerminalScope {
    const pill = this.getPill(pillId);
    if (!pill.sessions.has(sessionId)) return "pill"; // Already gone

    // Remove the session from tracking
    pill.sessions.delete(sessionId);
    pill.order = pill.order.filter((id) => id !== sessionId);

    // Sweep stale sessions
    this.sweepPill(pillId, timestamp);

    // If no sessions remain, the pill is empty
    if (pill.order.length === 0) {
      this.pills.delete(pillId);
      return "pill";
    }

    // Update the owner if the oldest session ended
    return "sessionOnly";
  }

  /** Remove sessions that have been silent too long. */
  private sweepPill(pillId: string, now: number): void {
    const pill = this.getPill(pillId);
    if (!pill) return;

    // Remove sessions that have been silent beyond maxSilence
    pill.order = pill.order.filter((id) => {
      const rec = pill.sessions.get(id);
      return rec && (now - rec.lastSeen) < PillSessionRegistry.maxSilence;
    });

    // Update the sessions map to match
    for (const id of [...pill.sessions.keys()]) {
      if (!pill.order.includes(id)) {
        pill.sessions.delete(id);
      }
    }

    // If no sessions remain, remove the pill entirely
    if (pill.order.length === 0) {
      this.pills.delete(pillId);
    }
  }
}

// Export a singleton instance for global use
export const GlobalSessionRegistry = new PillSessionRegistry();