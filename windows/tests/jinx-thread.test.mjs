// Jinx's shared thread as the chat shows it (src/core/jinx-thread.ts), and the chat on Jinx reading it
// from the gateway (src/views/chat.ts, src/island/jinx.ts) — this fork: Jinx is the assistant on the PC
// as on the phones.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { calls, emit, internals } from "./tauri.mjs";
import { modeLabel, sourceLabel, threadMessages } from "../src/core/jinx-thread.ts";

installFakeDom();
const { buildPrompt } = await import("../src/views/chat.ts");
const { DEFAULT_SETTINGS, JINX_ID, State } = await import("../src/core/state.ts");
const { registerJinxHandlers } = await import("../src/island/jinx.ts");

test("each line says where it was written; Jinx's are hers", () => {
  const lines = [
    { id: "a", ts: 1, src: "tg", text: "desde Telegram" },
    { id: "b", ts: 2, src: "jinx", text: "hola" },
    { id: "c", ts: 3, src: "s21", text: "desde el móvil" },
    { id: "d", ts: 4, src: "pc", text: "desde el PC" },
    { id: "e", ts: 5, src: "s22", text: "   " },
  ];
  const msgs = threadMessages(lines, 10);
  assert.deepEqual(msgs.map((m) => [m.id, m.role, m.via]), [
    [10, "user", "Telegram"], [11, "assistant", undefined], [12, "user", "S21"], [13, "user", "PC"],
  ]);
  assert.equal(sourceLabel("s22"), "S22");
  assert.equal(modeLabel("hermes"), "Jinx (Hermes)");
  assert.equal(modeLabel("local"), "Jinx de respaldo · local");
  assert.equal(modeLabel(null), "");
});

/** What the mocked Rust side answers, by command. */
let answers = {};
const plainInvoke = internals.invoke;
internals.invoke = async (cmd, args) => {
  const result = await plainInvoke(cmd, args);
  if (cmd in answers) return typeof answers[cmd] === "function" ? answers[cmd](args) : answers[cmd];
  return result;
};
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const emotes = [];
const island = { reveal() {}, alert() {}, dropPin() {}, setView() {}, emote(name) { emotes.push(name); } };
registerJinxHandlers(island);

let view;
beforeEach(() => {
  calls.length = 0;
  State.settings = { ...DEFAULT_SETTINGS };
  State.tasks = [{ id: JINX_ID, name: "Jinx", color: "#39FF14", state: "idle", stepIndex: 0, steps: [], source: "n8n", isIntegration: true }];
  State.jinxHistory = [];
  State.jinxBusy = false;
  State.jinxMode = null;
  State.view = "prompt";
  State.chatTarget = "jinx";
  State.chatPicked = true;
  State.agents = [];
  view = buildPrompt(() => {});
});

test("the chat on Jinx shows the thread shared with the phones and Telegram, labelled", async () => {
  answers = {
    jinx_history: [
      { id: "1", ts: 1, src: "s21", text: "¿qué hay hoy?" },
      { id: "2", ts: 2, src: "jinx", text: "Nada urgente." },
    ],
  };
  view.sync();
  await flush();
  view.sync();
  assert.deepEqual(State.jinxHistory.map((m) => m.content), ["¿qué hay hoy?", "Nada urgente."]);
  assert.deepEqual(view.el.find(".chat-via").map((e) => e.textContent), ["S21"]);
});

test("an answer through the gateway says which brain gave it and brings the thread up to date", async () => {
  answers = { jinx_history: [{ id: "x:pc", ts: 1, src: "pc", text: "hola" }, { id: "x:jinx", ts: 2, src: "jinx", text: "¡Hola!" }] };
  State.jinxBusy = true;
  emotes.length = 0;
  emit("jinx", { kind: "done", text: "¡Hola!", mode: "or" });
  await flush();
  assert.equal(State.jinxBusy, false);
  assert.deepEqual(emotes, ["happy"], "Mochi wears the mood of her reply");
  assert.equal(State.jinxMode, "or");
  assert.ok(calls.some(([cmd]) => cmd === "jinx_history"), "the thread is read again after the answer");
  assert.deepEqual(State.jinxHistory.map((m) => [m.role, m.via]), [["user", "PC"], ["assistant", undefined]]);
});

test("with the Hermes API (no gateway) the local history stays as it is", async () => {
  answers = { jinx_history: null };
  State.jinxHistory = [{ id: 1, role: "user", content: "local" }];
  emotes.length = 0;
  emit("jinx", { kind: "done", text: "respuesta", mode: "hermes" });
  await flush();
  assert.deepEqual(emotes, [], "a neutral reply leaves Mochi as he is");
  assert.deepEqual(State.jinxHistory.map((m) => m.content), ["local", "respuesta"]);
});
