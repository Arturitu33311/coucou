// Whether the desktop player's window is on screen reaches State: pulled once at startup
// (the first event fires before the webview listens) and updated by the event afterwards.
// Desktop Mochi's own `desktop-visible` must not move it.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { emit, sent } from "./tauri.mjs";

installFakeDom();
const { registerMusicHandlers } = await import("../src/island/music.ts");
const { State } = await import("../src/core/state.ts");

test("registerMusicHandlers pulls whether the desktop player is shown and follows the event", async () => {
  registerMusicHandlers(new Proxy({}, { get: () => () => {} }));
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(sent("desktop_player_shown").length, 1);
  assert.equal(sent("desktop_is_visible").length, 0);
  assert.equal(State.desktopPlayerShown, false);
  emit("desktop-player-shown", true);
  assert.equal(State.desktopPlayerShown, true);
  emit("desktop-player-shown", false);
  assert.equal(State.desktopPlayerShown, false);
});

test("Desktop Mochi's desktop-visible is not the player's", () => {
  assert.throws(() => emit("desktop-visible", true), /nothing listens/);
  assert.equal(State.desktopPlayerShown, false);
});
