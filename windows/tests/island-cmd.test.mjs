// What Jinx asks of the island that the page does itself: Mochi's mood and a Hub tool.

import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { emit } from "./tauri.mjs";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();
const { registerIslandCommands } = await import("../src/island/island-cmd.ts");
const { DEFAULT_SETTINGS, State } = await import("../src/core/state.ts");

const seen = [];
const island = {
  emote: (name, seconds) => seen.push(["emote", name, seconds]),
  expand: (view) => seen.push(["expand", view]),
  reveal: () => seen.push(["reveal"]),
};
const IDS = ["today", "tasks", "notes"];
registerIslandCommands(island, { ids: () => IDS, select: (id) => { State.hubTab = id; } });

beforeEach(() => {
  seen.length = 0;
  State.settings = { ...DEFAULT_SETTINGS };
  State.paused = false;
  State.mode = "compact";
  State.hubTab = "today";
});

test("Mochi makes the face she asked for, for the seconds asked (1–8)", () => {
  emit("island-cmd", { kind: "emote", emote: "love", seconds: 5 });
  emit("island-cmd", { kind: "emote", emote: "sad", seconds: 99 });
  emit("island-cmd", { kind: "emote", emote: "happy" });
  assert.deepEqual(seen, [["emote", "love", 5], ["emote", "sad", 8], ["emote", "happy", 3]]);
});

test("a face that is not one of Mochi's, or no face, does nothing", () => {
  emit("island-cmd", { kind: "emote", emote: "explode" });
  emit("island-cmd", { kind: "emote" });
  emit("island-cmd", { kind: "emote", emote: { toString: "x" } });
  assert.deepEqual(seen, []);
});

test("a hidden island comes up first so the face is seen", () => {
  State.mode = "hidden";
  emit("island-cmd", { kind: "emote", emote: "proud" });
  assert.deepEqual(seen, [["reveal"], ["emote", "proud", 3]]);
});

test("a Hub tool opens on the island, and an unknown one does not", () => {
  emit("island-cmd", { kind: "hub", tool: "tasks" });
  assert.equal(State.hubTab, "tasks");
  assert.deepEqual(seen, [["expand", "tool"]]);
  seen.length = 0;
  emit("island-cmd", { kind: "hub", tool: "../../etc" });
  emit("island-cmd", { kind: "hub" });
  assert.equal(State.hubTab, "tasks");
  assert.deepEqual(seen, []);
});

test("a paused island takes no orders", () => {
  State.paused = true;
  emit("island-cmd", { kind: "emote", emote: "love" });
  emit("island-cmd", { kind: "hub", tool: "notes" });
  assert.deepEqual(seen, []);
});

test("an order of a kind the page does not know is ignored", () => {
  emit("island-cmd", { kind: "shell", cmd: "rm -rf /" });
  assert.deepEqual(seen, []);
});
