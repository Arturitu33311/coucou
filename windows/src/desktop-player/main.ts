// The desktop player's page (desktop.html), shown by the `desktop-player` window
// at desktop level on the primary monitor. This part draws what is not touched:
// the scene tinted by the current cover (the hour's colours with no music), the
// wallpaper's clock with the date, the battery/network gauge as a glass pill,
// the edge light in the cover's colour and the line being sung. The touchable
// panels go into the empty slots #dp-recents, #dp-flow, #dp-search and #dp-bar.
//
// This window has its own JS context: it follows the same events the island
// does (`music`, `sysstate`, `settings-changed`) and derives the cover, its
// colour and the lyrics itself, with the island's own functions and commands
// (Rust caches the lyrics and enforces "fetch online", so nothing new goes out).
//
// Nothing loops. The clock wakes once a minute and the lyric line once per line,
// and both stop while the page is hidden.

import "./desktop.css";
import { Bridge, onEvent } from "../core/bridge";
import type { LyricLine, MusicInfo, MusicTrack, Settings, SysInfo } from "../core/state";
import { averageColor, lyricIndex, musicPosition } from "../island/music";
import { language, onLanguageChange, resolveLanguage, setLanguage, systemLanguages } from "../i18n/i18n";
import { Clock, clockText, msToNextMinute } from "./clock";
import { buildGauge } from "./gauge";
import { css, hourOf, mix, sceneColors, type RGB, type SceneColors } from "./scene";
import { buildRecents, type RecentsView } from "./recents";
import { AlbumSource, buildFlow, fetchArtistAlbums, flowArtist, type FlowView } from "./flow";
import { buildSearch, QueryGate, type SearchView } from "./search";
import { buildPlayerBar, type PlayerBarView } from "./bar";
import { Feedback } from "./feedback";
import { lyricShown, playedMatch, playerSource, progressTimerWanted, rereadWanted, SongLists, sourceLabel } from "./logic";
import type { PlayerItem } from "../core/player";
import { has3d } from "./tilt";

/** The mockup's stage; everything is laid out on it and it is scaled to the screen. */
const STAGE_W = 1920;
const STAGE_H = 1080;
/** How tall the clock's digits are on the stage (the mockup's slot, above "Continue listening"). */
const CLOCK_GLYPH_H = 230;
const BLACK: RGB = [0, 0, 0];

// ── DOM ───────────────────────────────────────────────────────────────────────

function div(cls: string, id = ""): HTMLDivElement {
  const e = document.createElement("div");
  if (cls) e.className = cls;
  if (id) e.id = id;
  return e;
}

const root = document.getElementById("dp")!;
const scene = div("dp-scene");
const art = document.createElement("img");
art.className = "dp-art";
art.alt = "";
art.decoding = "async";
const wash = div("dp-wash");
const floor = div("dp-floor");
const stage = div("dp-stage");
const edge = div("dp-edge");
for (const e of [scene, art, wash, floor, edge]) e.setAttribute("aria-hidden", "true");

const halo = div("dp-halo");
halo.setAttribute("aria-hidden", "true");
const time = div("dp-time");
const date = document.createElement("span");
date.className = "dp-date";
const clockCanvas = document.createElement("canvas");
clockCanvas.className = "dp-clock";
clockCanvas.setAttribute("role", "img");
time.append(date, clockCanvas);
const gauge = buildGauge();
const lyric = div("dp-lyric");
lyric.setAttribute("aria-live", "off");

// the slots the next panels fill, at the mockup's places
const slots = ["dp-recents", "dp-flow", "dp-search", "dp-bar"].map((id) => div("", id));

// The panels are built in main(), once the interface language is set.
let recentsView: RecentsView | null = null;
let flowView: FlowView | null = null;
let searchView: SearchView | null = null;
let barView: PlayerBarView | null = null;
const albumSource = new AlbumSource(fetchArtistAlbums);
/** Whether the line being sung is shown (the bar's lyrics button). */
let lyricsOn = true;

function buildPanels() {
  const feedback = new Feedback();
  // Without compositing (the launcher's WEBKIT_DISABLE_COMPOSITING_MODE=1) WebKitGTK
  // flattens rotateY: the cards then take the 2D likeness (tilt.ts).
  const flat = !has3d(window);
  root.classList.toggle("flat3d", flat);
  void Bridge.log(`desktop player: cards turned ${flat ? "in 2D (no 3D compositing)" : "in 3D"}`);
  recentsView = buildRecents(slots[0], feedback, flat);
  // the front cover's album lists its songs in the results
  flowView = buildFlow(slots[1], albumSource, (album) => searchView?.showAlbum(album), flat);
  searchView = buildSearch(slots[2], feedback);
  barView = buildPlayerBar(slots[3], feedback, () => {
    lyricsOn = !lyricsOn;
    barView?.setLyrics(lyricsOn);
    paintLyric();
  });
  barView.setLyrics(lyricsOn);
}

function relabelPanels() {
  recentsView?.relabel();
  flowView?.relabel();
  searchView?.relabel();
  barView?.relabel();
}

stage.append(halo, time, gauge.root, ...slots, lyric);
root.append(scene, art, wash, floor, stage, edge);

const clock = new Clock(clockCanvas, CLOCK_GLYPH_H);
let stageScale = 1;

function layout() {
  const w = window.innerWidth;
  const h = window.innerHeight;
  stageScale = Math.min(w / STAGE_W, h / STAGE_H) || 1;
  const left = (w - STAGE_W * stageScale) / 2;
  const top = (h - STAGE_H * stageScale) / 2;
  stage.style.transform = `translate(${left}px, ${top}px) scale(${stageScale})`;
}

// ── Music: the same derivation as the island (src/island/music.ts) ────────────

let track: MusicTrack | null = null;
let trackKey = "";
let artUrl = "";
/**
 * The cover being shown or fetched: the player's own (MPRIS `artUrl`), else the
 * one of the song the glass player started (`played`), which mpv does not report.
 */
let artSrc = "";
let rgb: RGB | null = null;
let lyrics: LyricLine[] | null = null;
let musicOnline = false;
/** The cover on screen, as a data: URL; the bar and the "now" card show it too. */
let artData: string | null = null;
/** What was played lately (newest first), as last read. */
let recents: PlayerItem[] = [];
/** The history's entry for the song playing, when the glass player started it. */
let played: PlayerItem | null = null;
/** The song the history was read a second time for (once per song). */
let rereadKey = "";
/** mpv writes the history once the song has started: read it again this much later. */
const REREAD_MS = 4000;

function keyOf(t: MusicTrack): string {
  return [t.player, t.title, t.artists.join(","), t.lengthMs].join("\u001f");
}

const songLists = new SongLists();
const listsGate = new QueryGate();

function updatePanels() {
  const source = track ? sourceLabel(track.player, played, recents) : "";
  barView?.setTrack(track, artData, source);
  recentsView?.setTrack(track, artData, source);
  syncProgress();
  // the play state, a seek or the cover arriving: the lists stay as they are
  if (songLists.changed(trackKey)) void refreshLists();
}

/** Once per song: what was played lately, for the recents and the cover flow's artist. */
async function refreshLists() {
  const seq = listsGate.next();
  const rec = (await Bridge.playerRecents()) ?? [];
  if (!listsGate.isCurrent(seq)) return;
  recents = rec;
  recentsView?.setItems(rec);
  void flowView?.update(flowArtist(track?.artists[0] ?? null, rec));
  const t = track;
  if (!t) return;
  played = playedMatch(t.player, t.title, rec);
  if (!t.artUrl && played?.artUrl && artSrc !== played.artUrl) showWantedArt(trackKey);
  updatePanels();
  // only mpv's songs wait for their history entry (an mpv the player did not start reads once more, that is all)
  if (rereadWanted(played, rereadKey === trackKey, !t.artUrl && playerSource(t.player) === "ytmusic")) {
    rereadKey = trackKey;
    const key = trackKey;
    window.setTimeout(() => {
      if (trackKey === key) void refreshLists();
    }, REREAD_MS);
  }
}

/** Fetches the cover the song should show (its own, else the played item's). */
function showWantedArt(key: string) {
  const t = track;
  if (t?.artUrl) {
    artSrc = t.artUrl;
    void loadArt(key, artSrc, Bridge.musicArt);
  } else if (played?.artUrl) {
    artSrc = played.artUrl;
    void loadArt(key, artSrc, Bridge.playerArt);
  } else {
    artSrc = "";
    void loadArt(key, "", Bridge.musicArt);
  }
}

function applyTrack(t: MusicTrack | null) {
  // a player with no title is idle: the hour's scene again
  if (!t || !t.title) {
    track = null;
    trackKey = "";
    artUrl = "";
    artSrc = "";
    played = null;
    rgb = null;
    lyrics = null;
    showArt(null);
    paintScene();
    paintLyric();
    updatePanels();
    return;
  }
  const key = keyOf(t);
  const newSong = key !== trackKey;
  track = t;
  if (newSong) {
    trackKey = key;
    lyrics = null;
    // from the last read; the next one (once per song) may find it
    played = playedMatch(t.player, t.title, recents);
    void loadLyrics(key);
  }
  if (newSong || t.artUrl !== artUrl) {
    artUrl = t.artUrl;
    // the old cover stays until the new one is there
    showWantedArt(key);
  }
  paintLyric();
  updatePanels();
}

/** Only ever lands on the song and the cover it was started for. */
async function loadArt(key: string, url: string, fetchArt: (url: string) => Promise<string>, retried = false) {
  const current = () => trackKey === key && artSrc === url;
  if (!url) {
    rgb = null;
    showArt(null);
    paintScene();
    updatePanels();
    return;
  }
  try {
    const data = await fetchArt(url);
    if (!current()) return;
    const avg = await averageColor(data);
    if (!current()) return;
    rgb = avg;
    showArt(data);
    paintScene();
    updatePanels();
  } catch {
    if (!current()) return;
    // a cover that has only just been written may not be readable yet
    if (!retried) {
      window.setTimeout(() => void loadArt(key, url, fetchArt, true), 500);
      return;
    }
    rgb = null;
    showArt(null);
    paintScene();
    updatePanels();
  }
}

async function loadLyrics(key: string) {
  const t = track;
  if (!t || keyOf(t) !== key || t.lengthMs <= 0 || !musicOnline) return;
  try {
    const lines = await Bridge.musicLyrics(t.title, t.artists.join(", "), t.album, t.lengthMs);
    if (trackKey !== key) return;
    lyrics = lines && lines.length > 0 ? lines : null;
  } catch {
    // "fetch online" is off, or lrclib could not be reached: no line
    if (trackKey !== key) return;
    lyrics = null;
  }
  paintLyric();
}

function showArt(data: string | null) {
  artData = data;
  root.classList.toggle("has-art", !!data);
  if (data) art.src = data;
  else art.removeAttribute("src");
}

// ── Scene, edge light and clock ───────────────────────────────────────────────

let colors: SceneColors = sceneColors(null, hourOf(new Date()));

function paintScene() {
  const now = new Date();
  colors = sceneColors(rgb, hourOf(now));
  const s = root.style;
  s.setProperty("--accent", colors.accent);
  s.setProperty("--accent-glow", colors.glow);
  s.setProperty("--accent-soft", colors.soft);
  // the mockup's sky: dark at the top, a lit band two thirds down, dark at the foot
  s.setProperty("--scene-0", css(mix(colors.a, BLACK, 0.84)));
  s.setProperty("--scene-1", css(mix(colors.a, BLACK, 0.66)));
  s.setProperty("--scene-2", css(mix(colors.b, BLACK, 0.38)));
  s.setProperty("--scene-3", css(mix(colors.b, BLACK, 0.12)));
  s.setProperty("--scene-4", css(mix(colors.b, BLACK, 0.86)));
  wash.style.background = colors.wash ?? "transparent";
  paintClock(now);
}

function paintClock(now = new Date()) {
  const text = clockText(now);
  clock.draw(text, colors.clock, (window.devicePixelRatio || 1) * stageScale);
  clockCanvas.setAttribute("aria-label", text);
  date.textContent = dateText(now);
}

function dateText(d: Date): string {
  try {
    return new Intl.DateTimeFormat(language(), { weekday: "long", day: "numeric", month: "long" }).format(d);
  } catch {
    return d.toDateString();
  }
}

let minuteTimer: number | null = null;

/** Once a minute: the clock, the date and (with no music) the hour's colours. */
function armMinute() {
  if (minuteTimer != null) window.clearTimeout(minuteTimer);
  minuteTimer = null;
  if (document.hidden) return;
  minuteTimer = window.setTimeout(() => {
    minuteTimer = null;
    paintScene();
    armMinute();
  }, msToNextMinute(new Date()) + 50);
}

// ── The line being sung ───────────────────────────────────────────────────────

let lyricTimer: number | null = null;
let shownLine = "";

/** Shows the current line and wakes for the next one, while playing and seen. */
function paintLyric() {
  if (lyricTimer != null) window.clearTimeout(lyricTimer);
  lyricTimer = null;
  const t = track;
  const lines = lyrics;
  let text = "";
  if (t && lines) {
    const pos = musicPosition(t as MusicInfo);
    const idx = lyricIndex(lines, pos);
    text = idx >= 0 ? lines[idx].text.trim() : "";
    const next = lines[idx + 1];
    if (t.status === "Playing" && !document.hidden && next) {
      lyricTimer = window.setTimeout(() => {
        lyricTimer = null;
        paintLyric();
      }, Math.max(30, next.t - pos + 30));
    }
  }
  if (text !== shownLine) {
    shownLine = text;
    if (text) lyric.textContent = text;
  }
  lyric.classList.toggle("on", lyricShown(text, lyricsOn));
}

// ── Gauge ─────────────────────────────────────────────────────────────────────

let sys: SysInfo | null = null;

function applySys(s: SysInfo | null) {
  sys = s;
  gauge.set(sys);
}

// ── Life cycle ────────────────────────────────────────────────────────────────

let progressTimer: number | null = null;

/** The progress bar moves twice a second while a song plays and the page is seen; else no timer. */
function syncProgress() {
  const want = progressTimerWanted(track?.status, document.hidden);
  if (want && progressTimer == null) {
    progressTimer = window.setInterval(() => barView?.tick(), 500);
  } else if (!want && progressTimer != null) {
    window.clearInterval(progressTimer);
    progressTimer = null;
  }
}

function wake() {
  syncProgress();
  if (document.hidden) {
    if (minuteTimer != null) window.clearTimeout(minuteTimer);
    if (lyricTimer != null) window.clearTimeout(lyricTimer);
    minuteTimer = null;
    lyricTimer = null;
    return;
  }
  paintScene();
  paintLyric();
  armMinute();
  barView?.tick();
}

async function main() {
  layout();
  paintScene();
  armMinute();
  wake();
  window.addEventListener("resize", () => {
    layout();
    paintClock();
  });
  document.addEventListener("visibilitychange", wake);
  onLanguageChange(() => {
    gauge.set(sys);
    paintClock();
    relabelPanels();
  });
  // the clock's face is a system font: draw again once fonts are settled
  void document.fonts?.ready.then(() => {
    clock.invalidate();
    paintClock();
  });

  void onEvent("desktop-player-search", () => {
    searchView?.focusSearch();
  });

  const boot = await Bridge.boot();
  setLanguage(resolveLanguage(boot?.settings.language, systemLanguages()));
  musicOnline = !!boot?.settings.musicOnline;
  // in the user's language from the start (the labels are relabelled on each change)
  buildPanels();
  updatePanels();

  void onEvent<Settings>("settings-changed", (s) => {
    setLanguage(resolveLanguage(s.language, systemLanguages()));
    const wasOnline = musicOnline;
    musicOnline = !!s.musicOnline;
    if (!wasOnline && musicOnline && track && !lyrics) void loadLyrics(trackKey);
    if (wasOnline && !musicOnline) {
      lyrics = null;
      paintLyric();
    }
  });
  void onEvent<MusicTrack | null>("music", applyTrack);
  void Bridge.musicState().then(applyTrack);
  void onEvent<SysInfo>("sysstate", applySys);
  void Bridge.sysState().then((s) => s && applySys(s));
}

void main();
