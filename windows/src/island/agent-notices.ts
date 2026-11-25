// What Jinx is told about the coding agents (Claude Code, Antigravity, OpenCode…) while you are away from the PC:
// a long task that finished, an error, a permission that has been waiting. She says it on the device you are
// using, or by Telegram when nobody is watching (the gateway's `op: notify`). Pure: the clock is passed in; the
// island decides whether you are away (Presence) and sends.
//
// Never a stream: short runs say nothing (the island already shows them when you are here), and the same news
// about the same session is said once per REPEAT_MS.

/** A run shorter than this finishing is not news. */
export const LONG_RUN_MS = 3 * 60_000;
/** A permission still unanswered after this, with you away, is worth telling. */
export const WAIT_MS = 45_000;
/** The same news for the same session, at most once in this long. */
export const REPEAT_MS = 2 * 60_000;

export interface Notice {
  text: string;
  kind: "info" | "warn" | "alert";
}

function minutes(ms: number): string {
  const m = Math.round(ms / 60_000);
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`;
}

export class AgentNotices {
  private started = new Map<string, number>();
  private said = new Map<string, number>();

  private key(pillId: string, sessionId: string) {
    return `${pillId}|${sessionId}`;
  }

  /** Says it unless the same news about the same session was said less than REPEAT_MS ago. */
  private once(what: string, pillId: string, sessionId: string, now: number, n: Notice): Notice | null {
    const k = `${what}|${this.key(pillId, sessionId)}`;
    const before = this.said.get(k);
    if (before != null && now - before < REPEAT_MS) return null;
    this.said.set(k, now);
    // Old entries go, so the map does not grow for ever.
    for (const [x, t] of this.said) if (now - t > 4 * REPEAT_MS) this.said.delete(x);
    return n;
  }

  /** A run began (a prompt, or the first tool of an agent that sends no prompt event). */
  begin(pillId: string, sessionId: string, now: number): void {
    const k = this.key(pillId, sessionId);
    if (!this.started.has(k)) this.started.set(k, now);
  }

  /** The run ended well: news when it was long. */
  finished(pillId: string, sessionId: string, now: number, who: string, project: string): Notice | null {
    const k = this.key(pillId, sessionId);
    const since = this.started.get(k);
    this.started.delete(k);
    if (since == null || now - since < LONG_RUN_MS) return null;
    return this.once("done", pillId, sessionId, now, {
      text: `${who} terminó en ${project} (${minutes(now - since)}).`,
      kind: "info",
    });
  }

  /** The run failed. */
  failed(pillId: string, sessionId: string, now: number, who: string, project: string): Notice | null {
    this.started.delete(this.key(pillId, sessionId));
    return this.once("error", pillId, sessionId, now, { text: `${who} tuvo un error en ${project}.`, kind: "warn" });
  }

  /** A permission has been waiting for WAIT_MS. */
  waiting(pillId: string, sessionId: string, now: number, who: string, project: string, tool: string): Notice | null {
    return this.once("wait", pillId, sessionId, now, {
      text: `${who} te espera en ${project}: quiere usar ${tool}.`,
      kind: "alert",
    });
  }

  /** The session ended: nothing more to say about it. */
  forget(pillId: string, sessionId: string): void {
    this.started.delete(this.key(pillId, sessionId));
  }
}
