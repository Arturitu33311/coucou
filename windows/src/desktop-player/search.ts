// Search bar & results overlay (#dp-search).
// Supports Spotify, YT Music, and Both. Manages keyboard focus via desktop_player_focus,
// uses QueryGate to drop stale answers, and provides arrow navigation.

import { Bridge } from "../core/bridge";
import type { PlayerItem, PlayerSource, SearchResult } from "../core/player";
import { t } from "../i18n/i18n";

export const DEBOUNCE_MS = 250;

/** Numbers every query so only the newest answer is kept. */
export class QueryGate {
  private seq = 0;
  next(): number {
    return ++this.seq;
  }
  isCurrent(seq: number): boolean {
    return this.seq === seq;
  }
}

/** Pure helper: keyboard row selection wrapping around at ends. */
export function nextIndex(current: number, len: number, key: "ArrowUp" | "ArrowDown"): number {
  if (len <= 0) return -1;
  if (key === "ArrowDown") return current < 0 || current >= len - 1 ? 0 : current + 1;
  return current <= 0 || current > len - 1 ? len - 1 : current - 1;
}

export interface SearchView {
  root: HTMLElement;
  focusSearch(): void;
  setSearch(query: string): void;
}

export function buildSearch(container: HTMLElement, onSelect?: (item: PlayerItem) => void): SearchView {
  container.className = "lg dp-search-bar";
  container.innerHTML = `
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.8)" stroke-width="2" stroke-linecap="round" aria-hidden="true">
      <circle cx="11" cy="11" r="7"></circle>
      <path d="M20 20l-3.5-3.5"></path>
    </svg>
    <label for="dp-search-input" class="dp-visually-hidden">${t("Search songs and artists")}</label>
    <input id="dp-search-input" type="search" placeholder="${t("Search songs and artists")}" autocomplete="off" />
    <div class="dp-search-sources">
      <button type="button" class="dp-src-btn" data-src="spotify">Spotify</button>
      <button type="button" class="dp-src-btn" data-src="ytmusic">YT Music</button>
      <button type="button" class="dp-src-btn active" data-src="both">${t("Both")}</button>
    </div>
  `;

  const resultsPanel = document.createElement("div");
  resultsPanel.className = "lg dp-search-results";
  resultsPanel.style.display = "none";
  container.appendChild(resultsPanel);

  const input = container.querySelector<HTMLInputElement>("#dp-search-input")!;
  const sourceButtons = container.querySelectorAll<HTMLButtonElement>(".dp-src-btn");

  let activeSource: PlayerSource = "both";
  const gate = new QueryGate();
  let debounceTimer: number | null = null;
  let items: PlayerItem[] = [];
  let selectedIndex = -1;

  function setSource(src: PlayerSource) {
    activeSource = src;
    sourceButtons.forEach((btn) => {
      btn.classList.toggle("active", btn.getAttribute("data-src") === src);
    });
    if (input.value.trim()) {
      triggerSearch(input.value.trim());
    }
  }

  sourceButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      const src = btn.getAttribute("data-src") as PlayerSource;
      setSource(src);
    });
  });

  input.addEventListener("focus", () => {
    void Bridge.desktopPlayerFocus(true);
    if (items.length > 0) resultsPanel.style.display = "block";
  });

  input.addEventListener("blur", () => {
    // Delay taking away keyboard so item click can register
    window.setTimeout(() => {
      void Bridge.desktopPlayerFocus(false);
    }, 150);
  });

  input.addEventListener("input", () => {
    const q = input.value.trim();
    if (debounceTimer != null) window.clearTimeout(debounceTimer);
    if (!q) {
      items = [];
      resultsPanel.style.display = "none";
      return;
    }
    debounceTimer = window.setTimeout(() => {
      debounceTimer = null;
      triggerSearch(q);
    }, DEBOUNCE_MS);
  });

  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      resultsPanel.style.display = "none";
      input.blur();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      selectedIndex = nextIndex(selectedIndex, items.length, e.key);
      highlightSelected();
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (selectedIndex >= 0 && selectedIndex < items.length) {
        pickItem(items[selectedIndex]);
      }
    }
  });

  function highlightSelected() {
    const rows = resultsPanel.querySelectorAll<HTMLElement>(".dp-res-row");
    rows.forEach((row, idx) => {
      row.classList.toggle("selected", idx === selectedIndex);
      if (idx === selectedIndex) {
        row.scrollIntoView({ block: "nearest" });
      }
    });
  }

  function pickItem(item: PlayerItem) {
    resultsPanel.style.display = "none";
    input.blur();
    void Bridge.playerPlay(item);
    if (onSelect) onSelect(item);
  }

  function renderResults(res: SearchResult) {
    resultsPanel.innerHTML = "";
    items = res.items || [];
    selectedIndex = -1;

    // Show login errors if needed
    for (const err of res.errors || []) {
      if (err.loginNeeded) {
        const loginBanner = document.createElement("div");
        loginBanner.className = "dp-search-login-banner";
        loginBanner.innerHTML = `
          <span>${t("Connect Spotify")}</span>
          <button type="button" class="dp-search-login-btn">${t("Connect Spotify")}</button>
        `;
        loginBanner.querySelector("button")?.addEventListener("click", () => {
          void Bridge.playerSpotifyLogin();
        });
        resultsPanel.appendChild(loginBanner);
      }
    }

    if (items.length === 0 && (res.errors || []).length === 0) {
      const empty = document.createElement("div");
      empty.className = "dp-search-empty";
      empty.textContent = t("No results");
      resultsPanel.appendChild(empty);
    } else {
      items.forEach((item) => {
        const row = document.createElement("div");
        row.className = "dp-res-row";
        const badgeColor = item.source === "spotify" ? "#1ed760" : "#ff5c5c";

        const coverEl = document.createElement("div");
        coverEl.className = "cover dp-res-cover";
        coverEl.style.background = "rgba(255, 255, 255, 0.1)";
        if (item.artUrl) {
          void Bridge.playerArt(item.artUrl).then((src) => {
            if (src) coverEl.style.background = `url('${src}') center/cover`;
          }).catch(() => {});
        }

        row.innerHTML = `
          <div class="dp-res-info">
            <span class="dp-res-title" title="${escapeHtml(item.title)}">${escapeHtml(item.title)}</span>
            <span class="dp-res-sub">${escapeHtml(item.subtitle)}</span>
          </div>
          <span class="dp-res-badge" style="background: ${badgeColor};" title="${item.source}"></span>
        `;
        row.prepend(coverEl);

        row.addEventListener("mousedown", (e) => e.preventDefault());
        row.addEventListener("click", () => pickItem(item));
        resultsPanel.appendChild(row);
      });
    }

    resultsPanel.style.display = "block";
  }

  async function triggerSearch(query: string) {
    const seq = gate.next();
    try {
      const res = await Bridge.playerSearch(activeSource, query, 0, seq);
      if (!res || !gate.isCurrent(res.seq)) return;
      renderResults(res);
    } catch {
      if (!gate.isCurrent(seq)) return;
      resultsPanel.innerHTML = `<div class="dp-search-empty">${t("Search isn't available right now")}</div>`;
      resultsPanel.style.display = "block";
    }
  }

  return {
    root: container,
    focusSearch() {
      input.focus();
    },
    setSearch(q: string) {
      input.value = q;
      triggerSearch(q);
    },
  };
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
