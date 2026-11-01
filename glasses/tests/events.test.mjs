import test from "node:test";
import assert from "node:assert/strict";
import { decode, EV } from "../src/events.ts";

const kinds = (e) => decode(e).map((i) => i.t);

test("a tap arrives as a sysEvent with no eventType (protobuf drops zero)", () => {
  assert.deepEqual(kinds({ sysEvent: {} }), ["tap"]);
  assert.deepEqual(kinds({ sysEvent: { eventType: 0 } }), ["tap"]);
});

test("an event with no envelope is not a tap (audio frames, empty events)", () => {
  assert.deepEqual(kinds({}), []);
  assert.deepEqual(kinds({ audioEvent: { audioPcm: new Uint8Array([1, 2]) } }), ["pcm"]);
});

test("scroll arrives as a textEvent", () => {
  assert.deepEqual(kinds({ textEvent: { eventType: EV.SCROLL_TOP } }), ["up"]);
  assert.deepEqual(kinds({ textEvent: { eventType: EV.SCROLL_BOTTOM } }), ["down"]);
});

test("double-tap wins in whichever envelope it arrives, and over a click in another", () => {
  assert.deepEqual(kinds({ sysEvent: { eventType: EV.DOUBLE_CLICK } }), ["double"]);
  assert.deepEqual(kinds({ textEvent: { eventType: EV.DOUBLE_CLICK } }), ["double"]);
  assert.deepEqual(kinds({ sysEvent: {}, textEvent: { eventType: EV.DOUBLE_CLICK } }), ["double"]);
});

test("long press, lifecycle and exits", () => {
  assert.deepEqual(kinds({ sysEvent: { eventType: EV.LONG_PRESS } }), ["pressStart"]);
  assert.deepEqual(kinds({ sysEvent: { eventType: EV.LONG_PRESS_RELEASE } }), ["pressEnd"]);
  assert.deepEqual(kinds({ sysEvent: { eventType: EV.FOREGROUND_ENTER } }), ["foreground"]);
  assert.deepEqual(kinds({ sysEvent: { eventType: EV.FOREGROUND_EXIT } }), ["background"]);
  assert.deepEqual(kinds({ sysEvent: { eventType: EV.SYSTEM_EXIT } }), ["exit"]);
  assert.deepEqual(kinds({ sysEvent: { eventType: EV.ABNORMAL_EXIT } }), ["exit"]);
  assert.deepEqual(kinds({ sysEvent: { eventType: 8 } }), []); // IMU: not ours
});

test("PCM given as a plain number array becomes bytes", () => {
  const [i] = decode({ audioEvent: { audioPcm: [1, 2, 3] } });
  assert.deepEqual([...i.data], [1, 2, 3]);
});
