import test from "node:test";
import assert from "node:assert/strict";
import { createApp } from "../src/app.ts";
import { EV } from "../src/events.ts";

const flush = () => new Promise((r) => setTimeout(r, 5));

function setup({ client = {}, saved = "0" } = {}) {
  const shown = [];
  const log = { mic: [], exit: 0, store: {} };
  const glasses = {
    show: async (t) => { shown.push(t); },
    mic: async (on) => { log.mic.push(on); },
    exit: () => { log.exit++; },
  };
  const store = { get: async (k) => (k === "seen" ? saved : ""), set: async (k, v) => { log.store[k] = v; } };
  const fullClient = {
    voice: async () => ({ id: "v", status: "done", heard: "hola", text: "Hola Alan" }),
    ask: async () => ({ id: "a", status: "done", text: "x" }),
    notices: async () => { await new Promise((r) => setTimeout(r, 20)); return []; },
    ...client,
  };
  const app = createApp({ glasses, client: fullClient, store, noticeWait: 0, retryMs: 5, sleep: (ms) => new Promise((r) => setTimeout(r, ms)) });
  return { app, shown, log };
}

const sys = (eventType) => ({ sysEvent: eventType === undefined ? {} : { eventType } });
const pcm = (n) => ({ audioEvent: { audioPcm: new Uint8Array(n) } });

test("hold to talk: audio is collected only while listening and sent on release", async () => {
  let sent;
  const { app, shown, log } = setup({ client: { voice: async (p) => { sent = p; return { id: "v", status: "done", heard: "hola", text: "Hola Alan" }; } } });
  await app.start();
  await app.onEvent(pcm(100)); // before pressing: ignored
  await app.onEvent(sys(EV.LONG_PRESS));
  await flush();
  assert.deepEqual(log.mic, [true]);
  await app.onEvent(pcm(300));
  await app.onEvent(pcm(200));
  await app.onEvent(sys(EV.LONG_PRESS_RELEASE));
  await flush();
  assert.equal(sent.length, 500);
  assert.deepEqual(log.mic, [true, false]);
  assert.match(shown.join("|"), /Te escucho/);
  assert.match(shown.join("|"), /“hola”/);
  assert.equal(shown.at(-1), "Hola Alan");
});

test("a too-short recording is 'no te entendí', not an error", async () => {
  const { app, shown } = setup({ client: { voice: async () => { throw Object.assign(new Error("audio demasiado corto"), { kind: "server" }); } } });
  await app.onEvent(sys(EV.LONG_PRESS));
  await app.onEvent(sys(EV.LONG_PRESS_RELEASE));
  await flush();
  assert.match(shown.at(-1), /No te entendí/);
});

test("an empty transcript from the server is 'no te entendí'", async () => {
  const { app, shown } = setup({ client: { voice: async () => ({ status: "empty", heard: "" }) } });
  await app.onEvent(sys(EV.LONG_PRESS));
  await app.onEvent(sys(EV.LONG_PRESS_RELEASE));
  await flush();
  assert.match(shown.at(-1), /No te entendí/);
});

test("server failures show a short message", async () => {
  const { app, shown } = setup({ client: { voice: async () => { throw Object.assign(new Error("x"), { kind: "network" }); } } });
  await app.onEvent(sys(EV.LONG_PRESS));
  await app.onEvent(sys(EV.LONG_PRESS_RELEASE));
  await flush();
  assert.match(shown.at(-1), /Sin conexión con Jinx/);
});

test("double-tap always exits, on every screen", async () => {
  const { app, log } = setup();
  await app.onEvent(sys(EV.DOUBLE_CLICK));
  await app.onEvent(sys(EV.LONG_PRESS));
  await app.onEvent({ textEvent: { eventType: EV.DOUBLE_CLICK } });
  assert.equal(log.exit, 2);
});

test("tap while waiting cancels the wait and a late answer is dropped", async () => {
  let release;
  const { app, shown } = setup({ client: { voice: (p, signal) => new Promise((res, rej) => {
    release = () => res({ id: "v", status: "done", text: "tarde" });
    signal.addEventListener("abort", () => rej(Object.assign(new Error("c"), { kind: "aborted" })));
  }) } });
  await app.onEvent(sys(EV.LONG_PRESS));
  await app.onEvent(sys(EV.LONG_PRESS_RELEASE));
  await flush();
  await app.onEvent(sys());
  release();
  await flush();
  assert.ok(!shown.includes("tarde"));
  assert.equal(app.model.screen.kind, "home");
});

test("taking the glasses off while listening closes the mic", async () => {
  const { app, log } = setup();
  await app.onEvent(sys(EV.LONG_PRESS));
  await app.onEvent(sys(EV.FOREGROUND_EXIT));
  await flush();
  assert.deepEqual(log.mic, [true, false]);
  assert.equal(app.model.screen.kind, "home");
});

test("system exit stops everything", async () => {
  const { app, log } = setup();
  await app.onEvent(sys(EV.SYSTEM_EXIT));
  await flush();
  assert.deepEqual(log.mic, [false]);
});

test("notices are polled, shown, and the read position is stored", async () => {
  let n = 0;
  const asked = [];
  const { app, shown, log } = setup({
    saved: "7",
    client: { notices: async (since) => { asked.push(since); return ++n === 1 ? [{ id: 8, text: "Claude Code terminó" }] : (await new Promise((r) => setTimeout(r, 20)), []); } },
  });
  await app.start();
  const polling = app.pollNotices();
  await flush();
  assert.match(shown.at(-1), /Claude Code terminó/);
  await app.onEvent(sys()); // tap: open
  await app.onEvent(sys()); // tap: last one, mark read
  await flush();
  assert.equal(log.store.seen, "8");
  assert.equal(asked[0], 7);
  assert.ok(asked.slice(1).every((s) => s === 8), "asks only for newer ones: " + asked);
  app.stop();
  await polling;
});

test("notice polling survives a bridge that is down", async () => {
  let n = 0;
  const { app, shown } = setup({ client: { notices: async (since) => { if (++n < 3) throw new Error("caído"); return [{ id: 1, text: "ya" }]; } } });
  const polling = app.pollNotices();
  await new Promise((r) => setTimeout(r, 60));
  assert.match(shown.at(-1) ?? "", /ya/);
  app.stop();
  await polling;
});
