// The Music pill's two faces, built on one panel:
//   * the compact card in the overview's left card (≈200 px of room next to Mochi):
//     title, one lyric line, the three controls and a progress bar;
//   * the full "music" view: the art, the title, three lyric lines, the seek bar
//     with times, the controls, and the visualizer along the bottom.
//
// Each is built once and then only updated: the buttons are never replaced under
// the pointer (a click that lands on a rebuilt button is lost), and the per-frame
// work in `tick` — progress, lyric line, bars — touches the DOM only when a value
// actually changed. The island's frame loop stops while it is hidden, so none of
// this runs then.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import type { ViewActions, ViewHost } from "./views";
import { Bridge } from "../core/bridge";
import { State, type MusicInfo, type Settings } from "../core/state";
import { liveBars, lyricIndex, musicPosition, musicTint, MUSIC_RED } from "../island/music";

export interface MusicHost extends ViewHost {
  tick(nowMs: number): void;
}

const fmt = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

function rgba(hex: string, a: number): string {
  const v = parseInt(hex.replace("#", ""), 16);
  return `rgba(${(v >> 16) & 255},${(v >> 8) & 255},${v & 255},${a})`;
}

// ── Pieces ────────────────────────────────────────────────────────────────────

/**
 * A line of text that loops like the GNOME extension's pill when it doesn't fit:
 * the text twice with a gap, sliding one copy past at a steady 30 px per second,
 * holding still at each end. Measured like before (a view still fading in has no
 * width yet, and would make every line look too long).
 */
function marquee(cls: string): { el: HTMLElement; set(text: string): void } {
  const first = h("span", {});
  const second = h("span", { class: "mq-dup", text: "" });
  const track = h("div", { class: "mq-track" }, first, h("span", { class: "mq-gap" }), second);
  const el = h("div", { class: `mq ${cls}` }, track);
  let last = "";
  let measuredFor = "";

  function measure() {
    const width = el.clientWidth;
    if (width === 0) return; // not laid out yet: try again on the next sync
    const key = `${width}|${last}`;
    if (key === measuredFor) return;
    measuredFor = key;
    const overflow = first.scrollWidth - width;
    if (overflow > 2) {
      const distance = first.scrollWidth + 30; // one copy and the gap, like the extension
      el.style.setProperty("--dx", `-${distance}px`);
      el.style.setProperty("--dur", `${2 + distance / 30}s`); // 1 s holds + 30 px/s of travel
      el.classList.add("scroll");
    } else {
      el.classList.remove("scroll");
    }
  }

  // The width is only known once the view is laid out and fades in, which is after
  // the text was set: measure again whenever it changes.
  new ResizeObserver(() => measure()).observe(el);

  return {
    el,
    set(text) {
      if (text !== last) {
        last = text;
        first.textContent = text;
        second.textContent = text;
        el.classList.remove("scroll");
        measuredFor = "";
      }
      measure();
    },
  };
}

function control(path: string, size: number, label: string, onClick: () => void): HTMLButtonElement {
  const btn = h("button", { class: "m-ctl", title: label, "aria-label": label }, svg(path, size));
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return btn;
}

function setIcon(btn: HTMLElement, path: string) {
  btn.querySelector("path")?.setAttribute("d", path);
}

/** The progress bar. A press seeks; it never rebuilds. */
function seekBar(onSeek: (fraction: number) => void) {
  const fill = h("i", {});
  const el = h("div", { class: "m-bar" }, fill);
  el.addEventListener("pointerdown", (e) => {
    e.stopPropagation();
    const r = el.getBoundingClientRect();
    if (r.width > 0) onSeek(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)));
  });
  return {
    el,
    set(fraction: number) {
      fill.style.width = `${Math.min(1, Math.max(0, fraction)) * 100}%`;
    },
    enable(on: boolean) {
      el.classList.toggle("locked", !on);
    },
  };
}

/**
 * The bars, drawn the way the GNOME extension draws them: symmetric about the
 * middle, the height of each eased toward its target (fast up, slower down), the
 * outer ones a little dimmer, all of them dim when it is silent. The levels come
 * from cava (real time) or, for Wave and Beat, from an animation — and also when
 * cava is not installed.
 */
class Visualizer {
  private ctx: CanvasRenderingContext2D | null;
  private last = 0;
  /** When the bars were last moved, to smooth by elapsed time rather than by frame. */
  private moved = 0;
  private heights: number[];
  private cleared = false;

  /** `fixed`: exact bar width and gap in CSS px (the little pill); otherwise the bars share the width. */
  constructor(
    readonly canvas: HTMLCanvasElement,
    private bars: number,
    private fixed?: { barWidth: number; gap: number },
  ) {
    this.ctx = canvas.getContext("2d");
    this.heights = new Array(bars).fill(1);
  }

  /** The 0–1 level of each of this visualizer's bars. */
  private levels(now: number, playing: boolean, mode: Settings["musicVisualizer"]): { norm: number[]; silent: boolean } {
    const live = mode === "realtime" && playing ? liveBars() : null;
    if (live) {
      // cava sends 64 bands; each bar is the mean of its share of them.
      const ratio = live.bars.length / this.bars;
      const norm = new Array<number>(this.bars).fill(0);
      for (let i = 0; i < this.bars; i++) {
        const from = Math.floor(i * ratio);
        const to = Math.max(from + 1, Math.floor((i + 1) * ratio));
        let sum = 0;
        let count = 0;
        for (let j = from; j < to && j < live.bars.length; j++) {
          sum += live.bars[j];
          count += 1;
        }
        norm[i] = count > 0 ? sum / count / 255 : 0;
      }
      return { norm, silent: live.silent };
    }
    // Animated: Wave, Beat, or the stand-in while cava is off or missing.
    const beat = mode === "beat";
    const norm = new Array<number>(this.bars).fill(0);
    if (playing) {
      for (let i = 0; i < this.bars; i++) {
        if (beat) {
          // A new height every ~140 ms per bar, hashed so neighbours differ.
          const bucket = Math.floor(now / 140) * 31 + i * 17;
          const hash = Math.sin(bucket * 12.9898) * 43758.5453;
          norm[i] = 0.15 + 0.85 * (hash - Math.floor(hash));
        } else {
          norm[i] = 0.3 + 0.55 * (0.5 + 0.5 * Math.sin(now / 260 + i * 0.55)) * (0.6 + 0.4 * Math.sin(now / 900 + i));
        }
      }
    }
    return { norm, silent: !playing };
  }

  draw(now: number, playing: boolean, mode: Settings["musicVisualizer"], color: string) {
    const ctx = this.ctx;
    if (!ctx) return;
    if (mode === "off") {
      if (!this.cleared) ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
      this.cleared = true;
      return;
    }
    if (now - this.last < 12) return;
    this.last = now;
    this.cleared = false;

    const dpr = window.devicePixelRatio || 1;
    const cssW = this.canvas.clientWidth;
    const cssH = this.canvas.clientHeight;
    if (cssW === 0 || cssH === 0) return;
    if (this.canvas.width !== Math.round(cssW * dpr) || this.canvas.height !== Math.round(cssH * dpr)) {
      this.canvas.width = Math.round(cssW * dpr);
      this.canvas.height = Math.round(cssH * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);
    ctx.fillStyle = color;

    const { norm, silent } = this.levels(now, playing, mode);
    // The extension moves its bars once per cava frame (60 a second): up by 95 % of the
    // way, down by 60 %. This draws fewer frames than that (software rendering), so the
    // same easing is applied for as many 60 Hz frames as have passed; otherwise the
    // bars fall at half the speed and seem stuck near the top.
    const steps = this.moved === 0 ? 1 : Math.min(6, Math.max(1, (now - this.moved) / (1000 / 60)));
    this.moved = now;
    const up = 1 - Math.pow(0.05, steps);
    const down = 1 - Math.pow(0.4, steps);
    const maxHalf = cssH / 2;
    const centre = Math.floor(cssH / 2);
    const slot = cssW / this.bars;
    const barW = this.fixed ? this.fixed.barWidth : Math.max(2, slot * 0.55);
    const pitch = this.fixed ? this.fixed.barWidth + this.fixed.gap : slot;
    // The little pill's few bars sit together in the middle of their canvas.
    const startX = this.fixed ? (cssW - (this.bars * pitch - this.fixed.gap)) / 2 : (slot - barW) / 2;

    for (let i = 0; i < this.bars; i++) {
      const level = norm[i];
      let target = Math.max(1, Math.round(Math.pow(level, 0.8) * maxHalf));
      if (!silent && level > 0 && target < 3) target = 3;
      const prev = this.heights[i];
      const alpha = target < prev ? down : up;
      const half = Math.round(prev * (1 - alpha) + target * alpha);
      this.heights[i] = half;

      const span = Math.max(1, (this.bars - 1) / 2);
      const edgeFade = 1 - (Math.abs(i - (this.bars - 1) / 2) / span) * 0.35;
      ctx.globalAlpha = (silent ? 0.3 : 1) * edgeFade;
      ctx.fillRect(startX + i * pitch, centre - Math.max(1, half), barW, Math.max(1, half) * 2);
    }
    ctx.globalAlpha = 1;
  }
}

function artBox(cls: string): { el: HTMLElement; set(url: string | null): void } {
  const note = svg(ICONS.note, cls === "big" ? 34 : cls === "strip" ? 14 : 16);
  const el = h("div", { class: `m-art ${cls}` }, note);
  let shown: string | null | undefined;
  return {
    el,
    set(url) {
      if (url === shown) return;
      shown = url;
      clear(el);
      if (url) {
        const img = h("img", { src: url, alt: "", draggable: false });
        el.append(img);
      } else {
        el.append(svg(ICONS.note, cls === "big" ? 34 : cls === "strip" ? 14 : 16));
      }
    },
  };
}

// ── The panel ─────────────────────────────────────────────────────────────────

function build(full: boolean, actions: ViewActions): { root: HTMLElement; card: HTMLElement; host: MusicHost } {
  const art = artBox(full ? "big" : "small");
  const title = marquee("m-title");
  const artist = marquee("m-artist");

  const prev = control(ICONS.backward, 12, "Previous", () => void Bridge.musicControl("previous"));
  const play = control(ICONS.pause, 13, "Play / pause", () => void Bridge.musicControl("playpause"));
  const next = control(ICONS.forward, 12, "Next", () => void Bridge.musicControl("next"));
  play.classList.add("main");

  const bar = seekBar((fraction) => {
    const m = State.music;
    if (!m || !m.canSeek || m.lengthMs <= 0) return;
    const ms = Math.round(fraction * m.lengthMs);
    // Move the bar at once; the player's Seeked signal confirms it.
    State.music = { ...m, positionMs: ms, positionAt: Date.now() };
    void Bridge.musicControl("seek", ms);
  });
  const time = h("span", { class: "m-time" });
  const row = h("div", { class: "m-row" }, prev, play, next, bar.el, time);

  const canvas = h("canvas", { class: "m-viz" });
  const viz = new Visualizer(canvas, full ? 56 : 22);

  // Lyrics: one line in the compact card, three in the full view.
  const lyricEls = full
    ? [h("div", { class: "m-ly dim" }), h("div", { class: "m-ly cur" }), h("div", { class: "m-ly dim" })]
    : [h("div", { class: "m-ly cur" })];
  const lyrics = h("div", { class: "m-lyrics" }, ...lyricEls);

  let root: HTMLElement;
  let card: HTMLElement;
  const open = () => actions.setView("music");

  if (full) {
    const main = h(
      "div",
      { class: "m-main" },
      title.el,
      artist.el,
      lyrics,
      row,
    );
    const body = h("div", { class: "m-full" }, art.el, main);
    card = h("div", { class: "card wash m-card full" }, body, canvas);
    root = h("div", { class: "view" }, card);
  } else {
    const head = h("div", { class: "m-head", onclick: open }, art.el, h("div", { class: "m-titles" }, title.el, artist.el));
    const body = h("div", { class: "int-card m-compact" }, head, lyrics, row);
    card = body;
    root = h("div", { class: "m-compact-wrap" }, body, canvas);
  }

  let lastIdx = -2;
  let lastLyricKey = "";
  let lastTime = "";

  function lyricText(m: MusicInfo, idx: number, offset: number): string {
    const lines = m.lyrics;
    if (!lines) return "";
    const i = idx + offset;
    return i >= 0 && i < lines.length ? lines[i].text : "";
  }

  function paintLyrics(m: MusicInfo, idx: number) {
    if (m.lyricsState === "ready" && m.lyrics) {
      if (full) {
        lyricEls[0].textContent = lyricText(m, idx, -1);
        lyricEls[1].textContent = idx < 0 ? "♪" : lyricText(m, idx, 0);
        lyricEls[2].textContent = lyricText(m, idx, 1);
      } else {
        lyricEls[0].textContent = idx < 0 ? "♪" : lyricText(m, idx, 0);
      }
      const cur = lyricEls[full ? 1 : 0];
      cur.classList.remove("fx");
      void cur.offsetWidth; // restart the fade
      cur.classList.add("fx");
      return;
    }
    // The hint that lyrics are off is for the full view; the small card would show
    // it on every song forever.
    const message =
      m.lyricsState === "loading" ? "Looking for lyrics…"
      : m.lyricsState === "off" && full ? "Lyrics are off — enable online fetching in Settings"
      : "";
    for (const el of lyricEls) el.textContent = "";
    lyricEls[full ? 1 : 0].textContent = message;
  }

  const host: MusicHost = {
    el: root,
    sync() {
      const m = State.music;
      const accent = m?.accent ?? MUSIC_RED;
      card.style.setProperty("--acc", accent);
      root.style.setProperty("--acc", accent);
      if (full) card.style.setProperty("--wash", rgba(accent, 0.38));

      // The cover's colour fills the card, like the GNOME extension's pill. In the
      // overview the card is the shared left card, so the fill goes on that one
      // (views.ts clears it again when another pill takes the card).
      const tint = musicTint(m);
      const surface = full ? card : (root.closest(".card") as HTMLElement | null);
      if (surface) surface.style.background = tint ? tint.bg : "";
      root.classList.toggle("tinted", tint !== null);

      if (!m) {
        art.set(null);
        title.set("Nothing playing");
        artist.set("Start a player to see it here");
        for (const el of lyricEls) el.textContent = "";
        row.classList.add("off");
        time.textContent = "";
        bar.set(0);
        lastLyricKey = "";
        lastIdx = -2;
        return;
      }
      row.classList.remove("off");
      art.set(m.art);
      title.set(m.title);
      artist.set(full && m.album ? `${m.artists.join(", ")} · ${m.album}` : m.artists.join(", ") || m.identity);
      setIcon(play, m.status === "Playing" ? ICONS.pause : ICONS.play);
      prev.disabled = !m.canPrevious;
      next.disabled = !m.canNext;
      bar.enable(m.canSeek && m.lengthMs > 0);

      // Repaint the lyrics when the song, their state, or the line changes.
      const key = `${m.key}|${m.lyricsState}|${m.lyrics?.length ?? 0}`;
      if (key !== lastLyricKey) {
        lastLyricKey = key;
        lastIdx = -2;
        host.tick(performance.now());
      }
    },
    tick(now: number) {
      const m = State.music;
      if (!m) return;
      const pos = musicPosition(m);
      bar.set(m.lengthMs > 0 ? pos / m.lengthMs : 0);
      const t = fmt(pos);
      if (t !== lastTime) {
        lastTime = t;
        time.textContent = full && m.lengthMs > 0 ? `${t} / ${fmt(m.lengthMs)}` : t;
      }
      const idx = m.lyricsState === "ready" && m.lyrics ? lyricIndex(m.lyrics, pos) : -1;
      if (idx !== lastIdx) {
        lastIdx = idx;
        paintLyrics(m, idx);
      }
      viz.draw(now, m.status === "Playing", State.settings.musicVisualizer, m.accent);
    },
  };
  return { root, card, host };
}

/** The overview's left card, while the Music pill is the one in focus. */
export function buildMusicCompact(actions: ViewActions): MusicHost {
  return build(false, actions).host;
}

/** The full view, opened from the compact card. */
export function buildMusicView(actions: ViewActions): MusicHost {
  return build(true, actions).host;
}

// ── The minimised island ──────────────────────────────────────────────────────

export interface StripActions {
  toggle(): void;
  next(): void;
  previous(): void;
  /** Bring the player's own window forward. */
  raise(): void;
  /** Open the island on the full Music view. */
  open(): void;
}

/**
 * The Music pill on the minimised island — the same pill as the GNOME extension's
 * (cover, scrolling "Title • Artist", a few bars, filled with the cover's colour)
 * plus the current lyric line, small, under the title. Its inputs follow that
 * extension's defaults: click plays/pauses, double-click or right-click opens the
 * full view, middle-click raises the player, the wheel changes track.
 */
export function buildMusicStrip(a: StripActions): MusicHost {
  const art = artBox("strip");
  const title = marquee("cm-title");
  const lyric = h("div", { class: "cm-lyric" });
  const canvas = h("canvas", { class: "cm-viz" });
  // Four bars, 2 px wide with 2 px between them: what the extension's pill has.
  const viz = new Visualizer(canvas, 4, { barWidth: 2, gap: 2 });
  const el = h("div", { id: "compact-music" }, art.el, h("div", { class: "cm-text" }, title.el, lyric), canvas);

  // The island opens on mousedown when minimised; this pill has its own actions.
  el.addEventListener("mousedown", (e) => {
    e.stopPropagation();
    if (e.button === 1) e.preventDefault(); // no middle-click autoscroll
  });

  // A click is play/pause unless a second one follows within the double-click
  // window, which opens the full view instead.
  let clickTimer: number | null = null;
  el.addEventListener("click", (e) => {
    e.stopPropagation();
    if (clickTimer != null) return;
    clickTimer = window.setTimeout(() => {
      clickTimer = null;
      a.toggle();
    }, 260);
  });
  el.addEventListener("dblclick", (e) => {
    e.stopPropagation();
    if (clickTimer != null) window.clearTimeout(clickTimer);
    clickTimer = null;
    if (!State.settings.openOnHover) a.open();
  });
  el.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (!State.settings.openOnHover) a.open();
  });
  el.addEventListener("auxclick", (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    e.stopPropagation();
    a.raise();
  });
  // The wheel, copied from the Dynamic Music Pill extension (scroll-action "track"):
  //   * wheel UP = next, DOWN = previous (and louder / quieter in volume mode);
  //   * a click of a mouse wheel acts at once — Clutter delivers those as discrete
  //     UP/DOWN events — while a touchpad's smooth deltas add up to one click's
  //     worth (threshold 1.0) before they count;
  //   * actions are at least 500 ms apart for tracks (50 ms for volume);
  //   * the whole pill slides 12 px in 100 ms and comes back in 250 ms with a bounce.
  // A page's wheel event is ~53 px for one click; anything smaller is a touchpad.
  const NOTCH_PX = 53;
  let scrollDelta = 0;
  let lastScrollAt = 0;
  let overlayText = "";
  let overlayUntil = 0;
  let overlayShown = false;
  let slideAnim: Animation | null = null;

  function slide(offset: number) {
    slideAnim?.cancel();
    const out = el.animate(
      [{ transform: "translateX(0px)" }, { transform: `translateX(${offset}px)` }],
      { duration: 100, easing: "cubic-bezier(0.25, 0.46, 0.45, 0.94)", fill: "forwards" },
    );
    slideAnim = out;
    out.finished
      .then(() => {
        slideAnim = el.animate(
          [{ transform: `translateX(${offset}px)` }, { transform: "translateX(0px)" }],
          { duration: 250, easing: "cubic-bezier(0.34, 1.56, 0.64, 1)" },
        );
        out.cancel();
      })
      .catch(() => {});
  }

  /** ±5 % of the player's own volume, with the read-out on the lyric line. */
  function stepVolume(louder: boolean) {
    const m = State.music;
    if (!m || m.volume < 0) return; // this player has no volume to change
    const v = Math.min(1, Math.max(0, Math.round((m.volume + (louder ? 0.05 : -0.05)) * 100) / 100));
    State.music = { ...m, volume: v };
    overlayText = `Volume ${Math.round(v * 100)}%`;
    overlayUntil = performance.now() + 1400;
    void Bridge.musicControl("volume", Math.round(v * 1000));
    State.notify();
  }

  el.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      e.stopPropagation();
      let up: boolean;
      if (Math.abs(e.deltaY) >= 40) {
        // A wheel click.
        up = e.deltaY < 0;
        scrollDelta = 0;
      } else {
        // A touchpad: smooth deltas, summed until they reach one click.
        scrollDelta += e.deltaY;
        if (scrollDelta < -NOTCH_PX) up = true;
        else if (scrollDelta > NOTCH_PX) up = false;
        else return;
        scrollDelta = 0;
      }
      const volume = State.settings.musicScroll === "volume";
      const now = performance.now();
      if (now - lastScrollAt < (volume ? 50 : 500)) return;
      lastScrollAt = now;
      slide(up ? 12 : -12);
      if (volume) stepVolume(up);
      else (up ? a.next : a.previous)();
    },
    { passive: false },
  );

  let lastIdx = -2;
  let lastKey = "";
  let info = ""; // "Title • Artist"

  // As in the extension, the lyric takes the pill's main line from the title while
  // one is being sung; "Title • Artist" then drops to the small line underneath.
  // With no lyric (none found, online off, or before the first line) the title is
  // the main line, centred on its own.
  function paintLine(m: MusicInfo, idx: number) {
    const line = m.lyricsState === "ready" && m.lyrics && idx >= 0 ? m.lyrics[idx].text : "";
    title.set(line || info);
    lyric.textContent = line ? info : "";
    el.classList.toggle("nolyric", line === "");
    title.el.classList.remove("fx");
    void title.el.offsetWidth; // restart the fade
    title.el.classList.add("fx");
  }

  const host: MusicHost = {
    el,
    sync() {
      const m = State.music;
      if (!m) return;
      const tint = musicTint(m);
      el.style.background = tint ? tint.bg : "";
      el.style.setProperty("--acc", m.accent);
      el.classList.toggle("tinted", tint !== null);
      el.classList.toggle("gone", m.status === "Stopped");
      art.set(m.art);
      // "Title • Artist" on one line, as the extension's inline-artist mode does.
      const who = m.artists.join(", ");
      info = who ? `${m.title} • ${who}` : m.title;
      const key = `${m.key}|${m.lyricsState}|${m.lyrics?.length ?? 0}|${info}`;
      if (key !== lastKey) {
        lastKey = key;
        lastIdx = -2;
        host.tick(performance.now());
      }
    },
    tick(now: number) {
      const m = State.music;
      if (!m) return;
      // The volume read-out takes the lyric line for a moment while the wheel is
      // turning, then the lyric comes back.
      if (performance.now() < overlayUntil) {
        if (!overlayShown || lyric.textContent !== overlayText) {
          overlayShown = true;
          lyric.textContent = overlayText;
          el.classList.remove("nolyric");
        }
      } else if (overlayShown) {
        overlayShown = false;
        lastIdx = -2; // repaint the lyric
      }
      if (!overlayShown) {
        const idx = m.lyricsState === "ready" && m.lyrics ? lyricIndex(m.lyrics, musicPosition(m)) : -1;
        if (idx !== lastIdx) {
          lastIdx = idx;
          paintLine(m, idx);
        }
      }
      viz.draw(now, m.status === "Playing", State.settings.musicVisualizer, m.accent);
    },
  };
  return host;
}
