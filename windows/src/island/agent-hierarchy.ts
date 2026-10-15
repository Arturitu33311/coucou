// Agent hierarchy definitions for the two-level structure
// Jinx on top, Claude Code, Antigravity, OpenCode as mini-Mochis below

export type AgentKind = "jinx" | "claude" | "antigravity" | "opencode";

export interface Agent {
  id: string;
  kind: AgentKind;
  name: string;
  color: string;
  icon: string;
  state: "working" | "done" | "approval" | "question";
  badge?: string; // e.g., "+2" for subagents
  projectName?: string;
  cwd?: string;
  hostApp?: string | null;
  bundleId?: string | null;
  sessionId?: string;
}

export const AGENTS: Record<AgentKind, Agent> = {
  jinx: {
    id: "jinx",
    kind: "jinx",
    name: "Jinx",
    color: "#FF6B6B", // Warm red
    icon: "🐱",
    state: "working",
    projectName: "Jinx Personality",
    cwd: "/home/jinx",
    sessionId: "jinx-main",
    hostApp: null,
    bundleId: null,
    badge: undefined,
  },
  claude: {
    id: "integration_claude",
    kind: "claude",
    name: "Claude Code",
    color: "#4A90E2", // Blue
    icon: "💻",
    state: "working",
    projectName: "Current Project",
    cwd: "/workspace/project",
    hostApp: null,
    bundleId: null,
    sessionId: "claude-session-1",
    badge: undefined,
  },
  antigravity: {
    id: "agent_antigravity",
    kind: "antigravity",
    name: "Antigravity",
    color: "#50C878", // Green
    icon: "⚛️",
    state: "working",
    projectName: "React Project",
    cwd: "/workspace/react-app",
    hostApp: "cursor",
    bundleId: "com.cursor.cursor",
    sessionId: "antigravity-session-1",
    badge: undefined,
  },
  opencode: {
    id: "opencode-agent",
    kind: "opencode",
    name: "OpenCode",
    color: "#FFA500", // Orange
    icon: "🔧",
    state: "working",
    projectName: "OpenCode Sessions",
    cwd: "/workspace/opencode",
    hostApp: "vscode",
    bundleId: "com.microsoft.VSCode",
    sessionId: "opencode-session-1",
    badge: undefined,
  },
};

export class AgentHierarchy {
  private currentAgent: AgentKind = "jinx";
  private agents: Map<AgentKind, Agent> = new Map();
  private agentHistory: AgentKind[] = [];

  constructor() {
    // Initialize agents map from AGENTS record
    for (const [kind, agent] of Object.entries(AGENTS)) {
      this.agents.set(kind as AgentKind, agent);
    }
    this.agentHistory = ["jinx"];
  }

  getCurrentAgent(): Agent {
    return this.agents.get(this.currentAgent) as Agent;
  }

  getAgent(kind: AgentKind): Agent {
    return this.agents.get(kind) as Agent;
  }

  getAllAgents(): Agent[] {
    return Array.from(this.agents.values());
  }

  switchToAgent(kind: AgentKind, fromSubagent: boolean = false): void {
    if (!this.agents.has(kind)) {
      return;
    }

    // If switching from subagent to parent, go to Jinx
    if (fromSubagent && kind !== "jinx") {
      kind = "jinx";
    }

    const previousAgent = this.currentAgent;
    this.currentAgent = kind;

    // Add to history if not already there
    if (!this.agentHistory.includes(kind)) {
      this.agentHistory.push(kind);
    }

    // Notify listeners about agent switch
    this.onAgentSwitch?.(previousAgent, kind);
  }

  goBackToPreviousAgent(): void {
    if (this.agentHistory.length <= 1) {
      return; // Can't go back further than Jinx
    }

    // Remove current from history
    this.agentHistory.pop();
    const previousAgent = this.agentHistory[this.agentHistory.length - 1];

    // If the previous agent was a mini-Mochi, switch to it
    this.switchToAgent(previousAgent as AgentKind);
  }

  updateAgentState(kind: AgentKind, state: Agent["state"]): void {
    const agent = this.agents.get(kind);
    if (agent) {
      agent.state = state;
      this.onAgentUpdate?.(kind, agent);
    }
  }

  updateAgentSubagentCount(kind: AgentKind, count: number): void {
    const agent = this.agents.get(kind);
    if (agent) {
      agent.badge = count > 0 ? `+${count}` : undefined;
      this.onAgentUpdate?.(kind, agent);
    }
  }

  updateAgentProject(kind: AgentKind, projectName: string, cwd: string): void {
    const agent = this.agents.get(kind);
    if (agent) {
      agent.projectName = projectName;
      agent.cwd = cwd;
      this.onAgentUpdate?.(kind, agent);
    }
  }

  isMiniMochi(kind: AgentKind): boolean {
    return kind !== "jinx";
  }

  getVisibleAgents(): Agent[] {
    // Always show Jinx first, then any other agents that have pending work
    const visible: Agent[] = [];

    // Always include Jinx
    visible.push(this.getCurrentAgent());

    // Include other agents that have pending approvals or questions
    for (const [kind, agent] of this.agents) {
      if (kind !== "jinx" && (agent.state === "approval" || agent.state === "question")) {
        visible.push(agent);
      }
    }

    return visible;
  }

  // Event handlers
  onAgentSwitch?: (from: AgentKind, to: AgentKind) => void;
  onAgentUpdate?: (kind: AgentKind, agent: Agent) => void;

  // Check if an agent has subagents
  hasSubagents(kind: AgentKind): boolean {
    return kind === "claude"; // Only Claude has subagents for now
  }
}

export const agentHierarchy = new AgentHierarchy();

export default agentHierarchy;