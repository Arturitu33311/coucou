import { test } from "node:test";
import assert from "node:assert/strict";
import { SessionRegistry, TURN_SILENCE_MS, FORGET_MS } from "../src/island/session-registry.ts";

test("a sibling silent for 30 min mid-turn no longer holds the pill", () => {
  const r = new SessionRegistry();
  r.note("p", "a", true, 0);
  r.note("p", "b", true, 0);
  assert.equal(r.endTurn("p", "a", 1000), "sessionOnly");
  assert.equal(r.endTurn("p", "a", TURN_SILENCE_MS + 1), "pill");
});

test("sessions silent for 12 h are forgotten", () => {
  const r = new SessionRegistry();
  r.note("p", "a", false, 0);
  r.note("p", "b", false, 0);
  assert.deepEqual(r.count("p", 1), { live: 2, working: 0 });
  assert.deepEqual(r.count("p", FORGET_MS), { live: 0, working: 0 });
  assert.equal(r.endSession("p", "a", FORGET_MS), "pill");
});

test("an event without a session is not counted", () => {
  const r = new SessionRegistry();
  r.note("p", "", true, 0);
  assert.deepEqual(r.count("p", 0), { live: 0, working: 0 });
});
