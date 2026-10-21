import test from "node:test";
import assert from "node:assert/strict";
import { createClient, describe } from "../src/client.ts";

const res = (status, body) => ({ ok: status >= 200 && status < 300, status, json: async () => body });

function fake(routes) {
  const calls = [];
  const f = async (url, init = {}) => {
    calls.push({ url, ...init });
    const key = `${init.method || "GET"} ${new URL(url).pathname}`;
    const r = routes[key];
    if (!r) throw new Error("sin ruta " + key);
    return typeof r === "function" ? r(url, init) : r;
  };
  return { f, calls };
}

test("ask returns a finished answer and sends the key", async () => {
  const { f, calls } = fake({ "POST /api/g2/ask": res(200, { id: "a", status: "done", text: "hola" }) });
  const c = createClient({ base: "https://x.ts.net/", key: "K" }, f);
  const r = await c.ask("qué tal");
  assert.equal(r.text, "hola");
  assert.equal(calls[0].url, "https://x.ts.net/api/g2/ask");
  assert.equal(calls[0].headers.Authorization, "Bearer K");
  assert.equal(JSON.parse(calls[0].body).text, "qué tal");
});

test("a pending answer is followed until done", async () => {
  let n = 0;
  const { f, calls } = fake({
    "POST /api/g2/ask": res(200, { id: "a b", status: "pending" }),
    "GET /api/g2/answer": () => res(200, ++n < 3 ? { id: "a b", status: "pending" } : { id: "a b", status: "done", text: "listo" }),
  });
  const r = await createClient({ base: "https://x", key: "K" }, f).ask("x");
  assert.equal(r.text, "listo");
  assert.equal(calls.length, 4);
  assert.match(calls[1].url, /id=a%20b&wait=25/);
});

test("a pending answer that never ends times out", async () => {
  let t = 0;
  const { f } = fake({
    "POST /api/g2/ask": res(200, { id: "a", status: "pending" }),
    "GET /api/g2/answer": () => { t += 3 * 60 * 1000; return res(200, { id: "a", status: "pending" }); },
  });
  await assert.rejects(createClient({ base: "https://x", key: "K" }, f, () => t).ask("x"), { kind: "timeout" });
});

test("errors are classified and described for the glasses", async () => {
  const mk = (routes, fn = "ask") => createClient({ base: "https://x", key: "K" }, fake(routes).f)[fn](new Uint8Array(4), undefined);
  await assert.rejects(mk({ "POST /api/g2/ask": res(401, {}) }), { kind: "auth" });
  await assert.rejects(mk({ "POST /api/g2/ask": res(500, { error: "roto" }) }), { kind: "server", message: "roto" });
  await assert.rejects(mk({}), { kind: "network" });
  await assert.rejects(mk({ "POST /api/g2/voice": res(503, { error: "no hay motor" }) }, "voice"), { kind: "voice" });
  await assert.rejects(mk({ "POST /api/g2/ask": res(200, { id: "a", status: "error", error: "modelo caído" }) }),
    { message: "modelo caído" });
  assert.equal(describe({ kind: "network" }), "Sin conexión con Jinx");
  assert.equal(describe({ kind: "aborted" }), "");
});

test("an aborted request is reported as aborted, not as a network failure", async () => {
  const ctl = new AbortController();
  const f = async () => { ctl.abort(); throw new Error("abort"); };
  await assert.rejects(createClient({ base: "https://x", key: "K" }, f).ask("x", ctl.signal), { kind: "aborted" });
});

test("voice: empty transcript comes back as is; heard text survives following", async () => {
  const empty = createClient({ base: "https://x", key: "K" }, fake({ "POST /api/g2/voice": res(200, { heard: "", status: "empty" }) }).f);
  assert.equal((await empty.voice(new Uint8Array(4))).status, "empty");
  const calls = [];
  const c = createClient({ base: "https://x", key: "K" }, fake({
    "POST /api/g2/voice": (u, i) => { calls.push(i); return res(200, { id: "v", status: "pending", heard: "hola" }); },
    "GET /api/g2/answer": res(200, { id: "v", status: "done", text: "qué tal" }),
  }).f);
  const r = await c.voice(new Uint8Array([1, 2]));
  assert.deepEqual([r.heard, r.text], ["hola", "qué tal"]);
  assert.equal(calls[0].headers["Content-Type"], "application/octet-stream");
});

test("notices", async () => {
  const { f, calls } = fake({ "GET /api/g2/notices": res(200, { notices: [{ id: 5, text: "x" }] }) });
  const items = await createClient({ base: "https://x", key: "K" }, f).notices(4, 20);
  assert.equal(items[0].id, 5);
  assert.match(calls[0].url, /since=4&wait=20/);
});
