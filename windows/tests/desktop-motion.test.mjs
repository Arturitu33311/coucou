// The desktop player's motion helpers (src/desktop-player/motion.ts) and the cover flow's placement as a move
// of a cover laid out once at the centre (flow.ts): the page animates transform and opacity only.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();

const { staggerDelay, STAGGER_CAP, STAGGER_STEP, trackKey, songChanged, freshFlags } = await import("../src/desktop-player/motion.ts");
const { flowPlacement, flowCoverTransform, flowTransform } = await import("../src/desktop-player/flow.ts");

test("staggerDelay grows by a step and stops at the cap", () => {
  assert.equal(staggerDelay(0), 0);
  assert.equal(staggerDelay(3), 3 * STAGGER_STEP);
  assert.equal(staggerDelay(50), STAGGER_CAP * STAGGER_STEP);
  assert.equal(staggerDelay(-2), 0);
});

test("a song change is a new key, never the first song or no song", () => {
  const a = trackKey({ title: "A", artists: ["X"] });
  const b = trackKey({ title: "A", artists: ["Y"] });
  assert.notEqual(a, b);
  assert.equal(trackKey(null), "");
  assert.equal(trackKey({ title: "", artists: [] }), "");
  assert.equal(songChanged("", a), false);
  assert.equal(songChanged(a, a), false);
  assert.equal(songChanged(a, ""), false);
  assert.equal(songChanged(a, b), true);
});

test("freshFlags marks only the rows not shown before", () => {
  assert.deepEqual(freshFlags(new Set(["a", "b"]), ["c", "a", "d"]), [true, false, true]);
  assert.deepEqual(freshFlags(new Set(), ["a"]), [true]);
});

test("the front cover stays at the centre at full size; the others move by transform", () => {
  const front = flowPlacement(2, 2);
  assert.equal(front.dx, 0);
  assert.equal(front.scale, 1);
  const right = flowPlacement(3, 2);
  assert.ok(right.dx > 0 && right.scale < 1);
  const left = flowPlacement(1, 2);
  assert.ok(left.dx < 0);
  // mirrored
  assert.ok(Math.abs(left.dx + right.dx) < 1e-6);
});

test("flowPlacement agrees with flowTransform's box", () => {
  for (const [i, c] of [[0, 2], [4, 2], [2, 2], [3, 1]]) {
    const tf = flowTransform(i, c);
    const p = flowPlacement(i, c);
    assert.ok(Math.abs(300 * p.scale - tf.size) < 1e-9);
    assert.ok(Math.abs(520 + p.dx - (tf.left + tf.size / 2)) < 1e-9);
  }
});

test("a cover's transform uses the same functions in the same order in 3D and flat", () => {
  const fn = (s) => s.match(/[a-zA-Z]+(?=\()/g).join(",");
  assert.equal(fn(flowCoverTransform(0, 2, false)), fn(flowCoverTransform(2, 2, false)));
  assert.equal(fn(flowCoverTransform(0, 2, true)), fn(flowCoverTransform(2, 2, true)));
  assert.match(flowCoverTransform(2, 2, false), /rotateY\(0deg\)/);
  assert.doesNotMatch(flowCoverTransform(1, 2, true), /rotateY|translateZ/);
});
