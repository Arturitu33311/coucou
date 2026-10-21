import test from "node:test";
import assert from "node:assert/strict";
import { loadSettings, normalizeBase, saveSettings } from "../src/config.ts";

const mem = () => { const m = {}; return { get: async (k) => m[k] ?? "", set: async (k, v) => { m[k] = v; }, m }; };

test("addresses are cleaned up", () => {
  assert.equal(normalizeBase(" jinx.ts.net/ "), "https://jinx.ts.net");
  assert.equal(normalizeBase("https://jinx.ts.net:8443//"), "https://jinx.ts.net:8443");
  assert.equal(normalizeBase(""), null);
  assert.equal(normalizeBase("https://"), null);
});

test("settings round-trip and bad ones are refused", async () => {
  const s = mem();
  assert.equal(await loadSettings(s), null);
  assert.equal(await saveSettings(s, "jinx.ts.net", "  "), null);
  assert.deepEqual(await saveSettings(s, "jinx.ts.net", " k "), { base: "https://jinx.ts.net", key: "k" });
  assert.deepEqual(await loadSettings(s), { base: "https://jinx.ts.net", key: "k" });
  s.m["jinx-g2-settings"] = "{roto";
  assert.equal(await loadSettings(s), null);
});
