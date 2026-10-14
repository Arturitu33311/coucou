import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { jinxEmote } from "../src/core/jinx-mood.ts";

const vectors = JSON.parse(readFileSync(new URL("../scripts/phone/jinx-mood-vectors.json", import.meta.url), "utf8"));

test("Jinx's reply gives Mochi the emote the phone gives it", () => {
  for (const c of vectors.cases) assert.equal(jinxEmote(c.text), c.emote, c.text);
});

test("nothing said, no emote", () => {
  assert.equal(jinxEmote(null), null);
  assert.equal(jinxEmote(undefined), null);
});
