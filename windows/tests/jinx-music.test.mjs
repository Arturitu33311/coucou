// This fork keeps the music with Jinx: her card shows what plays, and the minimised island carries the music
// strip with Jinx's pill alone (no Music pill needed).

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { sent } from "./tauri.mjs";

installFakeDom();
const { buildJinxCard } = await import("../src/views/jinx.ts");
const { DEFAULT_SETTINGS, JINX_ID, MUSIC_ID, State } = await import("../src/core/state.ts");

const track = (status) => ({ title: "Getaway Car", artists: ["Taylor Swift"], status, positionMs: 0, positionAt: Date.now(), lengthMs: 1 });

function card() {
  const views = [];
  const c = buildJinxCard({ talkToJinx() {}, stopJinx() {}, setView: (v) => views.push(v) });
  return { c, views };
}

test("her card shows the song and plays or pauses it; the line opens the player", () => {
  State.music = track("Playing");
  const { c, views } = card();
  c.sync();
  const text = c.el.textContent;
  assert.ok(text.includes("Getaway Car — Taylor Swift"), text);
  const music = c.el.querySelector(".jx-music");
  assert.notEqual(music.style.display, "none");
  c.el.querySelector(".jx-play").fire("click");
  assert.deepEqual(sent("music_control").at(-1), { action: "playpause", valueMs: null });
  assert.deepEqual(views, []); // the button does not open the player
  music.fire("click");
  assert.deepEqual(views, ["music"]);
});

test("no song, no line", () => {
  State.music = null;
  const { c } = card();
  c.sync();
  assert.equal(c.el.querySelector(".jx-music").style.display, "none");
});

test("the strip rides on Jinx's pill without a Music pill", () => {
  State.settings = { ...DEFAULT_SETTINGS };
  State.paused = false;
  State.tasks = [{ id: JINX_ID, name: "Jinx", color: "#10B981", state: "idle", stepIndex: 0, steps: [] }];
  State.music = track("Playing");
  assert.equal(State.musicStrip(), true);
  State.tasks = [{ id: "integration_github", name: "GitHub", color: "#fff", state: "idle", stepIndex: 0, steps: [] }];
  assert.equal(State.musicStrip(), false);
  State.tasks.push({ id: MUSIC_ID, name: "Music", color: "#FA2D48", state: "idle", stepIndex: 0, steps: [] });
  assert.equal(State.musicStrip(), true);
});
