// Entry point: boot the bridge, wire the island, start the greeting.

import "./style.css";
import "./music.css";
import "./pill.css";
import "./jinx.css";
import "./hub/hub-island.css";
import { Bridge, IS_TAURI, onEvent } from "./core/bridge";
import { Sound } from "./core/sound";
import { State, type Settings } from "./core/state";
import { Island } from "./island/island";
import { registerHookHandlers } from "./island/hooks";
import { registerJinxHandlers } from "./island/jinx";
import { registerPomodoro } from "./hub/pomodoro";
import { registerTasks, openCount, dueCount } from "./hub/tasks";
import { registerDictation } from "./hub/dictation";
import { registerSysState } from "./island/sysstate";
import { registerMic } from "./island/mic";
import { registerPeripherals } from "./island/peripherals";
import { registerActivities } from "./island/activities";
import { registerNotificationPeek } from "./hub/notifications";
import { registerIntegrationHandlers, refreshConfigured } from "./island/integrations";
import { musicSettingsChanged, registerMusicHandlers } from "./island/music";
import { startSceneSync } from "./mochi/scene";
import { startPresence } from "./core/presence";

// A script error must not vanish into a webview nobody is looking at: it goes to the log.
window.addEventListener("error", (e) => {
  void Bridge.log(`js error: ${e.message} (${e.filename?.split("/").pop()}:${e.lineno})`);
});
window.addEventListener("unhandledrejection", (e) => {
  void Bridge.log(`js rejection: ${String((e.reason && (e.reason.stack || e.reason.message)) ?? e.reason).slice(0, 300)}`);
});
import { registerPhoneLink } from "./island/phone-link";
import { registerShortcutHandlers } from "./island/shortcuts";
import { registerVoiceHandlers } from "./island/voice";
import { applySpotify, registerSpotifyHandlers } from "./island/spotify";
import { SPOTIFY_ID } from "./core/spotify";
import { Recap } from "./recap/recap";
import { onLanguageChange, resolveLanguage, setLanguage, systemLanguages } from "./i18n/i18n";

/** Shows the language Settings asks for ("" = the system's, when Coucou has it). */
function applyLanguage() {
  setLanguage(resolveLanguage(State.settings.language, systemLanguages()));
}

async function main() {
  const root = document.getElementById("root");
  if (!root) return;

  void Sound.preload();

  // Before the island exists: its canvases size their bitmaps from the pixel
  // ratio once, so the zoom correction has to land first. If zoom was adjusted,
  // wait until the ratio updates (or up to 500ms) so nothing is sized against
  // the stale value.
  const dprBefore = window.devicePixelRatio || 1;
  const zoomChanged = await Bridge.fitZoom(dprBefore);
  if (zoomChanged) {
    for (let i = 0; i < 20 && window.devicePixelRatio === dprBefore; i++) {
      await new Promise((r) => setTimeout(r, 25));
    }
  }

  const island = new Island(root);

  const boot = await Bridge.boot();
  if (boot) {
    State.settings = { ...State.settings, ...boot.settings };
  }
  // `npm run dev` in a plain browser: ?outfit=witchHat tries an outfit on the island.
  if (!IS_TAURI) {
    const outfit = new URLSearchParams(window.location.search).get("outfit");
    if (outfit) State.settings.mochiOutfit = outfit;
    // ?mic=Zoom,Chromium pretends those programs record the microphone.
    const mic = new URLSearchParams(window.location.search).get("mic");
    if (mic) State.micApps = mic.split(",");
    // ?music=1 pretends a track is playing (the Music pill is switched on for it).
    if (new URLSearchParams(window.location.search).get("music")) {
      if (!State.settings.activeIntegrations.includes("integration_music")) {
        State.settings.activeIntegrations = [...State.settings.activeIntegrations.slice(0, 3), "integration_music"];
      }
      State.music = {
        player: "mpris", identity: "Spotify", status: "Playing", title: "Dancing Queen", artists: ["ABBA"], album: "Arrival",
        lengthMs: 230000, artUrl: "", positionMs: 60000, positionAt: Date.now(), canSeek: true, canNext: true,
        canPrevious: true, volume: 0.5, key: "x", art: null, rgb: [200, 80, 120], accent: "#FA2D48", lyrics: null, lyricsState: "off",
      };
    }
    // ?acts=Show.S01E01:0.42:active,Movie:0.1:active,Backup:-:error pretends the server is busy.
    const acts = new URLSearchParams(window.location.search).get("acts");
    if (acts) {
      State.activities = acts.split(",").map((s, i) => {
        const [title, p, state] = s.split(":");
        return {
          id: `fake${i}`, app: "Server", kind: state === "error" ? "backup" : "download", title,
          progress: p === "-" ? null : Number(p), eta: p === "-" ? null : 600, state: (state || "active") as "active",
          detail: state === "error" ? "2 d ago" : "",
        };
      });
    }
    // ?devices=headphones:WH-1000XM4:87,gamepad:DualSense:15 pretends those batteries.
    const dev = new URLSearchParams(window.location.search).get("devices");
    if (dev) {
      State.peripherals = dev.split(",").map((s, i) => {
        const [kind, name, pct] = s.split(":");
        return { id: `fake${i}`, kind, name, percent: Number(pct), charging: false };
      });
    }
  }
  // A language change redraws the island in place: the texts given as tl(…)
  // relabel themselves (views/dom.ts) and the views redraw the rest on this
  // sync. Nothing is rebuilt, so tasks, steps and the chat stay as they are.
  onLanguageChange(() => State.notify());
  applyLanguage();
  // Rust shows the tray and its errors in the same language as the webview.
  void Bridge.setSystemLanguages(systemLanguages());
  island.applySettings();
  State.loadIntegrationTasks();
  if (boot && !boot.cursorPoll) island.followPageCursor();
  await island.desktop.init();

  await onEvent<{ x: number; y: number }>("cursor", ({ x, y }) => island.onCursor(x, y));
  // Linux/X11: the pointer anywhere on screen, for Mochi's eyes only.
  await onEvent<{ x: number; y: number }>("gaze", ({ x, y }) => island.onGaze(x, y));
  // Linux/X11: where the buttons of the windows behind the island are.
  await onEvent<number[][]>("obstacles", (zones) => island.setObstacles(zones));
  await onEvent<boolean>("pointer-inside", (inside) => island.setPointerInside(inside));

  /** Pause has to reach Rust too, or the pollers keep calling out. */
  const setPaused = (on: boolean) => {
    if (State.paused === on) return;
    State.paused = on;
    void Bridge.setPaused(on);
  };

  await onEvent<string>("tray", (what) => {
    switch (what) {
      case "settings":
        setPaused(false);
        island.alert("settings");
        break;
      case "open":
        setPaused(false);
        island.alert(State.defaultView());
        break;
      case "toggle":
        setPaused(false);
        if (State.mode === "expanded") island.fsm.forcePetit();
        else island.openFromKeyboard();
        break;
      case "recap":
        setPaused(false);
        void Recap.open(island);
        break;
      case "wardrobe":
        setPaused(false);
        island.alert("wardrobe");
        break;
      case "pause":
        setPaused(!State.paused);
        if (State.paused) island.fsm.forceHidden();
        else island.reveal();
        break;
    }
  });

  const checkZoom = async () => {
    const dpr = window.devicePixelRatio || 1;
    await Bridge.fitZoom(dpr);
  };

  await onEvent<null>("screen-changed", async () => {
    await checkZoom();
    void Bridge.reposition();
  });

  const mediaQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
  mediaQuery.addEventListener?.("change", () => void checkZoom());

  window.addEventListener(
    "wheel",
    (e) => {
      if (e.ctrlKey) e.preventDefault();
    },
    { passive: false }
  );
  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && (e.key === "+" || e.key === "-" || e.key === "=" || e.key === "0")) {
      e.preventDefault();
    }
  });

  // Settings → Reload sounds: read the sounds folder again, and let them hear it.
  await onEvent<null>("sounds-changed", () => {
    void Sound.reload().then(() => Sound.play("pop"));
  });

  // The settings window writes preferences; apply them here without a restart.
  await onEvent<Settings>("settings-changed", (s) => {
    const previousMain = State.mainPillId;
    State.settings = { ...State.settings, ...s };
    applyLanguage();
    island.applySettings();
    State.loadIntegrationTasks();
    // A new main tool comes to the front, as on macOS.
    if (State.mainPillId !== previousMain) State.setFocus(State.mainPillId);
    void refreshConfigured();
    musicSettingsChanged(island);
  });

  registerHookHandlers(island);
  registerPhoneLink(island);
  registerIntegrationHandlers(island);
  registerMusicHandlers(island);
  registerJinxHandlers(island);
  registerPomodoro(island);
  registerTasks(island);
  registerDictation(island);
  registerSysState(island);
  registerMic(island);
  registerPeripherals(island);
  registerActivities(island);
  registerNotificationPeek(island);
  registerShortcutHandlers(island, () => setPaused(false));
  registerVoiceHandlers();
  registerSpotifyHandlers(island);
  // Rust may have read Spotify before this page listened: ask once.
  if (State.settings.activeIntegrations.includes(SPOTIFY_ID)) {
    void Bridge.spotifyRefresh().then((s) => s && applySpotify(island, s));
  }

  // Monday recap: app start (greeting over), an agent starting work, waking up.
  const checkRecap = () => void Recap.check(island);
  island.onGreetingDone = checkRecap;
  island.onWake = checkRecap;
  await onEvent<null>("recap-check", checkRecap);

  island.launch();
  startSceneSync(island.mochi, openCount, dueCount);
  void startPresence();

  // In a plain browser there is no wake strip behind the cursor: make the whole
  // page wake the island so the visuals can be checked with `npm run dev`.
  if (!IS_TAURI) {
    document.addEventListener("click", () => Sound.resume(), { once: true });
  }
}

void main();
