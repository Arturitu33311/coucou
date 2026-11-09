// The glass player view (src/views/player.ts): keyboard navigation through the
// results, stale search answers dropped, the bars only while the view is seen,
// YT Music hidden on Windows, and the view folding away after 30 s of nothing.

import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { sent } from "./tauri.mjs";

installFakeDom();

const { nextIndex, QueryGate, DEBOUNCE_MS, PLAYER_IDLE_MS, sourceOptions, effectiveSource, buildPlayerView } =
  await import("../src/views/player.ts");
const { barsWantedFor, setBarsWanted } = await import("../src/island/music.ts");
const { State } = await import("../src/core/state.ts");

const LINUX_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15";
const WINDOWS_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/120";

function fakeActions() {
  const log = [];
  return {
    log,
    actions: new Proxy({}, { get: (_, name) => (...args) => log.push([name, ...args]) }),
  };
}

test("nextIndexWraps", () => {
  assert.equal(nextIndex(-1, 5, "ArrowDown"), 0);
  assert.equal(nextIndex(0, 5, "ArrowDown"), 1);
  assert.equal(nextIndex(4, 5, "ArrowDown"), 0);
  assert.equal(nextIndex(0, 5, "ArrowUp"), 4);
  assert.equal(nextIndex(-1, 5, "ArrowUp"), 4);
  assert.equal(nextIndex(3, 5, "ArrowUp"), 2);
  assert.equal(nextIndex(0, 0, "ArrowDown"), -1);
  assert.equal(nextIndex(2, 0, "ArrowUp"), -1);
  assert.equal(DEBOUNCE_MS, 250);
});

test("staleResultsAreDropped", () => {
  const gate = new QueryGate();
  const first = gate.next();
  const second = gate.next();
  assert.ok(second > first);
  assert.equal(gate.isCurrent(first), false);
  assert.equal(gate.isCurrent(second), true);
  const third = gate.next();
  assert.equal(gate.isCurrent(second), false);
  assert.equal(gate.isCurrent(third), true);
});

test("barsWantedOnlyWhilePlayerVisible", () => {
  State.focusId = null;
  assert.equal(barsWantedFor("player", "expanded"), true);
  assert.equal(barsWantedFor("player", "hidden"), false);
  assert.equal(barsWantedFor("player", "compact"), State.musicStrip() && State.pillKind() === null);
  // What already wanted them still does.
  assert.equal(barsWantedFor("music", "expanded"), true);
  assert.equal(barsWantedFor("settings", "expanded"), false);
  State.focusId = "integration_music";
  assert.equal(barsWantedFor("overview", "expanded"), true);
  State.focusId = null;
  assert.equal(barsWantedFor("overview", "expanded"), false);

  // And it is what turns cava on and off.
  const before = sent("music_bars").length;
  setBarsWanted(barsWantedFor("player", "expanded"));
  setBarsWanted(barsWantedFor("player", "hidden"));
  assert.deepEqual(sent("music_bars").slice(before).map((a) => a.on), [true, false]);
});

test("selectorHidesYtMusicOnWindows", () => {
  assert.deepEqual(sourceOptions(LINUX_UA), ["spotify", "ytmusic", "both"]);
  assert.deepEqual(sourceOptions(WINDOWS_UA), ["spotify"]);
  assert.equal(effectiveSource("both", WINDOWS_UA), "spotify");
  assert.equal(effectiveSource("ytmusic", WINDOWS_UA), "spotify");
  assert.equal(effectiveSource("both", LINUX_UA), "both");
  assert.equal(effectiveSource("ytmusic", LINUX_UA), "ytmusic");

  // The view built on Windows has no YT Music button; on Linux it does.
  const { actions } = fakeActions();
  const win = buildPlayerView(actions, { userAgent: WINDOWS_UA });
  assert.equal(win.el.find(".pl-src").length, 1);
  assert.ok(!win.el.textContent.includes("YT Music"));
  const linux = buildPlayerView(actions, { userAgent: LINUX_UA });
  assert.equal(linux.el.find(".pl-src").length, 3);
  assert.ok(linux.el.textContent.includes("YT Music"));
});

test("idleCollapsesAfter30s", () => {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: 1_000_000 });
  try {
    assert.equal(PLAYER_IDLE_MS, 30_000);
    const { actions, log } = fakeActions();
    const host = buildPlayerView(actions, { userAgent: LINUX_UA });
    State.mode = "expanded";
    State.view = "player";
    host.sync();
    mock.timers.tick(PLAYER_IDLE_MS - 1000);
    // Typing keeps it open.
    host.el.querySelector(".pl-input").fire("input");
    mock.timers.tick(PLAYER_IDLE_MS - 1000);
    assert.equal(log.filter(([n]) => n === "collapse").length, 0);
    // The pointer resting on it (the island's own pointer state) keeps it open too.
    host.pointer(true);
    mock.timers.tick(PLAYER_IDLE_MS * 2);
    assert.equal(log.filter(([n]) => n === "collapse").length, 0);
    host.pointer(false);
    mock.timers.tick(PLAYER_IDLE_MS - 1);
    assert.equal(log.filter(([n]) => n === "collapse").length, 0);
    mock.timers.tick(2);
    assert.equal(log.filter(([n]) => n === "collapse").length, 1);
  } finally {
    mock.timers.reset();
    State.view = "overview";
  }
});

test("idleStillCollapsesAfterClosingWithThePointerInside", () => {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: 2_000_000 });
  const collapses = (log) => log.filter(([n]) => n === "collapse").length;
  try {
    const { actions, log } = fakeActions();
    const host = buildPlayerView(actions, { userAgent: LINUX_UA });
    State.mode = "expanded";
    State.view = "player";
    State.pointerInIsland = true;
    host.sync();
    host.focus();
    host.pointer(true);
    // Closed with the pointer still on it: no "pointer left" ever reaches the page.
    State.mode = "hidden";
    mock.timers.tick(PLAYER_IDLE_MS + 10);
    assert.equal(collapses(log), 0);
    // Reopened from the keyboard, the pointer elsewhere.
    State.pointerInIsland = false;
    State.mode = "expanded";
    host.sync();
    host.focus();
    mock.timers.tick(PLAYER_IDLE_MS + 10);
    assert.equal(collapses(log), 1);

    // Same, but reopened before the idle timer noticed it was gone.
    State.pointerInIsland = true;
    host.pointer(true);
    host.focus();
    State.mode = "hidden";
    mock.timers.tick(1000);
    State.pointerInIsland = false;
    State.mode = "expanded";
    host.sync();
    host.focus();
    mock.timers.tick(PLAYER_IDLE_MS + 10);
    assert.equal(collapses(log), 2);
  } finally {
    mock.timers.reset();
    State.view = "overview";
    State.mode = "hidden";
    State.pointerInIsland = false;
  }
});

test("Esc goes back to the music view when the player was opened from it, else folds the island", () => {
  const { actions, log } = fakeActions();
  const host = buildPlayerView(actions, { userAgent: LINUX_UA });
  State.playerFrom = "music";
  host.escape();
  assert.deepEqual(log.at(-1), ["setView", "music"]);
  State.playerFrom = "overview";
  host.escape();
  assert.deepEqual(log.at(-1), ["collapse"]);
  // On an artist, Esc first goes back to the results.
  host.openArtist({ source: "spotify", kind: "artist", id: "a1", title: "Daft Punk", subtitle: "", artUrl: "" });
  assert.ok(host.el.textContent.includes("Daft Punk"));
  const count = log.length;
  host.escape();
  assert.equal(log.length, count);
  State.playerFrom = null;
});

// ── Task 9: the cover flow, recents, the DJ and the artist page ───────────────

const { coverflowTransform, buildCoverflow } = await import("../src/views/coverflow.ts");
const { internals } = await import("./tauri.mjs");

const album = (n) => ({ source: "spotify", kind: "album", id: `al${n}`, title: `Album ${n}`, subtitle: "X", artUrl: "" });
const track = (n, source = "spotify") => ({ source, kind: "track", id: `t${n}`, title: `Song ${n}`, subtitle: "X", artUrl: "" });
const flush = () => new Promise((r) => setImmediate(r));
const press = (el, key, target = el) => {
  const e = { key, target, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, stopPropagation() {} };
  for (const fn of el.listeners.get("keydown") ?? []) fn(e);
  return e;
};

/** Runs `fn` with these commands answered by `answers` (the rest by the stand-in). */
async function withAnswers(answers, fn) {
  const real = internals.invoke;
  internals.invoke = async (cmd, args) => {
    const r = await real(cmd, args);
    if (!(cmd in answers)) return r;
    return typeof answers[cmd] === "function" ? answers[cmd](args) : answers[cmd];
  };
  try {
    await fn();
  } finally {
    internals.invoke = real;
  }
}

test("coverflowCenterIsFlat", () => {
  const c = coverflowTransform(3, 3);
  assert.equal(c.scale, 1);
  assert.equal(c.rotateY, 0);
  assert.equal(c.x, 0);
  assert.equal(c.opacity, 1);
  for (const other of [2, 4, 0, 7]) assert.ok(coverflowTransform(other, 3).z < c.z, "the centre is in front");
});

test("coverflowNeighboursTurnInward", () => {
  const left = coverflowTransform(2, 3);
  const right = coverflowTransform(4, 3);
  assert.equal(left.scale, 0.8);
  assert.equal(right.scale, 0.8);
  assert.equal(left.rotateY, 45);
  assert.equal(right.rotateY, -45);
  assert.ok(left.x < 0 && right.x > 0);
  assert.equal(left.x, -right.x);
  // Farther ones are smaller, keep turning inward and keep stacking outward.
  const far = coverflowTransform(0, 3);
  assert.equal(far.scale, 0.65);
  assert.equal(far.rotateY, 45);
  assert.equal(coverflowTransform(6, 3).rotateY, -45);
  assert.ok(far.x < left.x);
});

test("coverflowFarItemsFadeOut", () => {
  for (let d = 0; d <= 4; d++) assert.ok(coverflowTransform(10 + d, 10).opacity > 0, `|d|=${d} still seen`);
  for (let d = 1; d <= 4; d++) {
    assert.ok(coverflowTransform(10 + d, 10).opacity <= coverflowTransform(10 + d - 1, 10).opacity, "fades outward");
  }
  assert.equal(coverflowTransform(15, 10).opacity, 0);
  assert.equal(coverflowTransform(5, 10).opacity, 0);
  assert.equal(coverflowTransform(30, 10).opacity, 0);
});

test("moveClampsAtEnds", () => {
  const picked = [];
  const cf = buildCoverflow([album(0), album(1), album(2)], (it) => picked.push(it.id));
  assert.equal(cf.center(), 0);
  cf.move(-1);
  assert.equal(cf.center(), 0);
  cf.move(1);
  assert.equal(cf.center(), 1);
  cf.move(10);
  assert.equal(cf.center(), 2);
  cf.move(1);
  assert.equal(cf.center(), 2);
  // The arrows move it, Enter picks the centre one.
  press(cf.el, "ArrowLeft");
  assert.equal(cf.center(), 1);
  press(cf.el, "Enter");
  assert.deepEqual(picked, ["al1"]);
  // A click on a side cover brings it to the centre; a click on the centre picks it.
  const covers = cf.el.find(".cf-cover");
  assert.equal(covers.length, 3);
  covers[2].fire("click");
  assert.equal(cf.center(), 2);
  assert.deepEqual(picked, ["al1"]);
  covers[2].fire("click");
  assert.deepEqual(picked, ["al1", "al2"]);
  // Nothing to show is not a crash.
  const empty = buildCoverflow([], () => picked.push("x"));
  assert.equal(empty.center(), -1);
  empty.move(1);
  press(empty.el, "Enter");
  assert.equal(empty.center(), -1);
  assert.equal(picked.length, 2);
});

test("recentsLoadOncePerShowWithoutPolling", async () => {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: 3_000_000 });
  try {
    await withAnswers({ player_recents: [track(1), track(2, "ytmusic")] }, async () => {
      const { actions } = fakeActions();
      const host = buildPlayerView(actions, { userAgent: LINUX_UA });
      const before = sent("player_recents").length;
      State.mode = "expanded";
      State.view = "player";
      host.sync();
      host.focus();
      await flush();
      assert.equal(sent("player_recents").length, before + 1);
      assert.ok(host.el.textContent.includes("Song 1"));
      assert.ok(host.el.textContent.includes("Song 2"));
      // No polling while it stays open.
      host.sync();
      mock.timers.tick(PLAYER_IDLE_MS - 1000);
      host.sync();
      assert.equal(sent("player_recents").length, before + 1);
      // Shut and shown again: once more.
      State.mode = "hidden";
      mock.timers.tick(PLAYER_IDLE_MS);
      State.mode = "expanded";
      host.sync();
      host.focus();
      assert.equal(sent("player_recents").length, before + 2);
    });
  } finally {
    mock.timers.reset();
    State.view = "overview";
    State.mode = "hidden";
  }
});

test("djRowOnlyWithSpotifyAndEnterStartsIt", async () => {
  const saved = State.settings.playerSource;
  try {
    const { actions } = fakeActions();
    State.settings.playerSource = "ytmusic";
    const yt = buildPlayerView(actions, { userAgent: LINUX_UA });
    yt.sync();
    assert.equal(yt.el.find(".pl-dj")[0]?.style.display, "none");
    // Choosing Spotify in the selector brings it back.
    yt.el.find(".pl-src").find((b) => b.getAttribute("data-src") === "spotify").fire("click");
    assert.notEqual(yt.el.find(".pl-dj")[0].style.display, "none");

    State.settings.playerSource = "both";
    const host = buildPlayerView(actions, { userAgent: LINUX_UA });
    host.sync();
    assert.notEqual(host.el.find(".pl-dj")[0].style.display, "none");
    assert.ok(host.el.textContent.includes("DJ"));
    // Windows is Spotify only: the DJ is there.
    const win = buildPlayerView(actions, { userAgent: WINDOWS_UA });
    win.sync();
    assert.notEqual(win.el.find(".pl-dj")[0].style.display, "none");

    // From the empty field: Down lands on the DJ, Enter starts it.
    const input = host.el.querySelector(".pl-input");
    const before = sent("player_dj").length;
    press(input, "ArrowDown", input);
    assert.ok(host.el.find(".pl-dj")[0].classList.contains("sel"));
    press(input, "Enter", input);
    await flush();
    assert.equal(sent("player_dj").length, before + 1);
  } finally {
    State.settings.playerSource = saved;
  }
});

test("artistPageShowsAlbumsAndSongsAndGoesBack", async () => {
  const artistItem = { source: "spotify", kind: "artist", id: "a1", title: "Daft Punk", subtitle: "", artUrl: "" };
  await withAnswers(
    {
      player_artist: { tracks: [track(1), track(2), track(3)], albums: [album(0), album(1)] },
      player_album: [track(10), track(11)],
    },
    async () => {
      const { actions, log } = fakeActions();
      State.settings.playerSource = "both";
      const host = buildPlayerView(actions, { userAgent: LINUX_UA });
      host.sync();
      host.openArtist(artistItem);
      await flush();
      // The artist's own source is asked, never "both".
      assert.deepEqual(sent("player_artist").at(-1), { source: "spotify", id: "a1", name: "Daft Punk" });
      const pane = host.el.querySelector(".pl-artist-pane");
      assert.equal(pane.find(".cf-cover").length, 2);
      assert.ok(pane.textContent.includes("Song 3"));

      // Enter on the cover flow opens the centre album's songs.
      const flow = pane.querySelector(".cf");
      press(flow, "ArrowRight");
      press(flow, "Enter");
      await flush();
      assert.deepEqual(sent("player_album").at(-1), { source: "spotify", id: "al1" });
      assert.ok(pane.textContent.includes("Song 11"));

      // Backspace while typing in a non-empty field edits the text: still on the artist.
      const input = host.el.querySelector(".pl-input");
      input.value = "daft";
      const typed = press(input, "Backspace", input);
      assert.equal(typed.defaultPrevented, false);
      assert.notEqual(pane.style.display, "none");
      // The root sees that same key bubble up and leaves it alone too.
      press(host.el, "Backspace", input);
      assert.notEqual(pane.style.display, "none");
      // Backspace elsewhere (the cover flow has the focus): back to the results.
      press(host.el, "Backspace", flow);
      assert.equal(pane.style.display, "none");

      // And with the field empty, Backspace in it goes back too.
      host.openArtist(artistItem);
      await flush();
      input.value = "";
      press(input, "Backspace", input);
      assert.equal(pane.style.display, "none");
      // Esc on the artist page goes back, not to Music or a fold.
      host.openArtist(artistItem);
      const count = log.length;
      host.escape();
      assert.equal(log.length, count);
      assert.equal(pane.style.display, "none");
    },
  );
});

test("artistPageWithNothingDoesNotCrash", async () => {
  const { actions } = fakeActions();
  const host = buildPlayerView(actions, { userAgent: LINUX_UA });
  host.openArtist({ source: "ytmusic", kind: "artist", id: "a2", title: "Nobody", subtitle: "", artUrl: "" });
  await flush(); // player_artist answers null outside Tauri / on error
  const pane = host.el.querySelector(".pl-artist-pane");
  assert.equal(pane.find(".cf-cover").length, 0);
  assert.ok(pane.textContent.includes("Nobody"));
});
