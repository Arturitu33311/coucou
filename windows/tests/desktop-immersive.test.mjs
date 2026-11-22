// The desktop player's full-screen mode (src/desktop-player/immersive.ts): idle entry, leaving,
// the swallowed exit click, and the guards (search focus, nothing playing, hidden page).

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();

const { Immersive, IDLE_MS, GRACE_MS, MOVE_PX, SWALLOW_MS } = await import("../src/desktop-player/immersive.ts");

/** A host with a manual clock: `advance(ms)` runs the timers that fall due. */
function setup(state = {}) {
  const s = { playing: true, track: true, typing: false, ...state };
  let now = 1_000;
  let seq = 0;
  const timers = new Map();
  const log = [];
  const host = {
    now: () => now,
    setTimer: (fn, ms) => {
      timers.set(++seq, { fn, at: now + ms });
      return seq;
    },
    clearTimer: (id) => timers.delete(id),
    apply: (on) => log.push(on),
    playing: () => s.playing,
    hasTrack: () => s.track,
    typing: () => s.typing,
  };
  const ctl = new Immersive(host);
  const advance = (ms) => {
    const end = now + ms;
    for (;;) {
      const due = [...timers.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      timers.delete(due[0]);
      now = Math.max(now, due[1].at);
      due[1].fn();
    }
    now = end;
  };
  return { ctl, s, log, advance, timers };
}

test("a playing song with no activity enters full screen after the idle time", () => {
  const { ctl, advance, log } = setup();
  ctl.sync();
  advance(IDLE_MS - 1);
  assert.equal(ctl.on, false);
  advance(1);
  assert.equal(ctl.on, true);
  assert.deepEqual(log, [true]);
});

test("activity pushes the idle time back, with a single timer", () => {
  const { ctl, advance, timers } = setup();
  ctl.sync();
  advance(IDLE_MS - 1000);
  ctl.move(10, 10);
  ctl.press();
  assert.equal(timers.size, 1);
  advance(1500); // past the first deadline: the timer finds activity and waits out the rest
  assert.equal(ctl.on, false);
  advance(IDLE_MS - 1500 - 1);
  assert.equal(ctl.on, false);
  advance(1);
  assert.equal(ctl.on, true);
});

test("nothing playing, or a paused song, never enters by idle", () => {
  const a = setup({ playing: false });
  a.ctl.sync();
  assert.equal(a.timers.size, 0);
  a.advance(IDLE_MS * 3);
  assert.equal(a.ctl.on, false);

  const b = setup();
  b.ctl.sync();
  b.s.playing = false; // paused (or the page hidden) before the deadline
  b.advance(IDLE_MS + 1);
  assert.equal(b.ctl.on, false);
  b.ctl.sync();
  assert.equal(b.timers.size, 0);
});

test("a field with the keyboard blocks entering; its blur starts a new idle period", () => {
  const { ctl, s, advance } = setup({ typing: true });
  ctl.sync();
  advance(IDLE_MS + 1);
  assert.equal(ctl.on, false);
  s.typing = false;
  ctl.touch(); // focusout
  advance(IDLE_MS - 1);
  assert.equal(ctl.on, false);
  advance(1);
  assert.equal(ctl.on, true);
});

test("the button enters and leaves; with no song it does nothing", () => {
  const { ctl, s, log } = setup();
  ctl.toggle();
  assert.equal(ctl.on, true);
  ctl.toggle();
  assert.equal(ctl.on, false);
  assert.deepEqual(log, [true, false]);
  s.track = false;
  ctl.toggle();
  assert.equal(ctl.on, false);
});

test("a press, key or wheel leaves and is consumed; the release and click are swallowed", () => {
  const { ctl, advance } = setup();
  ctl.toggle();
  advance(GRACE_MS + 1);
  assert.equal(ctl.press(), true);
  assert.equal(ctl.on, false);
  assert.equal(ctl.swallow(), true); // the click that would hit the control under the pointer
  advance(SWALLOW_MS);
  assert.equal(ctl.swallow(), false);
  assert.equal(ctl.press(), false); // outside full screen a press is only activity
});

test("pointer movement leaves, but jitter and the grace period after entering do not", () => {
  const { ctl, advance } = setup();
  ctl.move(500, 500);
  ctl.toggle();
  assert.equal(ctl.move(900, 900), false); // inside the grace period
  advance(GRACE_MS + 1);
  assert.equal(ctl.move(500 + MOVE_PX - 1, 500), false); // a hair
  assert.equal(ctl.on, true);
  assert.equal(ctl.move(500 + MOVE_PX, 500), true);
  assert.equal(ctl.on, false);
});

test("leaving re-arms the idle time from the moment of leaving", () => {
  const { ctl, advance } = setup();
  ctl.toggle();
  advance(GRACE_MS + 1);
  ctl.press();
  advance(IDLE_MS - 1);
  assert.equal(ctl.on, false);
  advance(1);
  assert.equal(ctl.on, true);
});

test("the song going away while in full screen leaves it", () => {
  const { ctl, s } = setup();
  ctl.toggle();
  s.track = false;
  ctl.sync();
  assert.equal(ctl.on, false);
});
