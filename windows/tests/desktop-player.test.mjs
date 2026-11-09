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
const { recentRowStyle } = await import("../src/desktop-player/recents.ts");
const { flowArtist, flowTransform } = await import("../src/desktop-player/flow.ts");
const { QueryGate, nextIndex } = await import("../src/desktop-player/search.ts");
const { formatTime } = await import("../src/desktop-player/bar.ts");

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

// ── Interactive panels (Task 5) ───────────────────────────────────────────────

test("recentRowsTiltWithoutDepth", () => {
  for (let i = 0; i < 6; i++) {
    const s = recentRowStyle(i);
    assert.equal(s.transform, "rotateY(22deg)");
    assert.ok(!s.transform.includes("translateZ"));
    assert.ok(s.opacity <= 1 && s.opacity >= 0.3);
  }
});

test("flowFallsBackToLastArtist", () => {
  const recents = [
    { source: "spotify", kind: "track", id: "1", title: "Instant Crush", subtitle: "Daft Punk", artUrl: "" },
    { source: "ytmusic", kind: "track", id: "2", title: "Bohemian Rhapsody", subtitle: "Queen", artUrl: "" },
  ];
  assert.equal(flowArtist(null, recents), "Daft Punk");
  assert.equal(flowArtist("", recents), "Daft Punk");
  assert.equal(flowArtist("The Weeknd", recents), "The Weeknd");
});

test("flowHiddenWithoutHistory", () => {
  assert.equal(flowArtist(null, []), null);
  assert.equal(flowArtist("", []), null);
  assert.equal(flowArtist("   ", []), null);
});

test("staleResultsAreDropped", () => {
  const gate = new QueryGate();
  const first = gate.next();
  const second = gate.next();
  assert.ok(second > first);
  assert.equal(gate.isCurrent(first), false);
  assert.equal(gate.isCurrent(second), true);
});

test("formatTime formats milliseconds as m:ss", () => {
  assert.equal(formatTime(0), "0:00");
  assert.equal(formatTime(65000), "1:05");
  assert.equal(formatTime(337000), "5:37");
});

// ── Review fixes (Tasks 5+6) ──────────────────────────────────────────────────

const { AlbumSource, FLOW_SCALE, firstArtist } = await import("../src/desktop-player/flow.ts");
const { coverflowTransform } = await import("../src/views/coverflow.ts");
const notch = await import("../src/views/player.ts");
const { firstSelection, DEBOUNCE_MS } = await import("../src/desktop-player/search.ts");
const { progressTimerWanted, lyricShown, SongLists } = await import("../src/desktop-player/logic.ts");
const { Feedback } = await import("../src/desktop-player/feedback.ts");
const { internals } = await import("./tauri.mjs");

const item = (over) => ({ source: "spotify", kind: "track", id: "x", title: "T", subtitle: "", artUrl: "", ...over });

test("flowArtist skips the DJ and lists, and takes the first of a subtitle's artists", () => {
  const recents = [
    item({ kind: "context", title: "DJ", subtitle: "Spotify" }),
    item({ title: "Get Lucky", subtitle: "Daft Punk, Pharrell Williams" }),
  ];
  assert.equal(flowArtist(null, recents), "Daft Punk");
  assert.equal(flowArtist(null, [item({ kind: "artist", title: "Queen" })]), "Queen");
  assert.equal(flowArtist(null, [item({ kind: "context", title: "DJ" })]), null);
  assert.equal(firstArtist("A, B, C"), "A");
});

test("the desktop's cover flow is the notch's coverflowTransform, scaled", () => {
  for (const [i, c] of [[0, 0], [1, 0], [0, 2], [5, 2], [2, 3]]) {
    const p = coverflowTransform(i, c);
    const f = flowTransform(i, c);
    assert.equal(f.size, 300 * p.scale);
    assert.ok(Math.abs(f.left + f.size / 2 - 520 - p.x * FLOW_SCALE) < 1e-9);
    assert.equal(f.rotateY, p.rotateY);
    assert.equal(f.z, p.z * FLOW_SCALE);
    assert.equal(f.opacity, p.opacity);
  }
});

test("AlbumSource asks once per artist, remembers empty and failed answers, drops stale ones", async () => {
  const asked = [];
  const pending = new Map();
  const src = new AlbumSource((artist) => {
    asked.push(artist);
    if (artist === "Fails") return Promise.reject(new Error("offline"));
    return new Promise((resolve) => pending.set(artist, resolve));
  });
  // an artist with no albums is asked once, not on every song
  const empty = src.albumsFor("Nobody");
  pending.get("Nobody")([]);
  assert.deepEqual(await empty, []);
  assert.deepEqual(await src.albumsFor("Nobody"), []);
  assert.deepEqual(await src.albumsFor("Fails"), []);
  assert.deepEqual(await src.albumsFor("Fails"), []);
  assert.deepEqual(asked, ["Nobody", "Fails"]);
  // A is overtaken by B: A's late answer is dropped, but kept for later
  const a = src.albumsFor("A");
  const b = src.albumsFor("B");
  pending.get("B")([item({ kind: "album", title: "B1" })]);
  pending.get("A")([item({ kind: "album", title: "A1" })]);
  assert.equal(await a, null);
  assert.equal((await b)[0].title, "B1");
  assert.equal((await src.albumsFor("A"))[0].title, "A1");
  assert.deepEqual(asked, ["Nobody", "Fails", "A", "B"]);
});

test("SongLists: the lists are read again only when the song changes", () => {
  const s = new SongLists();
  assert.equal(s.changed(""), true); // first call, nothing playing
  assert.equal(s.changed("song-1"), true);
  assert.equal(s.changed("song-1"), false); // pause, seek, cover loaded
  assert.equal(s.changed("song-1"), false);
  assert.equal(s.changed("song-2"), true);
  assert.equal(s.changed(""), true);
});

test("progress timer only while playing and seen; lyric line follows its button", () => {
  assert.equal(progressTimerWanted("Playing", false), true);
  assert.equal(progressTimerWanted("Paused", false), false);
  assert.equal(progressTimerWanted("Playing", true), false);
  assert.equal(progressTimerWanted(undefined, false), false);
  assert.equal(lyricShown("la la", true), true);
  assert.equal(lyricShown("la la", false), false);
  assert.equal(lyricShown("", true), false);
});

test("the search reuses the notch's gate, debounce and arrows, and preselects row 0", () => {
  assert.equal(QueryGate, notch.QueryGate);
  assert.equal(nextIndex, notch.nextIndex);
  assert.equal(DEBOUNCE_MS, notch.DEBOUNCE_MS);
  assert.equal(firstSelection(5), 0);
  assert.equal(firstSelection(0), -1);
});

// Feedback: the notch's foot, on the desktop.

const lines = (fb) => fb.el.children.filter((c) => c.tagName === "SPAN").map((c) => c.textContent);
const buttons = (fb) => fb.el.children.filter((c) => c.tagName === "BUTTON");
const failure = (message, loginNeeded) => Promise.resolve({ ok: false, error: { message, loginNeeded } });

test("footContent: a source's error reads 'Spotify: …', the sign-in offer comes once", () => {
  const base = { notice: null, now: 0, query: "abba", loadingMore: false, loginNeeded: false, spotifyOff: false, source: "both" };
  const c = notch.footContent({
    ...base,
    errors: [
      { source: "spotify", message: "Session expired", loginNeeded: true },
      { source: "ytmusic", message: "yt-dlp missing", loginNeeded: false },
    ],
  });
  assert.deepEqual(c.lines.map((l) => l.text), ["Spotify: Session expired", "YT Music: yt-dlp missing"]);
  assert.equal(c.offerLogin, true);
  // YT Music only: no offer for Spotify's missing session
  assert.equal(notch.footContent({ ...base, query: "", errors: [], spotifyOff: true, source: "ytmusic" }).offerLogin, false);
});

test("Feedback: a failed play says why and offers to connect Spotify once", async () => {
  const fb = new Feedback(() => 1000);
  await fb.start(() => failure("", true), "Couldn't play this", "spotify");
  assert.deepEqual(lines(fb), ["Couldn't play this"]);
  assert.equal(buttons(fb).length, 1);
  assert.equal(buttons(fb)[0].textContent, "Connect Spotify");
  // the search's per-source error is on the same line, still one button
  fb.notice = null;
  fb.side = { query: () => "abba", errors: () => [{ source: "spotify", message: "Login", loginNeeded: true }], source: () => "both", rerun() {} };
  fb.paint();
  assert.deepEqual(lines(fb), ["Spotify: Login"]);
  assert.equal(buttons(fb).length, 1);
});

test("Feedback: with the search closed, errors and the sign-in offer leave the wallpaper; a live notice stays", () => {
  const fb = new Feedback(() => 1000);
  let open = true;
  fb.spotifyOff = true;
  fb.side = { query: () => "abba", errors: () => [{ source: "spotify", message: "Login", loginNeeded: true }], source: () => "both", rerun() {}, open: () => open };
  fb.paint();
  assert.deepEqual(lines(fb), ["Spotify: Login"]);
  assert.equal(buttons(fb).length, 1);
  open = false;
  fb.paint();
  assert.deepEqual(lines(fb), []);
  assert.equal(buttons(fb).length, 0);
  fb.notice = { text: "Played through the Spotify app", tone: "", until: 5000 };
  fb.paint();
  assert.deepEqual(lines(fb), ["Played through the Spotify app"]);
  assert.equal(buttons(fb).length, 0);
});

test("Feedback: the DJ failing says so; a play through the Spotify app says that", async () => {
  const fb = new Feedback(() => 1000);
  await fb.start(() => failure("", false), "Couldn't start the DJ", "spotify");
  assert.deepEqual(lines(fb), ["Couldn't start the DJ"]);
  assert.equal(buttons(fb).length, 0);
  await fb.start(() => Promise.resolve({ ok: true, value: { fallbackUsed: true } }), "Couldn't play this", "spotify");
  assert.deepEqual(lines(fb), ["Played through the Spotify app"]);
});

test("Feedback: one start at a time, 'Starting…' meanwhile", async () => {
  const fb = new Feedback(() => 1000);
  let runs = 0;
  let finish;
  const first = fb.start(() => {
    runs++;
    return new Promise((r) => (finish = r));
  }, "x", "spotify");
  assert.deepEqual(lines(fb), ["Starting…"]);
  await fb.start(() => {
    runs++;
    return Promise.resolve({ ok: true, value: { fallbackUsed: false } });
  }, "x", "spotify");
  assert.equal(runs, 1);
  finish({ ok: true, value: { fallbackUsed: false } });
  await first;
  assert.deepEqual(lines(fb), []);
  assert.equal(fb.el.classList.contains("on"), false);
});

test("Feedback: Connect Spotify reports a failed sign-in, and searches again after one that worked", async () => {
  const fb = new Feedback(() => 1000);
  let reran = 0;
  fb.side = { query: () => "abba", errors: () => [], source: () => "both", rerun: () => reran++ };
  fb.loginNeeded = true;
  const real = internals.invoke;
  internals.invoke = async (cmd, args) => {
    if (cmd === "player_spotify_login") throw "Sign-in was cancelled";
    return real(cmd, args);
  };
  try {
    await fb.connect();
  } finally {
    internals.invoke = real;
  }
  assert.deepEqual(lines(fb), ["Sign-in was cancelled"]);
  assert.equal(reran, 0);
  await fb.connect(); // the stand-in answers null: signed in
  assert.equal(fb.loginNeeded, false);
  assert.equal(reran, 1);
});

// ── Live-test polish (2026-11-09) ─────────────────────────────────────────────

const { fetchArtistAlbums, ytArtist } = await import("../src/desktop-player/flow.ts");

/** A stand-in for the bridge that records the calls and answers from `answers`. */
function lookup(answers) {
  const calls = [];
  return {
    calls,
    playerSearch: async (source, query) => {
      calls.push(["search", source, query]);
      return { seq: 0, items: answers[`search:${source}`] ?? [], errors: [] };
    },
    playerArtist: async (source, id, name) => {
      calls.push(["artist", source, id, name]);
      return answers[`artist:${source}:${id}`] ?? { ok: false, error: { message: "no", loginNeeded: false } };
    },
  };
}

test("fetchArtistAlbums: a search's albums (Spotify's) are used as they are", async () => {
  const albums = [item({ kind: "album", id: "a1", title: "Discovery" })];
  const b = lookup({ "search:both": [item({ kind: "track" }), ...albums] });
  assert.deepEqual(await fetchArtistAlbums("Daft Punk", b), albums);
  assert.deepEqual(b.calls, [["search", "both", "Daft Punk"]]);
});

test("fetchArtistAlbums: with no albums, the YT Music artist's page gives them", async () => {
  const ytAlbums = [item({ source: "ytmusic", kind: "album", id: "MPRE1", title: "Random Access Memories" })];
  const b = lookup({
    "search:both": [
      item({ source: "ytmusic", kind: "track", id: "v1", title: "Instant Crush" }),
      item({ source: "ytmusic", kind: "artist", id: "UCother", title: "Daft Punk Tribute" }),
      item({ source: "ytmusic", kind: "artist", id: "UCdp", title: "Daft Punk" }),
    ],
    "artist:ytmusic:UCdp": { ok: true, value: { tracks: [], albums: ytAlbums } },
  });
  assert.deepEqual(await fetchArtistAlbums("Daft Punk", b), ytAlbums);
  assert.deepEqual(b.calls, [["search", "both", "Daft Punk"], ["artist", "ytmusic", "UCdp", "Daft Punk"]]);
});

test("fetchArtistAlbums: no YT Music artist in the first search asks YT Music alone; none at all is no albums", async () => {
  const ytAlbums = [item({ source: "ytmusic", kind: "album", id: "MPRE2" })];
  const b = lookup({
    "search:ytmusic": [item({ source: "ytmusic", kind: "artist", id: "UCq", title: "Queen" })],
    "artist:ytmusic:UCq": { ok: true, value: { tracks: [], albums: ytAlbums } },
  });
  assert.deepEqual(await fetchArtistAlbums("Queen", b), ytAlbums);
  assert.deepEqual(b.calls.map((c) => c.slice(0, 2)), [["search", "both"], ["search", "ytmusic"], ["artist", "ytmusic"]]);

  const none = lookup({});
  assert.deepEqual(await fetchArtistAlbums("Nobody", none), []);
  assert.equal(none.calls.some((c) => c[0] === "artist"), false, "never asks for a page with no artist");
});

test("ytArtist: the exact name first, else the first YT Music artist; Spotify artists are not taken", () => {
  const list = [
    item({ source: "spotify", kind: "artist", id: "s", title: "Daft Punk" }),
    item({ source: "ytmusic", kind: "artist", id: "y1", title: "Daft Punk Live" }),
    item({ source: "ytmusic", kind: "artist", id: "y2", title: "daft punk " }),
  ];
  assert.equal(ytArtist(list, "Daft Punk").id, "y2");
  assert.equal(ytArtist(list, "Someone").id, "y1");
  assert.equal(ytArtist(list.slice(0, 1), "Daft Punk"), null);
});

const { playedMatch, sourceLabel, sourceName, rereadWanted } = await import("../src/desktop-player/logic.ts");

test("playedMatch: the newest track of the history with the same title (trimmed, any case)", () => {
  const yt = item({ source: "ytmusic", id: "v1", title: "Instant Crush (feat. Julian Casablancas)", artUrl: "https://i.ytimg.com/a.jpg" });
  const older = item({ source: "spotify", id: "s1", title: "Instant Crush (feat. Julian Casablancas)" });
  const recents = [item({ kind: "album", title: "instant crush (feat. julian casablancas)" }), yt, older];
  assert.equal(playedMatch(" instant crush (feat. Julian Casablancas) ", recents), yt);
  assert.equal(playedMatch("Get Lucky", recents), null);
  assert.equal(playedMatch("", recents), null);
  assert.equal(playedMatch(null, recents), null);
});

test("sourceLabel: the matched song's source, else friendly names for known players", () => {
  const ytPlay = item({ source: "ytmusic", title: "Instant Crush" });
  assert.equal(sourceLabel("mpv", ytPlay, [ytPlay]), "YT Music");
  assert.equal(sourceLabel("mpv", item({ source: "spotify" }), []), "Spotify");
  assert.equal(sourceLabel("spotify", null, []), "Spotify");
  assert.equal(sourceLabel("spotifyd", null, []), "Spotify");
  assert.equal(sourceLabel("spotifyd.instance42", null, []), "Spotify");
  // mpv is the player's own only when its last play was on YT Music
  assert.equal(sourceLabel("mpv", null, [ytPlay]), "YT Music");
  assert.equal(sourceLabel("mpv", null, [item({ source: "spotify" }), ytPlay]), "mpv");
  assert.equal(sourceLabel("mpv", null, []), "mpv");
  assert.equal(sourceLabel("firefox.instance_1_23", null, [ytPlay]), "firefox.instance_1_23");
  assert.equal(sourceLabel("", null, []), "Coucou");
  assert.equal(sourceName("spotify"), "Spotify");
  assert.equal(sourceName("ytmusic"), "YT Music");
});

test("rereadWanted: once per song, only for an unmatched song with no cover of its own", () => {
  assert.equal(rereadWanted(null, false, true), true);
  assert.equal(rereadWanted(null, true, true), false, "already read again for this song");
  assert.equal(rereadWanted(item({}), false, true), false, "matched");
  assert.equal(rereadWanted(null, false, false), false, "a player with its own cover (Spotify's app)");
});

