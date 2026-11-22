// Full screen with the live pointer quirks (immersive.ts) and its word-by-word lyrics (kinetic.ts).

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();

const { Immersive, IDLE_MS } = await import("../src/desktop-player/immersive.ts");
const { planWords } = await import("../src/desktop-player/kinetic.ts");

function setup() {
  let now = 1_000;
  let seq = 0;
  const timers = new Map();
  const host = {
    now: () => now,
    setTimer: (fn, ms) => {
      timers.set(++seq, { fn, at: now + ms });
      return seq;
    },
    clearTimer: (id) => timers.delete(id),
    apply: () => {},
    playing: () => true,
    hasTrack: () => true,
    typing: () => false,
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
  return { ctl, advance };
}

test("a pointer resting on the page since it loaded: the lyric's synthetic moves never leave full screen", () => {
  const { ctl, advance } = setup();
  ctl.sync();
  advance(IDLE_MS);
  assert.equal(ctl.on, true);
  assert.equal(ctl.why, "idle");
  // live: WebKit fires a move at the resting pointer each time the lyric line changes under it
  for (let i = 0; i < 5; i++) {
    advance(3000);
    assert.equal(ctl.move(1589, 676), false, `synthetic move ${i}`);
  }
  assert.equal(ctl.on, true);
  assert.equal(ctl.move(1700, 700), true, "a real move still leaves");
  assert.equal(ctl.why, "pointer moved 114 px");
  advance(IDLE_MS - 1);
  assert.equal(ctl.on, false, "and it waits the whole idle time again");
});

test("planWords: every word enters early, and they light up across the line", () => {
  const p = planWords("Pa' que nada te pase", 4000);
  assert.deepEqual(
    p.map((w) => w.text),
    ["Pa'", "que", "nada", "te", "pase"],
  );
  assert.equal(p[0].enterMs, 0);
  assert.ok(p.at(-1).enterMs <= 4000 * 0.35, "all in within the first third");
  assert.ok(p.at(-1).litMs < 4000 && p.at(-1).litMs > 2000, "the last lights up late in the line");
  for (let i = 1; i < p.length; i++) assert.ok(p[i].litMs > p[i - 1].litMs);
  assert.deepEqual(planWords("   ", 3000), []);
  assert.ok(planWords("a b", 60_000).at(-1).litMs <= 12_000, "a long instrumental gap does not stretch it");
});
