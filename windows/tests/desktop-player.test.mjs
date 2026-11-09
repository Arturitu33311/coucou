// The desktop player's page (desktop.html, src/desktop-player): the 12 h clock's
// text, the glass gauge's labels and the scene's colours, with and without music.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";

installFakeDom();

const { clockText } = await import("../src/desktop-player/clock.ts");
const { gaugeLabel, RING } = await import("../src/desktop-player/gauge.ts");
const { sceneColors, hourTint, TINT_A, TINT_B } = await import("../src/desktop-player/scene.ts");
const { readableAccent, musicTint } = await import("../src/island/music.ts");
const { setLanguage } = await import("../src/i18n/i18n.ts");

// ── Clock ─────────────────────────────────────────────────────────────────────

test("clockText is 12 h with no leading zero on the hour", () => {
  assert.equal(clockText(new Date(2026, 10, 9, 7, 5)), "7:05");
  assert.equal(clockText(new Date(2026, 10, 9, 19, 45)), "7:45");
  assert.equal(clockText(new Date(2026, 10, 9, 0, 0)), "12:00");
  assert.equal(clockText(new Date(2026, 10, 9, 12, 9)), "12:09");
  assert.equal(clockText(new Date(2026, 10, 9, 10, 30)), "10:30");
  assert.equal(clockText(new Date(2026, 10, 9, 23, 59)), "11:59");
});

// ── Gauge ─────────────────────────────────────────────────────────────────────

test("gaugeLabel: nothing known yet hides the pill", () => {
  assert.equal(gaugeLabel(null), null);
});

test("gaugeLabel: charging, plugged, on battery", () => {
  const charging = gaugeLabel({ battery: 100, plugged: true, charging: true, online: true });
  assert.equal(charging.pct, "100");
  assert.equal(charging.state, "Charging");
  assert.equal(charging.net, "Online");
  assert.equal(charging.offset, 0);
  assert.equal(charging.low, false);

  const plugged = gaugeLabel({ battery: 80, plugged: true, charging: false, online: true });
  assert.equal(plugged.state, "Plugged in");
  assert.ok(Math.abs(plugged.offset - RING * 0.2) < 1e-9);

  const battery = gaugeLabel({ battery: 64, plugged: false, charging: false, online: true });
  assert.equal(battery.state, "On battery");
  assert.equal(battery.pct, "64");
});

test("gaugeLabel: a low battery not charging is marked low; charging it is not", () => {
  assert.equal(gaugeLabel({ battery: 20, plugged: false, charging: false, online: true }).low, true);
  assert.equal(gaugeLabel({ battery: 21, plugged: false, charging: false, online: true }).low, false);
  assert.equal(gaugeLabel({ battery: 5, plugged: true, charging: true, online: true }).low, false);
});

test("gaugeLabel: no battery (a desktop PC) shows no number and a full ring", () => {
  const g = gaugeLabel({ battery: null, plugged: true, charging: false, online: true });
  assert.equal(g.pct, "");
  assert.equal(g.offset, 0);
  assert.equal(g.state, "Plugged in");
  assert.equal(g.hasBattery, false);
});

test("gaugeLabel: offline, and out-of-range battery values are clamped", () => {
  assert.equal(gaugeLabel({ battery: 50, plugged: false, charging: false, online: false }).net, "Offline");
  assert.equal(gaugeLabel({ battery: 140, plugged: true, charging: false, online: true }).pct, "100");
  assert.equal(gaugeLabel({ battery: -3, plugged: false, charging: false, online: true }).offset, RING);
});

test("gaugeLabel speaks the interface language", () => {
  setLanguage("es");
  try {
    const g = gaugeLabel({ battery: 100, plugged: true, charging: true, online: false });
    assert.equal(g.state, "Cargando");
    assert.equal(g.net, "Sin conexión");
  } finally {
    setLanguage("en");
  }
});

// ── Scene ─────────────────────────────────────────────────────────────────────

const mix = (a, b, k) => a.map((v, i) => v + (b[i] - v) * k);

test("sceneColors without music follows the hour, as the wallpaper does", () => {
  const midnight = sceneColors(null, 0);
  const lateNight = sceneColors(null, 24);
  assert.deepEqual(midnight, lateNight);
  // 13:00 is a key: the wallpaper's base mixed 90 % towards that key's colour
  const noon = sceneColors(null, 13);
  assert.deepEqual(noon.a.map(Math.round), mix([20, 120, 110], TINT_A[3], 0.9).map(Math.round));
  assert.deepEqual(noon.b.map(Math.round), mix([30, 90, 70], TINT_B[3], 0.9).map(Math.round));
  assert.equal(noon.wash, null);
  assert.match(noon.accent, /^#[0-9a-f]{6}$/);
  // between keys it blends
  const h = hourTint(TINT_A, 10.5);
  assert.deepEqual(h.map(Math.round), mix(TINT_A[2], TINT_A[3], 0.5).map(Math.round));
});

test("sceneColors with a cover uses its colour and ignores the hour", () => {
  const rgb = [200, 40, 60];
  const a = sceneColors(rgb, 3);
  const b = sceneColors(rgb, 15);
  assert.deepEqual(a, b);
  assert.equal(a.accent, readableAccent(rgb));
  assert.equal(a.wash, musicTint({ rgb }).bg);
  assert.match(a.glow, /^rgba\(\d+,\d+,\d+,0\.55\)$/);
  assert.match(a.soft, /^rgba\(\d+,\d+,\d+,0\.22\)$/);
  assert.equal(a.clock.length, 3);
});

test("sceneColors: the hour and the cover give different scenes", () => {
  assert.notDeepEqual(sceneColors(null, 13), sceneColors([200, 40, 60], 13));
});
