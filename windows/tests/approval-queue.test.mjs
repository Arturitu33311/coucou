import { test } from "node:test";
import assert from "node:assert/strict";
import { ApprovalQueue, MAX_WAITING, REQUEST_LIFE_MS, MIN_CARD_MS } from "../src/island/approval-queue.ts";

const w = (id, at = 0, session = "s") => ({ requestId: id, pillId: "p", sessionId: session, receivedAt: at, payload: {} });

test("first in, first out, four waiting at most", () => {
  const q = new ApprovalQueue();
  for (let i = 1; i <= MAX_WAITING; i++) assert.ok(q.push(w("r" + i)));
  assert.equal(q.push(w("r9")), false);
  assert.equal(q.position("r3"), 3);
  assert.equal(q.next(0).next.requestId, "r1");
});

test("a request whose relay has given up is dropped on the way", () => {
  const q = new ApprovalQueue();
  q.push(w("old", 0));
  q.push(w("new", REQUEST_LIFE_MS));
  const { next, expired } = q.next(REQUEST_LIFE_MS);
  assert.equal(next.requestId, "new");
  assert.deepEqual(expired.map((x) => x.requestId), ["old"]);
});

test("a request that waited gets only the time its relay has left, never less than a glance", () => {
  assert.equal(ApprovalQueue.cardMs(0, 0), REQUEST_LIFE_MS + 2000);
  assert.equal(ApprovalQueue.cardMs(0, 60_000), REQUEST_LIFE_MS + 2000 - 60_000);
  assert.equal(ApprovalQueue.cardMs(0, REQUEST_LIFE_MS), MIN_CARD_MS);
});
