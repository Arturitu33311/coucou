import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { JINX_EMOTES, jinxEmote, replyEmote, validJinxEmote } from "../src/core/jinx-mood.ts";

const vectors = JSON.parse(readFileSync(new URL("../scripts/phone/jinx-mood-vectors.json", import.meta.url), "utf8"));

test("Jinx's reply gives Mochi the emote the phone gives it", () => {
  for (const c of vectors.cases) assert.equal(jinxEmote(c.text), c.emote, c.text);
});

test("nothing said, no emote", () => {
  assert.equal(jinxEmote(null), null);
  assert.equal(jinxEmote(undefined), null);
});

test("the mood Jinx sends with a reply wins over her words", () => {
  assert.equal(replyEmote("sad", "jajaja qué bueno 😂"), "sad");
  assert.equal(replyEmote("laughing", null), "laughing");
});

test("no mood, or one that is not Mochi's, falls back to her words", () => {
  assert.equal(replyEmote(undefined, "jajaja"), "laughing");
  assert.equal(replyEmote(null, "lo siento mucho"), "sad");
  assert.equal(replyEmote("furious", "jajaja"), "laughing");
  assert.equal(replyEmote("Happy", "hola"), "happy"); // not the exact name: ignored, then her words say hola
  assert.equal(replyEmote(3, "ok"), null);
  assert.equal(replyEmote(undefined, "ok"), null);
});

test("the nineteen names are the gateway's list", () => {
  assert.equal(JINX_EMOTES.length, 19);
  assert.equal(new Set(JINX_EMOTES).size, 19);
  for (const e of JINX_EMOTES) assert.equal(validJinxEmote(e), e);
  // the PC-only emote names are not Jinx moods
  for (const e of ["wink", "yawn", "annoyed", "dance"]) assert.equal(validJinxEmote(e), null);
});
