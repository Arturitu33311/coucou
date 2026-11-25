// The glass player ("player" view): what plays on top — the cover, the title, the
// controls and live bars, all on the cover's colour — then a search across
// Spotify and YT Music and its results, moved through with the arrow keys.
//
// Built once and then only updated, like the Music view: the buttons are never
// replaced under the pointer, and nothing runs while the island is shut (the
// search waits for typing, the idle check is one timer that stops when the view
// is no longer on screen, the bars are drawn from the island's frame loop).
//
// With nothing typed: the DJ (Spotify) and what was played lately, read once
// each time the view comes on screen. An artist opens its page: the albums as a
// cover flow (views/coverflow.ts), the songs beside them.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import type { ViewActions, ViewHost } from "./views";
import { Bridge } from "../core/bridge";
import { State, type Settings } from "../core/state";
import type {
  ArtistPage, PlayerFailure, PlayerItem, PlayerOutcome, PlayerSource, PlayerSourceName, PlayResult, SourceError,
} from "../core/player";
import { buildCoverflow, type Coverflow } from "./coverflow";
import { liveBars, musicTint, MUSIC_RED } from "../island/music";
import { t, tl } from "../i18n/i18n";

/** How long typing must pause before a search goes out (Spotify's quota is small). */
export const DEBOUNCE_MS = 250;
/** Without use, and with the pointer elsewhere, the player folds the island after this. */
export const PLAYER_IDLE_MS = 30_000;
/** Spotify gives at most 10 results per call: the next page starts 10 further on. */
const PAGE = 10;
/** The view is entered through both sync() and focus(): the recents are read once for both. */
const RECENTS_FRESH_MS = 2000;

export interface PlayerHost extends ViewHost {
  tick(nowMs: number): void;
  /** Opens an artist's page: its albums in a cover flow, its songs beside them. */
  openArtist(item: PlayerItem): void;
  /** The island's pointer state changed (island.ts, next to `wasInIsland`). */
  pointer(inside: boolean): void;
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

/**
 * The window lost the keyboard (a click in another app): the open player folds, as a click
 * outside it should — unless pinned, a card waits, or the pointer is still on the island
 * (a click there that opened the browser or the Spotify app).
 */
export function foldsOnBlur(s: {
  mode: string;
  view: string;
  isPinned: boolean;
  pointerInIsland: boolean;
  pendingApproval: unknown;
}): boolean {
  return s.mode === "expanded" && s.view === "player" && !s.isPinned && !s.pointerInIsland && !s.pendingApproval;
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

export const SOURCE_NAME: Record<PlayerSourceName, string> = { spotify: "Spotify", ytmusic: "YT Music" };

/** A line the foot shows for a while: "Starting…", or why something failed. */
export interface PlayerNotice {
  text: string;
  tone: "" | "err";
  until: number;
}

/**
 * What the foot says (shared with the desktop player, src/desktop-player/feedback.ts):
 * the current notice, else each source's search error ("Spotify: …") or "Loading…",
 * and whether to offer "Connect Spotify".
 */
export function footContent(s: {
  notice: PlayerNotice | null;
  now: number;
  query: string;
  errors: SourceError[];
  loadingMore: boolean;
  loginNeeded: boolean;
  spotifyOff: boolean;
  source: PlayerSource;
}): { lines: { text: string; err: boolean }[]; offerLogin: boolean } {
  const lines: { text: string; err: boolean }[] = [];
  if (s.notice && s.notice.until > s.now) {
    lines.push({ text: s.notice.text, err: s.notice.tone === "err" });
  } else if (s.query) {
    for (const e of s.errors) lines.push({ text: `${SOURCE_NAME[e.source] ?? e.source}: ${e.message}`, err: true });
    if (lines.length === 0 && s.loadingMore) lines.push({ text: t("Loading…"), err: false });
  }
  // From any action that needed it, or up front when Spotify is chosen and has no session.
  const offerLogin =
    s.loginNeeded || (s.query !== "" && s.errors.some((e) => e.loginNeeded)) || (s.spotifyOff && s.source !== "ytmusic");
  return { lines, offerLogin };
}

/** What a play or the DJ came to: why it failed (and whether for want of a sign-in), or what to say if it worked. */
export function playReport(
  res: PlayerOutcome<PlayResult>,
  failText: string,
): { ok: false; text: string; loginNeeded: boolean } | { ok: true; text: string | null } {
  if (!res.ok) return { ok: false, text: res.error.message || failText, loginNeeded: res.error.loginNeeded };
  return { ok: true, text: res.value.fallbackUsed ? t("Played through the Spotify app") : null };
}

// ── Small pieces ──────────────────────────────────────────────────────────────

const MAGNIFIER = "M10.5 4a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13zM15.4 15.4L20 20";
const PERSON = "M12 12.2a4 4 0 1 0 0-8a4 4 0 1 0 0 8zM4.8 20c.8-3.4 3.7-5.6 7.2-5.6s6.4 2.2 7.2 5.6";
/** The DJ's mark: two sparks. */
const SPARK = "M11 2.5l1.9 5.6 5.6 1.9-5.6 1.9L11 17.5l-1.9-5.6L3.5 10l5.6-1.9L11 2.5zM18.5 14.5l.9 2.6 2.6.9-2.6.9-.9 2.6-.9-2.6-2.6-.9 2.6-.9.9-2.6z";
const SOURCE_COLOR: Record<PlayerSourceName, string> = { spotify: "#1ED760", ytmusic: "#FF3344" };

function control(path: string, size: number, label: string, onClick: () => void): HTMLButtonElement {
  const btn = h("button", { class: "m-ctl", title: label, "aria-label": label }, svg(path, size));
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return btn;
}

/**
 * The covers come as web URLs, which the page may not load itself (CSP): Rust fetches them
 * (only from the player's services' image servers). A cover that failed is asked again next time.
 */
const thumbs = new Map<string, Promise<string | null>>();
export function thumb(url: string): Promise<string | null> {
  let p = thumbs.get(url);
  if (!p) {
    const asked: Promise<string | null> = Bridge.playerArt(url).then(
      (data) => data || null,
      () => null,
    );
    p = asked.then((data) => {
      if (data === null && thumbs.get(url) === p) thumbs.delete(url);
      return data;
    });
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

/** Arrow keys over a small list of rows, kept apart from the search results' own. */
class RowNav {
  rows: HTMLElement[] = [];
  acts: (() => void)[] = [];
  sel = -1;
  set(rows: HTMLElement[], acts: (() => void)[]) {
    this.rows[this.sel]?.classList.remove("sel");
    this.rows = rows;
    this.acts = acts;
    this.sel = -1;
  }
  select(i: number, scroll: boolean) {
    this.rows[this.sel]?.classList.remove("sel");
    this.sel = i;
    const row = this.rows[i];
    if (!row) return;
    row.classList.add("sel");
    if (scroll) row.scrollIntoView({ block: "nearest" });
  }
  arrow(key: "ArrowUp" | "ArrowDown") {
    this.select(nextIndex(this.sel, this.rows.length, key), true);
  }
  /** Runs the selected row; false when none is. */
  run(): boolean {
    const act = this.acts[this.sel];
    if (!act) return false;
    act();
    return true;
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
  // Idle: the DJ (Spotify only), then the recents.
  const djRow = h(
    "div",
    { class: "pl-row pl-dj", role: "option" },
    h("div", { class: "pl-dj-mark" }, svg(SPARK, 15)),
    h(
      "div",
      { class: "pl-row-text" },
      h("div", { class: "pl-row-title" }, tl("DJ")),
      h("div", { class: "pl-row-sub" }, tl("Music picked for you by Spotify's DJ")),
    ),
    h("i", { class: "pl-src-dot", style: `background:${SOURCE_COLOR.spotify}` }),
  );
  djRow.addEventListener("mousedown", (e) => e.preventDefault());
  djRow.addEventListener("click", (e) => {
    e.stopPropagation();
    void playDj();
  });
  djRow.addEventListener("mousemove", () => {
    const i = idleNav.rows.indexOf(djRow);
    if (idleNav.sel !== i) idleNav.select(i, false);
  });
  const recentsList = h("div", { class: "pl-recents" });
  const idleSlot = h(
    "div",
    { class: "pl-idle" },
    djRow,
    h("div", { class: "pl-label" }, tl("Recent")),
    recentsList,
  );
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
  let notice: PlayerNotice | null = null;
  /** A play or the DJ is on its way: further presses wait for it. */
  let starting = false;
  /** Something the user did failed for want of a Spotify sign-in. */
  let loginNeeded = false;
  /** Spotify said, when the view came on screen, that it has no session. */
  let spotifyOff = false;
  let artistOpen: PlayerItem | null = null;
  let debounce: number | null = null;
  // Idle (nothing typed) and the artist page each keep their own selection.
  const idleNav = new RowNav();
  let recents: PlayerItem[] = [];
  let recentsAt = -Infinity;
  const recentsGate = new QueryGate();
  const artistGate = new QueryGate();
  const trackNav = new RowNav();
  let flow: Coverflow | null = null;
  let albums: PlayerItem[] = [];
  let artistTracks: PlayerItem[] = [];

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
      // Nothing to watch until the view is opened again, and the pointer it last saw is stale.
      shown = false;
      pointerIn = false;
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

  /** The view came on screen: the pointer is wherever the island last saw it. */
  function onShow() {
    shown = true;
    pointerIn = State.pointerInIsland;
    used();
    void loadRecents();
  }
  for (const type of ["mousedown", "wheel", "keydown"]) root.addEventListener(type, () => used());

  // ── Idle: the DJ and the recents ──
  /** Once per opening of the view (sync and focus both announce it), never on a timer. */
  async function loadRecents() {
    const at = Date.now();
    if (at - recentsAt < RECENTS_FRESH_MS) return;
    recentsAt = at;
    void checkSpotify();
    const seq = recentsGate.next();
    const res = await Bridge.playerRecents();
    if (!recentsGate.isCurrent(seq)) return;
    recents = res ?? [];
    paintIdle();
  }

  // Spotify's DJ can't be started through its public API (it is the mobile app's): hidden until it can.
  const djShown = () => false;

  function paintIdle() {
    djRow.style.display = djShown() ? "" : "none";
    clear(recentsList);
    const rows = recents.map((item) => {
      // The recents mix both sources whatever the selector says (Windows only has Spotify).
      const row = baseRow(item, options.length > 1);
      row.addEventListener("mousedown", (e) => e.preventDefault());
      row.addEventListener("click", (e) => {
        e.stopPropagation();
        void activate(item);
      });
      row.addEventListener("mousemove", () => {
        const i = idleNav.rows.indexOf(row);
        if (idleNav.sel !== i) idleNav.select(i, false);
      });
      return row;
    });
    if (rows.length > 0) recentsList.append(...rows);
    // Nothing played from here yet (or not read yet): say what the field is for.
    else recentsList.append(h("div", { class: "pl-empty" }, tl("Type a song or an artist")));
    const allRows = djShown() ? [djRow, ...rows] : rows;
    const acts = recents.map((item) => () => void activate(item));
    idleNav.set(allRows, djShown() ? [() => void playDj(), ...acts] : acts);
  }

  /** Whether Spotify has a session, so "Connect Spotify" can be offered before anything fails. */
  async function checkSpotify() {
    const on = await Bridge.playerSpotifyConnected();
    if (on === null) return; // not known (outside the app)
    spotifyOff = !on;
    paintFoot();
  }

  function playDj() {
    return start(() => Bridge.playerDj(), t("Couldn't start the DJ"), "spotify");
  }

  /** A play or the DJ: one at a time, "Starting…" meanwhile, then what happened. */
  async function start(run: () => Promise<PlayerOutcome<PlayResult>>, failText: string, source: PlayerSourceName) {
    used();
    if (starting) {
      paintFoot(); // the press is ignored: the first one is still starting
      return;
    }
    starting = true;
    notice = { text: t("Starting…"), tone: "", until: Infinity };
    paintFoot();
    try {
      const res = playReport(await run(), failText);
      notice = null;
      if (!res.ok) {
        if (res.loginNeeded) loginNeeded = true;
        say(res.text, "err");
        return;
      }
      loginNeeded = false;
      if (source === "spotify") spotifyOff = false;
      if (res.text) say(res.text);
      else paintFoot();
    } finally {
      starting = false;
    }
  }

  /** Says why something failed, and offers to sign in to Spotify when that is why. */
  function report(e: PlayerFailure, fallback: string) {
    if (e.loginNeeded) loginNeeded = true;
    say(e.message || fallback, "err");
  }

  // ── Rendering ──
  function paintBody() {
    const typed = query !== "";
    idleSlot.style.display = !typed && !artistOpen ? "" : "none";
    list.style.display = typed && !artistOpen ? "" : "none";
    artistPane.style.display = artistOpen ? "" : "none";
  }

  function paintFoot() {
    clear(foot);
    const said = footContent({ notice, now: Date.now(), query, errors, loadingMore, loginNeeded, spotifyOff, source: source() });
    const parts: Node[] = said.lines.map((l) => h("span", { class: l.err ? "pl-msg err" : "pl-msg", text: l.text }));
    if (said.offerLogin) {
      const b = h("button", { class: "pl-login", text: t("Connect Spotify") });
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        void connectSpotify();
      });
      parts.push(b);
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
    const row = baseRow(item, source() === "both");
    row.setAttribute("data-idx", String(index));
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

  /** One item as a row: its cover, title, artist, kind and (with both sources mixed) source. */
  function baseRow(item: PlayerItem, dot: boolean): HTMLElement {
    const art = h("div", { class: item.kind === "artist" ? "pl-thumb round" : "pl-thumb" });
    setImage(art, null, item.kind === "artist" ? PERSON : ICONS.note, 12);
    if (item.artUrl) {
      void thumb(item.artUrl).then((src) => {
        if (src) setImage(art, src, ICONS.note, 12);
      });
    }
    const kind = kindLabel(item.kind);
    return h(
      "div",
      { class: "pl-row", role: "option" },
      art,
      h(
        "div",
        { class: "pl-row-text" },
        h("div", { class: "pl-row-title", text: item.title }),
        item.subtitle ? h("div", { class: "pl-row-sub", text: item.subtitle }) : null,
      ),
      kind ? h("span", { class: "pl-kind", text: kind }) : null,
      // With both sources mixed, a dot says which one a row comes from.
      dot
        ? h("i", { class: "pl-src-dot", title: SOURCE_NAME[item.source], style: `background:${SOURCE_COLOR[item.source]}` })
        : null,
    );
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
    paintFoot(); // the sign-in button goes with a YT Music only choice
    if (query) void runSearch();
    input.focus();
  }

  function paintSources() {
    const current = source();
    for (const [opt, b] of sourceButtons) b.classList.toggle("on", opt === current);
    // The DJ is Spotify's: it goes with a YT Music only choice and comes back with Spotify.
    if ((djRow.style.display === "none") === djShown()) paintIdle();
  }

  async function connectSpotify() {
    used();
    const err = await Bridge.playerSpotifyLogin();
    if (err) {
      say(err, "err");
      return;
    }
    loginNeeded = false;
    spotifyOff = false;
    paintFoot();
    if (query) void runSearch();
  }

  async function activate(item: PlayerItem) {
    used();
    if (item.kind === "artist") {
      void openArtist(item);
      return;
    }
    await start(() => Bridge.playerPlay(item), t("Couldn't play this"), item.source);
  }

  // ── An artist: name, albums in a cover flow, songs beside them ──
  const artistFace = h("div", { class: "pl-artist-face" });
  const artistName = h("div", { class: "pl-artist-name" });
  const backBtn = control(ICONS.chevronLeft, 12, t("Back"), () => back());
  backBtn.classList.add("pl-back");
  const flowBox = h("div", { class: "pl-artist-flow" });
  const songsHead = h("div", { class: "pl-songs-head" });
  const songs = h("div", { class: "pl-songs", tabindex: "-1", role: "listbox" });
  const artistMain = h("div", { class: "pl-artist-main" }, flowBox, h("div", { class: "pl-artist-songs" }, songsHead, songs));
  artistPane.append(
    h(
      "div",
      { class: "pl-artist-head" },
      backBtn,
      artistFace,
      h("div", { class: "pl-artist-id" }, artistName, h("div", { class: "pl-row-sub" }, tl("Artist"))),
    ),
    artistMain,
  );
  /** Which songs the list shows: the artist's own, or one album's. */
  let shownAlbum: PlayerItem | null = null;

  async function openArtist(item: PlayerItem) {
    used();
    artistOpen = item;
    const seq = artistGate.next();
    artistName.textContent = item.title;
    setImage(artistFace, null, PERSON, 14);
    if (item.artUrl) {
      void thumb(item.artUrl).then((src) => {
        if (src && artistOpen === item) setImage(artistFace, src, PERSON, 14);
      });
    }
    albums = [];
    artistTracks = [];
    shownAlbum = null;
    flow = null;
    clear(flowBox);
    artistMain.classList.add("no-albums");
    paintSongs(null);
    paintBody();
    songs.focus();
    // Asked of the artist's own source: the selector may say "both".
    const res = await Bridge.playerArtist(item.source, item.id, item.title);
    if (!artistGate.isCurrent(seq) || artistOpen !== item) return;
    const page: ArtistPage | null = res.ok ? res.value : null;
    if (!res.ok) report(res.error, t("Couldn't load the songs"));
    albums = page?.albums ?? [];
    artistTracks = page?.tracks ?? [];
    if (albums.length > 0) {
      flow = buildCoverflow(albums, (album) => void openAlbum(album), thumb);
      flowBox.append(flow.el);
      artistMain.classList.remove("no-albums");
    }
    paintSongs(page ? artistTracks : [], page === null); // a failed load says so instead of loading forever
    // The keys go to the covers first, else to the songs (never into the field: Backspace goes back).
    if (flow) flow.el.focus();
    else songs.focus();
  }

  async function openAlbum(album: PlayerItem) {
    used();
    const artistAt = artistOpen;
    const seq = artistGate.next();
    shownAlbum = album;
    paintSongs(null);
    const res = await Bridge.playerAlbum(album.source, album.id);
    if (!artistGate.isCurrent(seq) || artistOpen !== artistAt) return;
    if (!res.ok) report(res.error, t("Couldn't load the songs"));
    paintSongs(res.ok ? res.value : [], !res.ok);
  }

  function showArtistSongs() {
    used();
    artistGate.next(); // an album still on its way is no longer wanted
    shownAlbum = null;
    paintSongs(artistTracks);
  }

  /** `list` null: still loading. */
  function paintSongs(list: PlayerItem[] | null, failedLoad = false) {
    clear(songsHead);
    const songsTab = h("button", { class: shownAlbum ? "pl-tab" : "pl-tab on" }, tl("Songs"));
    songsTab.addEventListener("mousedown", (e) => e.preventDefault());
    songsTab.addEventListener("click", (e) => {
      e.stopPropagation();
      if (shownAlbum) showArtistSongs();
    });
    songsHead.append(songsTab);
    const album = shownAlbum;
    if (album) {
      songsHead.append(h("span", { class: "pl-tab on", title: album.title, text: album.title }));
      const playAll = control(ICONS.play, 9, t("Play the album"), () => void activate(album));
      playAll.classList.add("pl-play-album");
      songsHead.append(playAll);
    }
    clear(songs);
    const shown = list ?? [];
    const rows = shown.map((it, i) => {
      const row = baseRow(it, false);
      row.addEventListener("mousedown", (e) => e.preventDefault());
      row.addEventListener("mousemove", () => {
        if (trackNav.sel !== i) trackNav.select(i, false);
      });
      row.addEventListener("click", (e) => {
        e.stopPropagation();
        void activate(it);
      });
      return row;
    });
    trackNav.set(rows, shown.map((it) => () => void activate(it)));
    if (rows.length > 0) songs.append(...rows);
    else {
      const text = list === null ? tl("Loading…") : failedLoad ? tl("Couldn't load the songs") : tl("No songs");
      songs.append(h("div", { class: "pl-empty" }, text));
    }
    songs.scrollTop = 0;
  }

  /** Back from an artist to the results (or to the recents it was opened from). */
  function back() {
    used();
    artistOpen = null;
    artistGate.next();
    flow = null;
    paintBody();
    input.focus();
  }

  // ── Keys (Esc is the island's, which hands it to escape()) ──
  input.addEventListener("input", () => {
    used();
    scheduleSearch();
  });
  /** Nothing typed and no artist open: the arrows go through the DJ and the recents. */
  const idle = () => !artistOpen && query === "" && input.value.trim() === "" && debounce == null;

  input.addEventListener("keydown", (e) => {
    used();
    if (artistOpen) {
      artistKey(e, true);
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (idle()) {
        e.preventDefault();
        idleNav.arrow(e.key);
        return;
      }
      if (items.length === 0) return;
      e.preventDefault();
      select(nextIndex(selected, items.length, e.key), true);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (idle()) {
        idleNav.run();
        return;
      }
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
    // Backspace and everything else edit the text.
  });

  /**
   * Keys on the artist page. From the field (`inField`) the text comes first:
   * Backspace goes back only when the field is empty, and Left/Right move the
   * caret. Elsewhere (the cover flow, the songs) Backspace always goes back.
   */
  function artistKey(e: KeyboardEvent, inField: boolean) {
    if (e.defaultPrevented) return; // the cover flow took it
    if (e.key === "Backspace") {
      if (inField && input.value !== "") return;
      e.preventDefault();
      back();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (trackNav.rows.length === 0) return;
      e.preventDefault();
      // Up from the first song gives the keys back to the cover flow.
      if (e.key === "ArrowUp" && trackNav.sel <= 0 && flow && flow.center() >= 0) {
        trackNav.select(-1, false);
        flow.el.focus();
        return;
      }
      if (e.key === "ArrowDown" && trackNav.sel === trackNav.rows.length - 1) return;
      trackNav.arrow(e.key);
      if (!inField) songs.focus();
    } else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && !inField) {
      e.preventDefault();
      flow?.move(e.key === "ArrowLeft" ? -1 : 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      // A song chosen with the arrows plays; otherwise the album in front opens.
      if (trackNav.run()) return;
      const front = flow ? albums[flow.center()] : undefined;
      if (front) void openAlbum(front);
    }
  }

  // Keys typed anywhere but the field (the cover flow, the songs) reach the page here.
  root.addEventListener("keydown", (e: KeyboardEvent) => {
    if (e.target === input || !artistOpen) return;
    // A button reached with Tab handles its own keys (Enter presses it).
    if ((e.target as HTMLElement | null)?.tagName === "BUTTON") return;
    artistKey(e, false);
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
      if (visible() && !shown) onShow();
      paintNow();
      paintSources();
      paintBody();
    },
    focus() {
      // The island calls this on every entry into the view, also when it came back
      // before the idle timer noticed it had gone.
      onShow();
      input.focus();
      input.select();
    },
    tick(nowMs: number) {
      const m = State.music;
      bars.draw(nowMs, m?.status === "Playing" && !State.paused, State.settings.musicVisualizer);
    },
    pointer(inside: boolean) {
      pointerIn = inside;
      if (shown) used(); // leaving starts the 30 s from now
    },
    openArtist(item: PlayerItem) {
      void openArtist(item);
    },
    escape() {
      used();
      if (artistOpen) {
        back();
        return;
      }
      if (State.playerFrom === "music") actions.setView("music");
      else if (!State.isPinned) actions.collapse(); // pinned: Esc leaves it open, as in the other views
    },
  };
  paintIdle();
  paintSources();
  paintBody();
  return host;
}
