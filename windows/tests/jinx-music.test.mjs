// This fork keeps the music with Jinx: her card shows what plays, and the minimised island carries the music
// strip with Jinx's pill alone (no Music pill needed).

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { sent } from "./tauri.mjs";

installFakeDom();
const { buildJinxCard, buildJinxMusicFace } = await import("../src/views/jinx.ts");
const { islandDances, desktopDances } = await import("../src/core/spotify.ts");
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

// ── The combined card: the player has the floor unless she is busy ───────────

const jinxTask = (state = "idle") => ({ id: JINX_ID, name: "Jinx", color: "#39FF14", state, stepIndex: 0, steps: [] });

function quiet() {
  State.settings = { ...DEFAULT_SETTINGS };
  State.tasks = [jinxTask()];
  State.music = track("Playing");
  State.jinxBusy = false;
  State.jinxReplyAt = 0;
  State.pendingApproval = null;
  State.pendingQuestion = null;
}

test("music plays and Jinx is quiet: the player leads", () => {
  quiet();
  assert.equal(State.musicLeadsJinx(), true);
});

test("Jinx working, asking for approval or with a reply under two minutes old: her own card", () => {
  quiet();
  State.tasks = [jinxTask("working")];
  assert.equal(State.musicLeadsJinx(), false, "working");
  quiet();
  State.jinxBusy = true;
  assert.equal(State.musicLeadsJinx(), false, "a message is on its way");
  quiet();
  State.pendingApproval = { pillId: JINX_ID };
  assert.equal(State.musicLeadsJinx(), false, "asking for approval");
  quiet();
  const now = Date.now();
  State.jinxReplyAt = now - 60_000;
  assert.equal(State.musicLeadsJinx(now), false, "a reply a minute old");
  State.jinxReplyAt = now - 121_000;
  assert.equal(State.musicLeadsJinx(now), true, "the reply has aged out");
  State.pendingApproval = null;
});

test("paused or stopped music never leads", () => {
  quiet();
  State.music = track("Paused");
  assert.equal(State.musicLeadsJinx(), false);
  State.music = null;
  assert.equal(State.musicLeadsJinx(), false);
});

test("the combined face is a minimal header of hers over the player, and Talk reaches her", () => {
  quiet();
  const talks = [];
  const player = { el: { tag: "player" }, sync() {} };
  const face = buildJinxMusicFace({ talkToJinx: () => talks.push(1), stopJinx() {}, setView() {} }, player);
  face.sync();
  const text = face.el.textContent;
  assert.ok(text.includes("Jinx"), text);
  assert.ok(!text.includes("Talk to Jinx"), "the full card's long button is not here");
  face.el.querySelector(".jx-mini-talk").fire("click");
  assert.deepEqual(talks, [1]);
});

test("Mochi dances to any player, with Jinx's card in front", () => {
  quiet();
  assert.equal(State.musicPlaying, true);
  State.music = track("Paused");
  assert.equal(State.musicPlaying, false);
  const dance = { music: true, state: "idle", mode: "expanded", view: "overview" };
  assert.equal(islandDances({ ...dance, focusId: JINX_ID }), true);
  assert.equal(islandDances({ ...dance, focusId: "integration_github" }), false);
  assert.equal(islandDances({ ...dance, mode: "compact", focusId: null }), true);
  assert.equal(desktopDances(true, "idle"), true);
});

test("the cover's colour paints Jinx's pill and gives it back when the music stops", async () => {
  quiet();
  const { musicSettingsChanged } = await import("../src/island/music.ts");
  State.music = { ...track("Playing"), accent: "#112233" };
  musicSettingsChanged({ setView() {}, defaultView() {}, refreshMusicStrip() {} });
  assert.equal(State.tasks[0].color, "#112233");
  // paused: her own green again (the cover's colour is only for while it plays)
  State.music = { ...track("Paused"), accent: "#112233" };
  musicSettingsChanged({ setView() {}, defaultView() {}, refreshMusicStrip() {} });
  assert.equal(State.tasks[0].color, "#39FF14");
});
