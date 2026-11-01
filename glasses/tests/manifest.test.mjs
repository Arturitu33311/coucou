import test from "node:test";
import assert from "node:assert/strict";
import { manifest } from "../scripts/make-manifest.mjs";

test("whitelists the exact origin of the bridge and asks for the glasses' microphone", () => {
  const m = manifest("https://jinx.tail1234.ts.net/algo");
  const net = m.permissions.find((p) => p.name === "network");
  assert.deepEqual(net.whitelist, ["https://jinx.tail1234.ts.net"]);
  assert.ok(m.permissions.some((p) => p.name === "g2-microphone"));
  assert.equal(m.package_id, "com.alan.jinx");
});

test("refuses a missing or insecure origin", () => {
  assert.throws(() => manifest(""));
  assert.throws(() => manifest("http://100.64.0.1:8898"), /https/);
});
