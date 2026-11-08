// The glass player ("player" view): what plays on top — the cover, the title, the
// controls and live bars, all on the cover's colour — then a search across
// Spotify and YT Music and its results, moved through with the arrow keys.
//
// Built once and then only updated, like the Music view: the buttons are never
// replaced under the pointer, and nothing runs while the island is shut (the
// search waits for typing, the idle check is one timer that stops when the view
// is no longer on screen, the bars are drawn from the island's frame loop).
//
// Recents, the DJ and the artist page are filled in by the next stage: the
// "nothing typed yet" content goes in `idleSlot`, an artist opens `artistPane`.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import type { ViewActions, ViewHost } from "./views";
import { Bridge } from "../core/bridge";
import { State, type Settings } from "../core/state";
import type { PlayerItem, PlayerSource, PlayerSourceName, SourceError } from "../core/player";
import { liveBars, musicTint, MUSIC_RED } from "../island/music";
import { t, tl } from "../i18n/i18n";

/** How long typing must pause before a search goes out (Spotify's quota is small). */
export const DEBOUNCE_MS = 250;
/** Without use, and with the pointer elsewhere, the player folds the island after this. */
export const PLAYER_IDLE_MS = 30_000;
/** Spotify gives at most 10 results per call: the next page starts 10 further on. */
const PAGE = 10;

export interface PlayerHost extends ViewHost {
  tick(nowMs: number): void;
  /** Opens an artist's page (the next stage fills it with songs and albums). */
  openArtist(item: PlayerItem): void;
  /** Esc: back from an artist, else back to Music when opened from it, else fold the island. */
  escape(): void;
}

// ── Pure pieces (tests/player.test.mjs) ───────────────────────────────────────

/** The row the arrows land on: wraps at both ends; -1 when there is nothing to land on. */
export function nextIndex(current: number, len: number, key: "ArrowUp" | "ArrowDown"): number {
  if (len <= 0) return -1;
  if (key === "ArrowDown") return current < 0 || current >= len - 1 ? 0 : current + 1;
  return current <= 0 || current > len - 1 ? len - 1 : current - 1;
}

/** Numbers every query: only the answer to the newest one is shown. */
export class QueryGate {
  private seq = 0;
  next(): number {
    return ++this.seq;
  }
  isCurrent(seq: number): boolean {
    return seq === this.seq;
  }
}

const isWindows = (userAgent: string) => userAgent.includes("Windows");

/** What the selector offers: YT Music (mpv + yt-dlp) is Linux only, so Windows has Spotify alone. */
export function sourceOptions(userAgent: string): PlayerSource[] {
  return isWindows(userAgent) ? ["spotify"] : ["spotify", "ytmusic", "both"];
}

/** What a search really asks for, from the remembered choice. */
export function effectiveSource(choice: Settings["playerSource"], userAgent: string): PlayerSource {
  return isWindows(userAgent) ? "spotify" : choice;
}

// ── Small pieces ──────────────────────────────────────────────────────────────

const MAGNIFIER = "M10.5 4a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13zM15.4 15.4L20 20";
const PERSON = "M12 12.2a4 4 0 1 0 0-8a4 4 0 1 0 0 8zM4.8 20c.8-3.4 3.7-5.6 7.2-5.6s6.4 2.2 7.2 5.6";
const SOURCE_COLOR: Record<PlayerSourceName, string> = { spotify: "#1ED760", ytmusic: "#FF3344" };
const SOURCE_NAME: Record<PlayerSourceName, string> = { spotify: "Spotify", ytmusic: "YT Music" };

function control(path: string, size: number, label: string, onClick: () => void): HTMLButtonElement {
  const btn = h("button", { class: "m-ctl", title: label, "aria-label": label }, svg(path, size));
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return btn;
}

/** The covers come as web URLs, which the page may not load itself (CSP): Rust fetches them. */
const thumbs = new Map<string, Promise<string | null>>();
function thumb(url: string): Promise<string | null> {
  let p = thumbs.get(url);
  if (!p) {
    p = Bridge.musicArt(url).then((data) => data || null, () => null);
    thumbs.set(url, p);
    // A long session of searching must not keep every cover it ever saw.
    if (thumbs.size > 150) thumbs.delete(thumbs.keys().next().value as string);
  }
  return p;
}

function setImage(box: HTMLElement, src: string | null, fallback: string, size: number) {
  clear(box);
  box.append(src ? h("img", { src, alt: "", draggable: false }) : svg(fallback, size));
}

/**
 * The few bars of the now-playing row: cava's levels while it runs (the island
 * asks for them while this view is seen), a gentle wave otherwise, flat when paused.
 */
class MiniBars {
  private ctx: CanvasRenderingContext2D | null;
  private last = 0;
  private heights: number[];
  private readonly canvas: HTMLCanvasElement;
  private readonly bars: number;
  constructor(canvas: HTMLCanvasElement, bars: number) {
    this.canvas = canvas;
    this.bars = bars;
    this.ctx = typeof canvas.getContext === "function" ? canvas.getContext("2d") : null;
    this.heights = new Array(bars).fill(1);
  }
  draw(now: number, playing: boolean, mode: Settings["musicVisualizer"]) {
    const ctx = this.ctx;
    if (!ctx || now - this.last < 16) return;
    this.last = now;
    const dpr = window.devicePixelRatio || 1;
    const w = this.canvas.clientWidth;
    const hgt = this.canvas.clientHeight;
    if (w === 0 || hgt === 0) return;
    if (this.canvas.width !== Math.round(w * dpr)) this.canvas.width = Math.round(w * dpr);
    if (this.canvas.height !== Math.round(hgt * dpr)) this.canvas.height = Math.round(hgt * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, hgt);
    if (mode === "off") return;
    const live = mode === "realtime" && playing ? liveBars() : null;
    const pitch = w / this.bars;
    const barW = Math.max(2, Math.round(pitch * 0.5));
    for (let i = 0; i < this.bars; i++) {
      let level = 0;
      if (live) {
        // cava's 64 bands, lows to highs, folded into this many bars.
        const from = Math.floor((i * live.bars.length) / this.bars);
        const to = Math.max(from + 1, Math.floor(((i + 1) * live.bars.length) / this.bars));
        let sum = 0;
        for (let j = from; j < to; j++) sum += live.bars[j] ?? 0;
        level = live.silent ? 0 : sum / (to - from) / 255;
      } else if (playing) {
        level = 0.25 + 0.5 * (0.5 + 0.5 * Math.sin(now / 240 + i * 0.7)) * (0.6 + 0.4 * Math.sin(now / 820 + i));
      }
      const target = Math.max(1.5, Math.pow(level, 0.8) * hgt);
      const prev = this.heights[i];
      const next = prev + (target - prev) * (target > prev ? 0.6 : 0.25);
      this.heights[i] = next;
      ctx.globalAlpha = playing ? 0.95 : 0.35;
      ctx.fillStyle = "#fff";
      const x = Math.round(i * pitch + (pitch - barW) / 2);
      const radius = Math.min(barW / 2, next / 2);
      ctx.beginPath();
      if (typeof ctx.roundRect === "function") ctx.roundRect(x, hgt - next, barW, next, radius);
      else ctx.rect(x, hgt - next, barW, next);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }
}

// ── The view ──────────────────────────────────────────────────────────────────

export function buildPlayerView(actions: ViewActions, env: { userAgent?: string } = {}): PlayerHost {
  const userAgent = env.userAgent ?? (typeof navigator !== "undefined" ? navigator.userAgent : "");
  const options = sourceOptions(userAgent);

  // Now playing
  const cover = h("div", { class: "pl-cover" }, svg(ICONS.note, 16));
  const title = h("div", { class: "pl-title" });
  const artist = h("div", { class: "pl-artist" });
  const prev = control(ICONS.backward, 11, t("Previous"), () => void Bridge.musicControl("previous"));
  const play = control(ICONS.play, 12, t("Play"), () => void Bridge.musicControl("playpause"));
  const next = control(ICONS.forward, 11, t("Next"), () => void Bridge.musicControl("next"));
  play.classList.add("main");
  const barsCanvas = h("canvas", { class: "pl-bars" });
  const bars = new MiniBars(barsCanvas, 9);
  const ctls = h("div", { class: "pl-ctls" }, prev, play, next);
  const now = h("div", { class: "pl-now" }, cover, h("div", { class: "pl-now-text" }, title, artist), barsCanvas, ctls);

  // Search
  const input = h("input", {
    class: "pl-input",
    type: "text",
    spellcheck: "false",
    autocomplete: "off",
    placeholder: tl("Search songs and artists"),
  });
  input.value = "";
  const sourceButtons = new Map<PlayerSource, HTMLButtonElement>();
  const seg = h("div", { class: "pl-seg" });
  for (const opt of options) {
    const label = opt === "both" ? t("Both") : SOURCE_NAME[opt];
    const b = h(
      "button",
      { class: "pl-src", "data-src": opt, title: label },
      opt === "both" ? null : h("i", { class: "pl-src-dot", style: `background:${SOURCE_COLOR[opt]}` }),
      h("span", { text: label }),
    );
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      chooseSource(opt);
    });
    b.addEventListener("mousedown", (e) => e.preventDefault()); // the field keeps the keyboard
    sourceButtons.set(opt, b);
    seg.append(b);
  }
  if (options.length === 1) seg.classList.add("single");
  const search = h("label", { class: "pl-search" }, svg(MAGNIFIER, 14, { stroke: 2 }), input, seg);

  // Body: the idle slot (nothing typed), the results, or an artist.
  const idleSlot = h("div", { class: "pl-idle", "data-hint": tl("Type a song or an artist") });
  const list = h("div", { class: "pl-list", role: "listbox" });
  const artistPane = h("div", { class: "pl-artist-pane" });
  const body = h("div", { class: "pl-body" }, idleSlot, list, artistPane);

  // The quiet line at the bottom: a source that failed, the Spotify app used, a song that did not start.
  const foot = h("div", { class: "pl-foot" });

  const card = h("div", { class: "card pl-card" }, now, search, body, foot);
  const root = h("div", { class: "view pl" }, card);

  // ── State ──
  const gate = new QueryGate();
  let query = "";
  let items: PlayerItem[] = [];
  let rows: HTMLElement[] = [];
  let errors: SourceError[] = [];
  let selected = -1;
  let offset = 0;
  let exhausted = false;
  let loading = false;
  let loadingMore = false;
  let failed = false;
  let notice: { text: string; tone: "" | "err"; until: number } | null = null;
  let artistOpen: PlayerItem | null = null;
  let debounce: number | null = null;

  const source = (): PlayerSource => effectiveSource(State.settings.playerSource ?? "both", userAgent);
  const visible = () => State.view === "player" && State.mode === "expanded";

  // ── Idle: fold after PLAYER_IDLE_MS without use and with the pointer elsewhere ──
  let lastUse = Date.now();
  let pointerIn = false;
  let idleTimer: number | null = null;
  let shown = false;

  function checkIdle() {
    idleTimer = null;
    if (!visible()) {
      shown = false; // nothing to watch until the view is opened again
      return;
    }
    const left = PLAYER_IDLE_MS - (Date.now() - lastUse);
    if (pointerIn || left > 0) {
      idleTimer = window.setTimeout(checkIdle, pointerIn ? PLAYER_IDLE_MS : left);
      return;
    }
    shown = false;
    actions.collapse();
  }
  function used() {
    lastUse = Date.now();
    if (idleTimer == null) idleTimer = window.setTimeout(checkIdle, PLAYER_IDLE_MS);
  }

  root.addEventListener("mouseenter", () => {
    pointerIn = true;
    used();
  });
  root.addEventListener("mouseleave", () => {
    pointerIn = false;
    used();
  });
  for (const type of ["mousedown", "wheel", "keydown"]) root.addEventListener(type, () => used());

  // ── Rendering ──
  function paintBody() {
    const typed = query !== "";
    idleSlot.style.display = !typed && !artistOpen ? "" : "none";
    list.style.display = typed && !artistOpen ? "" : "none";
    artistPane.style.display = artistOpen ? "" : "none";
  }

  function paintFoot() {
    clear(foot);
    const parts: Node[] = [];
    if (notice && notice.until > Date.now()) {
      parts.push(h("span", { class: notice.tone === "err" ? "pl-msg err" : "pl-msg", text: notice.text }));
    } else if (query) {
      for (const e of errors) {
        parts.push(h("span", { class: "pl-msg err", text: `${SOURCE_NAME[e.source] ?? e.source}: ${e.message}` }));
      }
      if (errors.some((e) => e.loginNeeded)) {
        const b = h("button", { class: "pl-login", text: t("Connect Spotify") });
        b.addEventListener("click", (e) => {
          e.stopPropagation();
          void connectSpotify();
        });
        parts.push(b);
      }
      if (parts.length === 0 && loadingMore) parts.push(h("span", { class: "pl-msg", text: t("Loading…") }));
    }
    foot.append(...parts);
    foot.classList.toggle("on", parts.length > 0);
  }

  function say(text: string, tone: "" | "err" = "") {
    notice = { text, tone, until: Date.now() + 4000 };
    paintFoot();
    window.setTimeout(() => paintFoot(), 4100);
  }

  function kindLabel(kind: PlayerItem["kind"]): string {
    return kind === "artist" ? t("Artist") : kind === "album" ? t("Album") : kind === "context" ? t("List") : "";
  }

  function rowFor(item: PlayerItem, index: number): HTMLElement {
    const art = h("div", { class: item.kind === "artist" ? "pl-thumb round" : "pl-thumb" });
    setImage(art, null, item.kind === "artist" ? PERSON : ICONS.note, 12);
    if (item.artUrl) {
      void thumb(item.artUrl).then((src) => {
        if (src) setImage(art, src, ICONS.note, 12);
      });
    }
    const kind = kindLabel(item.kind);
    const row = h(
      "div",
      { class: "pl-row", role: "option", "data-idx": index },
      art,
      h(
        "div",
        { class: "pl-row-text" },
        h("div", { class: "pl-row-title", text: item.title }),
        item.subtitle ? h("div", { class: "pl-row-sub", text: item.subtitle }) : null,
      ),
      kind ? h("span", { class: "pl-kind", text: kind }) : null,
      // With both sources mixed, a dot says which one a row comes from.
      source() === "both"
        ? h("i", { class: "pl-src-dot", title: SOURCE_NAME[item.source], style: `background:${SOURCE_COLOR[item.source]}` })
        : null,
    );
    row.addEventListener("mousemove", () => {
      if (selected !== index) select(index, false);
    });
    row.addEventListener("mousedown", (e) => e.preventDefault()); // keep the caret in the field
    row.addEventListener("click", (e) => {
      e.stopPropagation();
      void activate(item);
    });
    return row;
  }

  function paintList() {
    clear(list);
    rows = items.map((it, i) => rowFor(it, i));
    if (rows.length > 0) list.append(...rows);
    else if (query && !loading) list.append(h("div", { class: "pl-empty", text: failed ? t("Search isn't available right now") : t("No results") }));
    else if (query && loading) list.append(h("div", { class: "pl-empty", text: t("Searching…") }));
    list.scrollTop = 0;
    select(items.length > 0 ? 0 : -1, false);
  }

  function appendRows(fresh: PlayerItem[]) {
    const start = items.length;
    items.push(...fresh);
    const added = fresh.map((it, i) => rowFor(it, start + i));
    rows.push(...added);
    list.append(...added);
  }

  function select(index: number, scroll: boolean) {
    rows[selected]?.classList.remove("sel");
    selected = index;
    const row = rows[selected];
    if (!row) return;
    row.classList.add("sel");
    if (scroll) row.scrollIntoView({ block: "nearest" });
    // Arrowing onto the last row asks for the next page (not a fresh list's only row, nor a hover).
    if (scroll && selected === items.length - 1) void loadMore();
  }

  // ── Searching ──
  function scheduleSearch() {
    if (debounce != null) window.clearTimeout(debounce);
    debounce = window.setTimeout(() => {
      debounce = null;
      void runSearch();
    }, DEBOUNCE_MS);
  }

  async function runSearch() {
    query = input.value.trim();
    artistOpen = null;
    const seq = gate.next();
    items = [];
    errors = [];
    offset = 0;
    exhausted = false;
    failed = false;
    loadingMore = false;
    loading = query !== "";
    paintBody();
    paintList();
    paintFoot();
    if (!query) return;
    const res = await Bridge.playerSearch(source(), query, 0, seq);
    if (!gate.isCurrent(seq) || (res && res.seq !== seq)) return; // a newer query is on its way
    loading = false;
    failed = res === null;
    items = res?.items ?? [];
    errors = res?.errors ?? [];
    exhausted = items.length === 0;
    paintList();
    paintFoot();
  }

  async function loadMore() {
    if (!query || loading || loadingMore || exhausted) return;
    const seq = gate.next();
    const from = offset + PAGE;
    loadingMore = true;
    paintFoot();
    const res = await Bridge.playerSearch(source(), query, from, seq);
    if (!gate.isCurrent(seq)) return; // typed meanwhile: runSearch has reset everything
    loadingMore = false;
    if (!res) {
      paintFoot();
      return;
    }
    offset = from;
    const seen = new Set(items.map((it) => `${it.source}:${it.kind}:${it.id}`));
    const fresh = res.items.filter((it) => !seen.has(`${it.source}:${it.kind}:${it.id}`));
    if (fresh.length === 0) exhausted = true;
    else appendRows(fresh);
    paintFoot();
  }

  function chooseSource(opt: PlayerSource) {
    used();
    if (State.settings.playerSource === opt) return;
    State.settings.playerSource = opt as Settings["playerSource"];
    void Bridge.saveSettings(State.settings);
    paintSources();
    if (query) void runSearch();
    input.focus();
  }

  function paintSources() {
    const current = source();
    for (const [opt, b] of sourceButtons) b.classList.toggle("on", opt === current);
  }

  async function connectSpotify() {
    used();
    const err = await Bridge.playerSpotifyLogin();
    if (err) say(err, "err");
    else if (query) void runSearch();
  }

  async function activate(item: PlayerItem) {
    used();
    if (item.kind === "artist") {
      host.openArtist(item);
      return;
    }
    const res = await Bridge.playerPlay(item);
    if (!res) say(t("Couldn't play this"), "err");
    else if (res.fallbackUsed) say(t("Played through the Spotify app"));
  }

  // ── Keys (Esc is the island's, which hands it to escape()) ──
  input.addEventListener("input", () => {
    used();
    scheduleSearch();
  });
  input.addEventListener("keydown", (e) => {
    used();
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (artistOpen || items.length === 0) return;
      e.preventDefault();
      select(nextIndex(selected, items.length, e.key), true);
    } else if (e.key === "Enter") {
      if (artistOpen) return;
      e.preventDefault();
      // Typed faster than the debounce: search now rather than play an old result.
      if (debounce != null || input.value.trim() !== query) {
        if (debounce != null) window.clearTimeout(debounce);
        debounce = null;
        void runSearch();
        return;
      }
      const item = items[selected] ?? items[0];
      if (item) void activate(item);
    }
    // Backspace and everything else edit the text, always.
  });
  list.addEventListener("scroll", () => {
    if (list.scrollTop + list.clientHeight >= list.scrollHeight - 24) void loadMore();
  });

  // ── Now playing ──
  function paintNow() {
    const m = State.music;
    const accent = m?.accent ?? MUSIC_RED;
    root.style.setProperty("--acc", accent);
    const tint = musicTint(m);
    card.style.background = tint ? tint.bg : "";
    root.classList.toggle("tinted", tint !== null);
    if (!m) {
      setImage(cover, null, ICONS.note, 16);
      shownArt = undefined;
      title.textContent = t("Nothing is playing");
      artist.textContent = "";
      ctls.classList.add("off");
      return;
    }
    ctls.classList.remove("off");
    if (m.art !== shownArt) {
      shownArt = m.art;
      setImage(cover, m.art, ICONS.note, 16);
    }
    if (title.textContent !== m.title) title.textContent = m.title;
    const who = m.artists.join(", ") || m.identity;
    if (artist.textContent !== who) artist.textContent = who;
    const playing = m.status === "Playing";
    const path = playing ? ICONS.pause : ICONS.play;
    const p = play.querySelector("path");
    if (p && p.getAttribute("d") !== path) p.setAttribute("d", path);
    const label = playing ? t("Pause") : t("Play");
    play.title = label;
    play.setAttribute("aria-label", label);
    prev.disabled = !m.canPrevious;
    next.disabled = !m.canNext;
  }
  let shownArt: string | null | undefined;

  const host: PlayerHost = {
    el: root,
    sync() {
      if (visible() && !shown) {
        shown = true;
        used();
      }
      paintNow();
      paintSources();
      paintBody();
    },
    focus() {
      used();
      input.focus();
      input.select();
    },
    tick(nowMs: number) {
      const m = State.music;
      bars.draw(nowMs, m?.status === "Playing" && !State.paused, State.settings.musicVisualizer);
    },
    openArtist(item: PlayerItem) {
      used();
      artistOpen = item;
      clear(artistPane);
      const face = h("div", { class: "pl-artist-face" });
      setImage(face, null, PERSON, 22);
      if (item.artUrl) {
        void thumb(item.artUrl).then((src) => {
          if (src && artistOpen === item) setImage(face, src, PERSON, 22);
        });
      }
      artistPane.append(
        h(
          "div",
          { class: "pl-artist-head" },
          face,
          h("div", {}, h("div", { class: "pl-artist-name", text: item.title }), h("div", { class: "pl-row-sub", text: t("Artist") })),
        ),
      );
      paintBody();
    },
    escape() {
      used();
      if (artistOpen) {
        artistOpen = null;
        paintBody();
        input.focus();
        return;
      }
      if (State.playerFrom === "music") actions.setView("music");
      else actions.collapse();
    },
  };
  paintSources();
  paintBody();
  return host;
}
