// FIFO queue of pending PermissionRequests — port of ApprovalQueue.swift.
// Up to 5 requests wait in line; beyond that, the newest falls back to its terminal.
// The head is the request currently shown on the approval card.

export interface AskedQuestion {
  question: string;
  options: string[];
}

export interface QueuedApproval {
  requestId: string;
  sessionId: string;
  /** "integration_claude", "agent_cursor", "agent_codex", etc. */
  pillId: string;
  projectName: string;
  cwd: string;
  tool: string;
  command: string;
  /** tool_input serialized with sorted keys, used to match PostToolUse. */
  inputKey?: string;
  /** Set when Jinx (Hermes) is asking. */
  jinxRun?: string;
  questions?: AskedQuestion[];
  timeoutTimer?: number;
}

export class ApprovalQueue {
  static readonly maxPending = 5;

  private entries: QueuedApproval[] = [];

  get head(): QueuedApproval | undefined {
    return this.entries[0];
  }

  get count(): number {
    return this.entries.length;
  }

  get isEmpty(): boolean {
    return this.entries.length === 0;
  }

  contains(requestId: string): boolean {
    return this.entries.some((e) => e.requestId === requestId);
  }

  /** 1-based position of the request in the queue, undefined if absent. */
  position(requestId: string): number | undefined {
    const idx = this.entries.findIndex((e) => e.requestId === requestId);
    return idx >= 0 ? idx + 1 : undefined;
  }

  /** True when the queued requests do not all come from the same session and agent. */
  get hasMixedSources(): boolean {
    const first = this.entries[0];
    if (!first) return false;
    return this.entries.some((e) => e.sessionId !== first.sessionId || e.pillId !== first.pillId);
  }

  /**
   * Enqueues an approval request. Returns "enqueued" or "rejectedFull".
   */
  enqueue(entry: QueuedApproval): "enqueued" | "rejectedFull" {
    if (this.count >= ApprovalQueue.maxPending) {
      return "rejectedFull";
    }
    this.entries.push(entry);
    return "enqueued";
  }

  /**
   * Removes and returns the head of the queue when requestId matches.
   * Returns the new head if any, or null if queue becomes empty.
   */
  resolved(requestId: string, _decision: string): QueuedApproval | null {
    const idx = this.entries.findIndex((e) => e.requestId === requestId);
    if (idx === -1) return null;

    const removed = this.entries.splice(idx, 1)[0];
    
    // Clear the timeout timer if it exists
    if (removed?.timeoutTimer) {
      window.clearTimeout(removed.timeoutTimer);
    }

    return this.head ?? null;
  }

  /** Removes the first entry from the queue and returns it, or null if empty. */
  dropFirst(): QueuedApproval | null {
    if (this.isEmpty) return null;
    const removed = this.entries.shift();
    if (removed?.timeoutTimer) {
      window.clearTimeout(removed.timeoutTimer);
    }
    return removed ?? null;
  }

  /** Removes a specific request from the queue by requestId, or null if not found. */
  dequeue(requestId: string): QueuedApproval | null {
    const idx = this.entries.findIndex((e) => e.requestId === requestId);
    if (idx === -1) return null;
    
    const removed = this.entries.splice(idx, 1)[0];
    
    // Clear the timeout timer if it exists
    if (removed?.timeoutTimer) {
      window.clearTimeout(removed.timeoutTimer);
    }
    
    return removed ?? null;
  }

  /** Removes all entries from the queue. */
  clear() {
    for (const entry of this.entries) {
      if (entry.timeoutTimer) {
        window.clearTimeout(entry.timeoutTimer);
      }
    }
    this.entries = [];
  }
}

// Export a singleton instance for global use
export const GlobalApprovalQueue = new ApprovalQueue();