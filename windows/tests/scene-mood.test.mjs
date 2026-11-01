import { test } from "node:test";
import assert from "node:assert/strict";
import { wornEmote } from "../src/mochi/scene.ts";

test("calm, he wears Jinx's background mood; anything else keeps its own face", () => {
  assert.equal(wornEmote("idle", "sad"), "sad");
  assert.equal(wornEmote("idle", null), null);
  assert.equal(wornEmote("idle", "Sad"), null, "only Mochi's own emote names");
  assert.equal(wornEmote("dancing", "sad"), "dance");
  assert.equal(wornEmote("sleepy", "happy"), "yawn");
});
