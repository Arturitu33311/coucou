// The island stepping aside for the buttons of the windows behind it (src/island/dodge.ts): which place clears
// the buttons, how long the pointer must rest on them, when the island is left alone, and when it comes back.
// The areas themselves (which windows count) are tested in src-tauri/src/platform/linux.rs.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Dodger, freeOffset, toBoxes } from "../src/island/dodge.ts";

// A 600 px wide window with the island (288 wide) in the middle; a tiled window's buttons reach into its right end.
const rest = { x: 156, y: 0, w: 288, h: 38 };
const buttons = toBoxes([[400, 0, 114, 46]]);
const MAX = (600 - 288) / 2;
const onButtons = { x: 450, y: 10 };
const onIsland = { x: 200, y: 10 };
const away = { x: 50, y: 300 };

test("the nearest place that clears the buttons, with room to spare", () => {
  // Before the buttons' left edge (400) with the 16 px margin: the island's right edge at 384.
  assert.equal(freeOffset(rest, buttons, MAX), -60);
  // Nothing in the way: stay.
  assert.equal(freeOffset(rest, [], MAX), 0);
  // Two sets of buttons, one at each side of the island: nowhere to go.
  assert.equal(freeOffset(rest, toBoxes([[400, 0, 114, 46], [60, 0, 114, 46]]), MAX), null);
  // A place that exists but is further than the window allows.
  assert.equal(freeOffset(rest, buttons, 30), null);
});

test("a flyby over the buttons does nothing; resting on them steps aside", () => {
  const d = new Dodger();
  assert.equal(d.update(0, onButtons, rest, buttons, MAX), 0);
  assert.equal(d.update(40, onButtons, rest, buttons, MAX), 0, "not long enough");
  assert.equal(d.update(100, onButtons, rest, buttons, MAX), -60, "rested past the dwell");
  const flyby = new Dodger();
  flyby.update(0, onButtons, rest, buttons, MAX);
  flyby.update(30, away, rest, buttons, MAX);
  assert.equal(flyby.update(200, onButtons, rest, buttons, MAX), 0, "the clock starts again after leaving");
});

test("the island's own body is the island's: the buttons behind its edge wait", () => {
  const d = new Dodger();
  d.update(0, onIsland, rest, buttons, MAX);
  assert.equal(d.update(500, onButtons, rest, buttons, MAX), 0, "just left the island's body");
  assert.equal(d.update(1700, onButtons, rest, buttons, MAX), 0, "dwell starts now");
  assert.equal(d.update(1800, onButtons, rest, buttons, MAX), -60);
});

test("it stays out of the way while the pointer is around the buttons, and comes back after a moment", () => {
  const d = new Dodger();
  d.update(0, onButtons, rest, buttons, MAX);
  assert.equal(d.update(100, onButtons, rest, buttons, MAX), -60);
  // Near the buttons (within 40 px) or on the island at its new place: stays.
  assert.equal(d.update(300, { x: 520, y: 20 }, rest, buttons, MAX), -60);
  assert.equal(d.update(600, { x: 150, y: 10 }, rest, buttons, MAX), -60, "on the island where it now is");
  // Gone: it waits RELEASE_MS (500), then goes back.
  assert.equal(d.update(700, away, rest, buttons, MAX), -60);
  assert.equal(d.update(1100, away, rest, buttons, MAX), -60);
  assert.equal(d.update(1300, away, rest, buttons, MAX), 0);
});

test("with no buttons behind it the island never moves, and forgets what it was waiting for", () => {
  const d = new Dodger();
  d.update(0, onButtons, rest, buttons, MAX);
  assert.equal(d.update(100, onButtons, rest, [], MAX), 0);
  assert.equal(d.pending(100), null);
});

test("pending says when something changes by itself, so a resting pointer is not forgotten", () => {
  const d = new Dodger();
  assert.equal(d.pending(0), null);
  d.update(0, onButtons, rest, buttons, MAX);
  const wait = d.pending(10);
  assert.ok(wait !== null && wait > 0 && wait < 100, String(wait)); // the dwell is nearly over
  d.update(100, onButtons, rest, buttons, MAX); // stepped aside
  assert.equal(d.pending(120), null, "while the pointer is around the buttons nothing is waiting");
  d.update(200, away, rest, buttons, MAX);
  assert.ok(d.pending(250) > 0, "the way back is waiting");
});

test("when there is no room it says so once instead of failing silently", () => {
  const d = new Dodger();
  assert.equal(d.takeNoRoom(), false);
  d.update(0, onButtons, rest, buttons, 10);
  d.update(100, onButtons, rest, buttons, 10);
  assert.equal(d.update(200, onButtons, rest, buttons, 10), 0, "nowhere to go: stays");
  assert.equal(d.takeNoRoom(), true);
  assert.equal(d.takeNoRoom(), false, "said once");
});
