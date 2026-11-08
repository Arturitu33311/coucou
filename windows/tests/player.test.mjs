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
    // The pointer resting on it keeps it open too.
    host.el.fire("mouseenter");
    mock.timers.tick(PLAYER_IDLE_MS * 2);
    assert.equal(log.filter(([n]) => n === "collapse").length, 0);
    host.el.fire("mouseleave");
    mock.timers.tick(PLAYER_IDLE_MS - 1);
    assert.equal(log.filter(([n]) => n === "collapse").length, 0);
    mock.timers.tick(2);
    assert.equal(log.filter(([n]) => n === "collapse").length, 1);
  } finally {
    mock.timers.reset();
    State.view = "overview";
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
