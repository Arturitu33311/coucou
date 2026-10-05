// The Music pill: MPRIS events → State.music, plus what the island derives from
// the track — the album art's colour and the synchronised lyrics.
//
// Nothing here polls. A `music` event arrives when the track, the play state or
// the position (a seek) changes, and the views extrapolate the position from the
// timestamp in it, so the lyrics and the seek bar cost nothing while idle.

import { Bridge, onEvent } from "../core/bridge";
import { MUSIC_ID, State, type MusicInfo, type MusicTrack } from "../core/state";
import type { Island } from "./island";

export const MUSIC_RED = "#FA2D48";

let islandRef: Island | null = null;

export function registerMusicHandlers(island: Island) {
  islandRef = island;
  void onEvent<MusicTrack | null>("music", (track) => apply(track));
  void Bridge.musicState().then((track) => apply(track));
  void onEvent<{ bars: number[]; silent: boolean }>("music-bars", (p) => {
    live = { bars: p.bars, silent: p.silent, at: performance.now() };
  });
}

// ── Real-time bars (cava) ─────────────────────────────────────────────────────

/** The newest levels cava sent: 64 bands, 0–255, and whether it is silent. */
let live: { bars: number[]; silent: boolean; at: number } | null = null;
let barsOn = false;
/** True once cava turned out not to be installed: the animated bars take over. */
let cavaMissing = false;

/** Fresh real-time levels, or null (cava off, missing, or not heard from lately). */
export function liveBars(): { bars: number[]; silent: boolean } | null {
  if (cavaMissing || !live || performance.now() - live.at > 400) return null;
  return live;
}

/**
 * cava costs CPU and listens to the sound output, so it runs only while the bars
 * can be seen and a track is playing. Called whenever what is on screen changes.
 */
export function setBarsWanted(wanted: boolean) {
  if (wanted === barsOn) return;
  barsOn = wanted;
  if (!wanted) live = null;
  void Bridge.musicBars(wanted).then((running) => {
    if (wanted && running === false) cavaMissing = true;
  });
}

/** The minimised island carries the pill while a track is known: tell it. */
function strip() {
  islandRef?.refreshMusicStrip();
}

/**
 * Called when Settings changed. Switching the Music pill off while its view is
 * open must not leave the island on a view with no pill behind it; switching
 * "online" on with a song already playing fetches its lyrics and cover now.
 */
export function musicSettingsChanged(island: Island) {
  // Switching the pill off and on again creates a fresh task with the default
  // colour: give it the cover's colour back.
  if (State.music) setPillColor(State.music.accent);
  if (State.view === "music" && !State.tasks.some((t) => t.id === MUSIC_ID)) {
    island.setView(State.defaultView());
  }
  // The pill may have been switched on or off: the minimised island follows.
  island.refreshMusicStrip();
  const m = State.music;
  if (!m || !State.settings.musicOnline) return;
  if (m.lyricsState === "off") void loadLyrics(m.key);
  if (!m.art && m.artUrl) void loadArt(m.key, m.artUrl);
}

/**
 * The cover's colour as a background, the way the GNOME extension fills its pill.
 * A bright cover would make the white text unreadable, so the fill is darkened
 * until its luminance is at most 0.35; `soft` is the same colour for small
 * surfaces (the pill in the grid) where a full fill would shout.
 */
export function musicTint(m: MusicInfo | null): { bg: string; soft: string } | null {
  if (!m?.rgb) return null;
  const [r, g, b] = m.rgb;
  const luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  const k = luma > 0.35 ? 0.35 / luma : 1;
  const d = [r, g, b].map((x) => Math.round(x * k));
  const dd = d.map((x) => Math.round(x * 0.55));
  return {
    bg: `linear-gradient(135deg, rgb(${d.join(",")}) 0%, rgb(${dd.join(",")}) 100%)`,
    soft: `rgba(${d.join(",")},0.4)`,
  };
}

/** Where the track is right now, from the last report. */
export function musicPosition(m: MusicInfo, now = Date.now()): number {
  const base = m.status === "Playing" ? m.positionMs + Math.max(0, now - m.positionAt) : m.positionMs;
  return m.lengthMs > 0 ? Math.min(Math.max(base, 0), m.lengthMs) : Math.max(base, 0);
}

/** Index of the lyric line being sung at `posMs`, or -1 before the first one. */
export function lyricIndex(lines: { t: number }[], posMs: number): number {
  let lo = 0;
  let hi = lines.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].t <= posMs) {
      found = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found;
}

function keyOf(t: MusicTrack): string {
  return [t.player, t.title, t.artists.join(","), t.lengthMs].join("\u001f");
}

function setPillColor(color: string) {
  const task = State.tasks.find((t) => t.id === MUSIC_ID);
  if (task) task.color = color;
}

function apply(track: MusicTrack | null) {
  // A player with no title is idle, not "playing nothing": show the empty card.
  if (!track || !track.title) {
    // "Always ON", like the GNOME extension's setting: when the player closes the
    // pill keeps the last track (stopped) instead of vanishing. Before any track
    // there is nothing to keep, and the card says so.
    if (State.music && State.settings.musicKeep) {
      State.music = { ...State.music, status: "Stopped", positionAt: Date.now() };
    } else {
      State.music = null;
      setPillColor(MUSIC_RED);
    }
    State.notify();
    strip();
    return;
  }

  const previous = State.music;
  const key = keyOf(track);
  if (previous && previous.key === key) {
    const artChanged = previous.artUrl !== track.artUrl;
    State.music = { ...previous, ...track };
    if (artChanged) void loadArt(key, track.artUrl);
    // "Fetch online" may have been switched on since the track started.
    if (previous.lyricsState === "off" && State.settings.musicOnline) void loadLyrics(key);
    State.notify();
    strip();
    return;
  }

  State.music = {
    ...track,
    key,
    art: null,
    rgb: null,
    accent: MUSIC_RED,
    lyrics: null,
    lyricsState: State.settings.musicOnline ? "loading" : "off",
  };
  setPillColor(MUSIC_RED);
  void loadArt(key, track.artUrl);
  if (State.settings.musicOnline) void loadLyrics(key);
  State.notify();
  strip();
}

/** Only ever writes to the track it was started for: songs can change mid-fetch. */
function patch(key: string, change: Partial<MusicInfo>) {
  if (State.music?.key !== key) return;
  State.music = { ...State.music, ...change };
  State.notify();
}

async function loadArt(key: string, url: string) {
  if (!url) {
    patch(key, { art: null, rgb: null, accent: MUSIC_RED });
    setPillColor(MUSIC_RED);
    return;
  }
  try {
    const art = await Bridge.musicArt(url);
    const rgb = await averageColor(art);
    const accent = rgb ? readableAccent(rgb) : MUSIC_RED;
    patch(key, { art, rgb, accent });
    if (State.music?.key === key) setPillColor(accent);
  } catch {
    // No art (a web URL while online fetching is off, a missing file): the card
    // keeps its note icon and the default colour.
    patch(key, { art: null, rgb: null, accent: MUSIC_RED });
  }
}

async function loadLyrics(key: string) {
  const m = State.music;
  if (!m || m.key !== key || m.lengthMs <= 0) {
    patch(key, { lyricsState: "none" });
    return;
  }
  patch(key, { lyricsState: "loading" });
  try {
    const lines = await Bridge.musicLyrics(m.title, m.artists.join(", "), m.album, m.lengthMs);
    patch(key, lines && lines.length > 0 ? { lyrics: lines, lyricsState: "ready" } : { lyrics: null, lyricsState: "none" });
  } catch {
    // Online fetching is off, or lrclib could not be reached.
    patch(key, { lyrics: null, lyricsState: State.settings.musicOnline ? "none" : "off" });
  }
}

// ── Colour ────────────────────────────────────────────────────────────────────

/** Mean colour of the picture, like the GNOME extension's `getAverageColor`. */
async function averageColor(dataUrl: string): Promise<[number, number, number] | null> {
  const img = new Image();
  img.src = dataUrl;
  try {
    await img.decode();
  } catch {
    return null;
  }
  const size = 24;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0, size, size);
  const data = ctx.getImageData(0, 0, size, size).data;
  let r = 0, g = 0, b = 0, n = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 128) continue;
    r += data[i];
    g += data[i + 1];
    b += data[i + 2];
    n += 1;
  }
  return n === 0 ? null : [Math.round(r / n), Math.round(g / n), Math.round(b / n)];
}

/**
 * The average of a dark cover is itself dark — unreadable as text or as bars on
 * the island's near-black. Keep the hue, lift the lightness (and a little of the
 * saturation, unless the cover is grey on purpose).
 */
export function readableAccent([r, g, b]: [number, number, number]): string {
  const [h, s, l] = rgbToHsl(r, g, b);
  const grey = s < 0.08;
  const [rr, gg, bb] = hslToRgb(h, grey ? s : Math.max(s, 0.5), Math.max(l, grey ? 0.8 : 0.62));
  return `#${[rr, gg, bb].map((x) => x.toString(16).padStart(2, "0")).join("")}`;
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255, gn = g / 255, bn = b / 255;
  const max = Math.max(rn, gn, bn), min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = max === rn ? (gn - bn) / d + (gn < bn ? 6 : 0) : max === gn ? (bn - rn) / d + 2 : (rn - gn) / d + 4;
  h /= 6;
  return [h, s, l];
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  if (s === 0) {
    const v = Math.round(l * 255);
    return [v, v, v];
  }
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const f = (t: number) => {
    let x = t;
    if (x < 0) x += 1;
    if (x > 1) x -= 1;
    if (x < 1 / 6) return p + (q - p) * 6 * x;
    if (x < 1 / 2) return q;
    if (x < 2 / 3) return p + (q - p) * (2 / 3 - x) * 6;
    return p;
  };
  return [Math.round(f(h + 1 / 3) * 255), Math.round(f(h) * 255), Math.round(f(h - 1 / 3) * 255)];
}
