// The cards of agents with no session: Claude Code shows the plan's bars (else its hooks), the hook agents their
// last session (else their hooks). No "Open Visual Studio Code", no "Refresh" that changes nothing.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();
const { renderIntegrationCard, lastSessionLine } = await import("../src/views/integrations.ts");
const { DEFAULT_SETTINGS, State } = await import("../src/core/state.ts");

const hooks = { detailOpen: false, openDetail() {}, closeDetail() {}, openSettings() {} };
const task = (id, extra = {}) => ({ id, name: id, color: "#fff", state: "idle", stepIndex: 0, steps: [], ...extra });

beforeEach(() => {
  State.settings = { ...DEFAULT_SETTINGS };
  State.integrations = {};
  State.planUsage = null;
});

test("Claude Code with no session: the plan's 5 h and weekly bars, and no editor button", () => {
  State.planUsage = {
    fiveHour: { usedPct: 42, resetsAt: Date.now() + 3_600_000 },
    sevenDay: { usedPct: 10, resetsAt: Date.now() + 86_400_000 },
    updatedAt: Date.now(),
  };
  const el = renderIntegrationCard(task("integration_claude"), hooks);
  assert.ok(!el.textContent.includes("Open Visual Studio Code"), el.textContent);
  assert.ok(!el.textContent.includes("Refresh"), el.textContent);
  assert.ok(el.querySelector(".plan-rows"), "the gauges are there");
  assert.ok(el.textContent.includes("42%"), el.textContent);
});

test("Claude Code without usage numbers: the state of its hooks, nothing to press", () => {
  const el = renderIntegrationCard(task("integration_claude"), hooks);
  assert.ok(/Hooks (not )?installed/.test(el.textContent), el.textContent);
  assert.equal(el.querySelector(".plan-rows"), null);
  assert.ok(!el.textContent.includes("Refresh"));
});

test("a hook agent shows its last session, and its hooks when it has none", () => {
  const none = renderIntegrationCard(task("agent_opencode"), hooks);
  assert.ok(!none.textContent.includes("Last session"));
  assert.ok(!none.textContent.includes("Refresh"));
  const t = task("agent_antigravity", { lastSession: { cwd: "/home/alan/dev/coucou", at: Date.now() - 3 * 3_600_000 } });
  assert.equal(lastSessionLine(t), "coucou · 3h ago");
  assert.ok(renderIntegrationCard(t, hooks).textContent.includes("coucou · 3h ago"));
  assert.equal(lastSessionLine(task("agent_opencode")), null);
  assert.equal(lastSessionLine(task("agent_opencode", { lastSession: { cwd: null, at: Date.now() } })), "just now");
});

test("a session that ends leaves its project and time on the kept pill", () => {
  State.settings = { ...DEFAULT_SETTINGS, mainPill: "integration_jinx", activeIntegrations: ["agent_opencode"] };
  State.tasks = [];
  State.loadIntegrationTasks();
  const t = State.tasks.find((x) => x.id === "agent_opencode");
  t.sessionCwd = "/work/api";
  t.steps = ["Edit · a.ts"];
  State.removeTask("agent_opencode");
  assert.equal(State.tasks.find((x) => x.id === "agent_opencode").lastSession.cwd, "/work/api");
});
