// The desktop-visible flag reaches State: pulled once at startup (the first event fires
// before the webview listens) and updated by the event afterwards.

import { test } from "node:test";
import assert from "node:assert/strict";
import { installFakeDom } from "./fakedom.mjs";
import { emit, sent } from "./tauri.mjs";

installFakeDom();
const { registerMusicHandlers } = await import("../src/island/music.ts");
const { State } = await import("../src/core/state.ts");

test("registerMusicHandlers pulls the initial desktop visibility and follows the event", async () => {
  registerMusicHandlers(new Proxy({}, { get: () => () => {} }));
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(sent("desktop_is_visible").length, 1);
  assert.equal(State.desktopVisible, false);
  emit("desktop-visible", true);
  assert.equal(State.desktopVisible, true);
  emit("desktop-visible", false);
  assert.equal(State.desktopVisible, false);
});
