import test from "node:test";
import assert from "node:assert/strict";
import { initial, reduce, render } from "../src/state.ts";

const run = (m, ...msgs) => msgs.reduce((acc, msg) => {
  const s = reduce(acc.model, msg);
  return { model: s.model, effects: [...acc.effects, ...s.effects] };
}, { model: m, effects: [] });

test("long press talks, release sends, the answer shows, tap pages and goes home", () => {
  let r = run(initial(), { t: "pressStart" });
  assert.equal(r.model.screen.kind, "listening");
  assert.deepEqual(r.effects, [{ e: "startMic" }]);
  r = run(r.model, { t: "pressEnd" });
  assert.equal(r.model.screen.kind, "working");
  assert.deepEqual(r.effects, [{ e: "sendVoice", ticket: 1 }]);
  const long = Array.from({ length: 80 }, (_, i) => `palabra${i}`).join(" ");
  r = run(r.model, { t: "heard", ticket: 1, text: "qué tal" }, { t: "answer", ticket: 1, text: long });
  assert.equal(r.model.screen.kind, "answer");
  const pages = r.model.screen.pages.length;
  assert.ok(pages > 1);
  r = run(r.model, { t: "down" }, { t: "up" }, { t: "up" });
  assert.equal(r.model.screen.page, 0);
  for (let i = 0; i < pages; i++) r = run(r.model, { t: "tap" });
  assert.equal(r.model.screen.kind, "home");
});

test("an answer to a cancelled question is ignored", () => {
  let r = run(initial(), { t: "pressStart" }, { t: "pressEnd" }, { t: "tap" });
  assert.equal(r.model.screen.kind, "home");
  assert.deepEqual(r.effects.at(-1), { e: "cancelWork" });
  r = run(r.model, { t: "answer", ticket: 1, text: "tarde" });
  assert.equal(r.model.screen.kind, "home");
});

test("empty and failed turns", () => {
  let r = run(initial(), { t: "pressStart" }, { t: "pressEnd" }, { t: "empty", ticket: 1 });
  assert.match(render(r.model), /No te entendí/);
  r = run(initial(), { t: "pressStart" }, { t: "pressEnd" }, { t: "failed", ticket: 1, message: "Sin conexión" });
  assert.equal(r.model.screen.kind, "error");
  assert.match(render(r.model), /Sin conexión/);
  r = run(r.model, { t: "tap" });
  assert.equal(r.model.screen.kind, "home");
});

test("pressing again while listening or waiting does nothing; releasing when not listening does nothing", () => {
  const listening = run(initial(), { t: "pressStart" }).model;
  assert.deepEqual(run(listening, { t: "pressStart" }).effects, []);
  assert.deepEqual(run(initial(), { t: "pressEnd" }).effects, []);
});

test("taking the glasses off while listening drops the audio and goes home", () => {
  const r = run(initial(), { t: "pressStart" }, { t: "background" });
  assert.equal(r.model.screen.kind, "home");
  assert.deepEqual(r.effects.at(-1), { e: "cancelMic" });
  // ...and a release that arrives afterwards sends nothing
  assert.deepEqual(run(r.model, { t: "pressEnd" }).effects, []);
});

test("notices: show on home, tap through, mark read, never twice", () => {
  const items = [{ id: 3, text: "Claude Code terminó", source: "agentes" }, { id: 4, text: "OpenCode falló" }];
  let r = run(initial(), { t: "notices", items });
  assert.match(render(r.model), /OpenCode falló/);
  assert.match(render(r.model), /2 avisos/);
  r = run(r.model, { t: "tap" });
  assert.equal(r.model.screen.kind, "notices");
  assert.match(render(r.model), /agentes\nClaude Code terminó\n1\/2/);
  r = run(r.model, { t: "tap" }, { t: "tap" });
  assert.equal(r.model.screen.kind, "home");
  assert.deepEqual(r.effects.at(-1), { e: "markRead", upTo: 4 });
  assert.equal(r.model.unread.length, 0);
  r = run(r.model, { t: "notices", items });
  assert.equal(r.model.unread.length, 0, "already read");
  r = run(r.model, { t: "notices", items: [{ id: 5, text: "nuevo" }] });
  assert.equal(r.model.unread.length, 1);
});

test("notices arriving mid-conversation do not interrupt it", () => {
  let r = run(initial(), { t: "pressStart" }, { t: "notices", items: [{ id: 1, text: "x" }] });
  assert.equal(r.model.screen.kind, "listening");
  assert.equal(r.model.unread.length, 1);
});

test("home with nothing says how to talk", () => {
  assert.match(render(initial()), /Mantén pulsado/);
});
