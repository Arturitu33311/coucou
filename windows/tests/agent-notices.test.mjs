import { test } from "node:test";
import assert from "node:assert/strict";
import { AgentNotices, LONG_RUN_MS, REPEAT_MS } from "../src/island/agent-notices.ts";

test("only a long run finishing is news, said with how long it took", () => {
  const n = new AgentNotices();
  n.begin("p", "s", 0);
  assert.equal(n.finished("p", "s", LONG_RUN_MS - 1, "Claude Code", "coucou"), null);
  n.begin("p", "s", 0);
  assert.deepEqual(n.finished("p", "s", 12 * 60_000, "OpenCode", "web"), { text: "OpenCode terminó en web (12 min).", kind: "info" });
  // A Stop with no start seen (the island restarted) says nothing.
  assert.equal(n.finished("p", "x", 99 * 60_000, "Claude Code", "coucou"), null);
});

test("the same news about the same session is said once in a while, not on every beat", () => {
  const n = new AgentNotices();
  assert.ok(n.waiting("p", "s", 0, "Claude Code", "coucou", "Bash"));
  assert.equal(n.waiting("p", "s", REPEAT_MS - 1, "Claude Code", "coucou", "Bash"), null);
  assert.ok(n.waiting("p", "s", REPEAT_MS, "Claude Code", "coucou", "Bash"));
  assert.ok(n.failed("p", "s", 0, "Antigravity", "app"));
  assert.ok(n.waiting("p", "other", 1, "Claude Code", "coucou", "Bash"));
});

test("long runs read in hours past sixty minutes", () => {
  const n = new AgentNotices();
  n.begin("p", "s", 0);
  assert.equal(n.finished("p", "s", 75 * 60_000, "Claude Code", "x").text, "Claude Code terminó en x (1 h 15 min).");
});
