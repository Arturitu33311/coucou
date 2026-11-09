// Search bar & results overlay (#dp-search).
// Spotify, YT Music, or both. The results open above the field, row 0 selected so
// Enter plays the first one, as in the notch (views/player.ts, whose QueryGate,
// debounce and arrow rule it uses). An album picked in the cover flow lists its
// songs here too. What happens (starting, failures, "Connect Spotify") goes to
// the shared feedback line (feedback.ts).
//
// The window only takes the keyboard when asked: opening the search asks Rust
// for it (desktop_player_focus) before focusing the field, and gives it back on
// Esc and when the field loses focus.

import { Bridge } from "../core/bridge";
import type { PlayerItem, PlayerSource, SourceError } from "../core/player";
import { DEBOUNCE_MS, nextIndex, QueryGate } from "../views/player";
import { t } from "../i18n/i18n";
import type { Feedback } from "./feedback";

export { DEBOUNCE_MS, nextIndex, QueryGate };

/** How long after the field loses focus the keyboard is given back (a row click lands first). */
const RELEASE_MS = 150;

/** The row selected after a fresh list: the first, so Enter plays it (the notch's paintList). */
export function firstSelection(len: number): number {
  return len > 0 ? 0 : -1;
}

export interface SearchView {
  root: HTMLElement;
  focusSearch(): void;
  setSearch(query: string): void;
  /** Lists an album's songs in the results (the cover flow's front cover was picked). */
  showAlbum(album: PlayerItem): void;
  /** The interface language changed. */
  relabel(): void;
}

export function buildSearch(container: HTMLElement, feedback: Feedback): SearchView {
  container.className = "dp-search";
  container.replaceChildren();

  const bar = document.createElement("div");
  bar.className = "lg dp-search-bar";
  bar.innerHTML = `
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.8)" stroke-width="2" stroke-linecap="round" aria-hidden="true">
      <circle cx="11" cy="11" r="7"></circle>
      <path d="M20 20l-3.5-3.5"></path>
    </svg>
    <label for="dp-search-input" class="dp-visually-hidden"></label>
    <input id="dp-search-input" type="search" autocomplete="off" />
    <div class="dp-search-sources">
      <button type="button" class="dp-src-btn" data-src="spotify">Spotify</button>
      <button type="button" class="dp-src-btn" data-src="ytmusic">YT Music</button>
      <button type="button" class="dp-src-btn active" data-src="both"></button>
    </div>
  `;

  // Above the field: the results, then the feedback line right over the field.
  const over = document.createElement("div");
  over.className = "dp-search-over";
  const resultsPanel = document.createElement("div");
  resultsPanel.className = "lg dp-search-results";
  resultsPanel.style.display = "none";
  resultsPanel.setAttribute("role", "listbox");
  over.append(resultsPanel, feedback.el);
  container.append(over, bar);

  const input = bar.querySelector<HTMLInputElement>("#dp-search-input")!;
  const label = bar.querySelector<HTMLLabelElement>("label")!;
  const bothBtn = bar.querySelector<HTMLButtonElement>('[data-src="both"]')!;
  const sourceButtons = bar.querySelectorAll<HTMLButtonElement>(".dp-src-btn");

  let activeSource: PlayerSource = "both";
  const gate = new QueryGate();
  let debounceTimer: number | null = null;
  let releaseTimer: number | null = null;
  /** What the panel lists: a search's results, or an album's songs. */
  let album: PlayerItem | null = null;
  let query = "";
  let items: PlayerItem[] = [];
  let errors: SourceError[] = [];
  let loading = false;
  let failed = false;
  let selectedIndex = -1;
  let open = false;

  feedback.side = {
    query: () => (album ? "" : query),
    errors: () => (album ? [] : errors),
    source: () => activeSource,
    rerun: () => {
      if (query) void triggerSearch(query);
    },
    open: () => open,
  };

  function relabel() {
    const text = t("Search songs and artists");
    input.placeholder = text;
    label.textContent = text;
    bothBtn.textContent = t("Both");
    if (open) render();
    feedback.paint();
  }
  relabel();

  function setSource(src: PlayerSource) {
    activeSource = src;
    sourceButtons.forEach((btn) => {
      btn.classList.toggle("active", btn.getAttribute("data-src") === src);
    });
    feedback.paint(); // the sign-in offer goes with a YT Music only choice
    const q = input.value.trim();
    if (q) void triggerSearch(q);
  }

  sourceButtons.forEach((btn) => {
    btn.addEventListener("mousedown", (e) => e.preventDefault()); // the field keeps the keyboard
    btn.addEventListener("click", () => setSource(btn.getAttribute("data-src") as PlayerSource));
  });

  // ── Keyboard: asked for explicitly, given back on Esc / blur ──
  function cancelRelease() {
    if (releaseTimer != null) window.clearTimeout(releaseTimer);
    releaseTimer = null;
  }

  async function focusSearch() {
    cancelRelease();
    void feedback.checkSpotify();
    // A desktop-type window without the keyboard gets no focus event: ask Rust first.
    await Bridge.desktopPlayerFocus(true);
    input.focus();
    // GTK may hand the keyboard over a moment later: focus the field again then.
    window.addEventListener("focus", () => input.focus(), { once: true });
    if (open || items.length > 0 || album) show();
  }

  function release() {
    cancelRelease();
    void Bridge.desktopPlayerFocus(false);
  }

  input.addEventListener("mousedown", () => {
    if (document.activeElement !== input) void focusSearch();
  });
  input.addEventListener("focus", () => cancelRelease());
  input.addEventListener("blur", () => {
    cancelRelease();
    // a little later, so a click on a row registers first
    releaseTimer = window.setTimeout(() => {
      releaseTimer = null;
      release();
    }, RELEASE_MS);
  });

  input.addEventListener("input", () => {
    const q = input.value.trim();
    if (debounceTimer != null) window.clearTimeout(debounceTimer);
    debounceTimer = null;
    if (!q) {
      gate.next(); // drops an answer still on its way
      query = "";
      album = null;
      items = [];
      errors = [];
      hide();
      feedback.paint();
      return;
    }
    debounceTimer = window.setTimeout(() => {
      debounceTimer = null;
      void triggerSearch(q);
    }, DEBOUNCE_MS);
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      hide();
      input.blur();
      release();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      select(nextIndex(selectedIndex, items.length, e.key));
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (selectedIndex >= 0 && selectedIndex < items.length) pickItem(items[selectedIndex]);
    }
  });

  function show() {
    open = true;
    resultsPanel.style.display = "block";
    feedback.paint();
  }

  function hide() {
    open = false;
    resultsPanel.style.display = "none";
    feedback.paint();
  }

  function select(index: number) {
    selectedIndex = index;
    resultsPanel.querySelectorAll<HTMLElement>(".dp-res-row").forEach((row, idx) => {
      row.classList.toggle("selected", idx === selectedIndex);
      if (idx === selectedIndex) row.scrollIntoView({ block: "nearest" });
    });
  }

  function pickItem(item: PlayerItem) {
    hide();
    input.blur();
    release();
    void feedback.start(() => Bridge.playerPlay(item), t("Couldn't play this"), item.source);
  }

  function line(cls: string, text: string): HTMLElement {
    const e = document.createElement("div");
    e.className = cls;
    e.textContent = text;
    return e;
  }

  function row(item: PlayerItem, index: number): HTMLElement {
    const r = document.createElement("div");
    r.className = "dp-res-row";
    r.setAttribute("role", "option");
    const cover = document.createElement("div");
    cover.className = "cover dp-res-cover";
    cover.style.background = "rgba(255, 255, 255, 0.1)";
    if (item.artUrl) {
      void Bridge.playerArt(item.artUrl).then((src) => {
        if (src) cover.style.background = `url('${src}') center/cover`;
      }).catch(() => {});
    }
    const info = document.createElement("div");
    info.className = "dp-res-info";
    const title = document.createElement("span");
    title.className = "dp-res-title";
    title.textContent = item.title;
    title.title = item.title;
    const sub = document.createElement("span");
    sub.className = "dp-res-sub";
    sub.textContent = item.subtitle;
    info.append(title, sub);
    const badge = document.createElement("span");
    badge.className = "dp-res-badge";
    badge.style.background = item.source === "spotify" ? "#1ed760" : "#ff5c5c";
    badge.title = item.source === "spotify" ? "Spotify" : "YT Music";
    r.append(cover, info, badge);
    r.addEventListener("mousedown", (e) => e.preventDefault()); // the field keeps the keyboard
    r.addEventListener("mousemove", () => {
      if (selectedIndex !== index) select(index);
    });
    r.addEventListener("click", () => pickItem(item));
    return r;
  }

  /** The panel from the state: a heading for an album, the rows, or why there are none. */
  function render() {
    resultsPanel.replaceChildren();
    if (album) resultsPanel.append(line("dp-search-head", `${album.title} · ${t("Album")}`));
    if (items.length > 0) {
      resultsPanel.append(...items.map((it, i) => row(it, i)));
    } else if (loading) {
      resultsPanel.append(line("dp-search-empty", album ? t("Loading…") : t("Searching…")));
    } else if (album) {
      resultsPanel.append(line("dp-search-empty", failed ? t("Couldn't load the songs") : t("No results")));
    } else if (failed) {
      resultsPanel.append(line("dp-search-empty", t("Search isn't available right now")));
    } else if (errors.length === 0) {
      resultsPanel.append(line("dp-search-empty", t("No results")));
    }
    // (a source's error and the sign-in offer are on the feedback line)
    resultsPanel.scrollTop = 0;
    select(firstSelection(items.length));
  }

  async function triggerSearch(q: string) {
    const seq = gate.next();
    album = null;
    query = q;
    items = [];
    errors = [];
    failed = false;
    loading = true;
    render();
    show();
    feedback.paint();
    const res = await Bridge.playerSearch(activeSource, q, 0, seq);
    if (!gate.isCurrent(seq) || (res && res.seq !== seq)) return; // a newer query is on its way
    loading = false;
    failed = res === null;
    items = res?.items ?? [];
    errors = res?.errors ?? [];
    render();
    feedback.paint();
  }

  async function showAlbum(a: PlayerItem) {
    const seq = gate.next();
    if (debounceTimer != null) window.clearTimeout(debounceTimer);
    debounceTimer = null;
    album = a;
    query = "";
    input.value = "";
    items = [];
    errors = [];
    failed = false;
    loading = true;
    render();
    show();
    feedback.paint();
    const res = await Bridge.playerAlbum(a.source, a.id);
    if (!gate.isCurrent(seq)) return;
    loading = false;
    if (res.ok) {
      items = res.value;
    } else {
      failed = true;
      feedback.report(res.error, t("Couldn't load the songs"));
    }
    render();
  }

  return {
    root: container,
    focusSearch: () => void focusSearch(),
    setSearch(q: string) {
      input.value = q;
      void triggerSearch(q);
    },
    showAlbum: (a) => void showAlbum(a),
    relabel,
  };
}
