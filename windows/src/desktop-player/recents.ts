// "Continue listening" panel on the left (#dp-recents).
// Shows the currently playing track as a large glass card, followed by
// recent tracks tilted with rotateY(22deg) and fading out. The list itself is
// read by main.ts once per song (the same read feeds the cover flow).

import { Bridge } from "../core/bridge";
import type { PlayerItem } from "../core/player";
import type { MusicTrack } from "../core/state";
import { t } from "../i18n/i18n";
import type { Feedback } from "./feedback";

/** Pure helper: calculates the tilt and opacity for recent item rows without depth (translateZ). */
export function recentRowStyle(index: number): { transform: string; opacity: number } {
  return {
    transform: "rotateY(22deg)",
    opacity: Math.max(0.3, 1 - index * 0.13),
  };
}

export interface RecentsView {
  root: HTMLElement;
  setTrack(track: MusicTrack | null, artData: string | null): void;
  /** What was played lately, newest first. */
  setItems(items: PlayerItem[]): void;
  relabel(): void;
}

export function buildRecents(container: HTMLElement, feedback: Feedback): RecentsView {
  container.className = "stack dp-recents-container";
  container.innerHTML = "";

  const header = document.createElement("div");
  header.className = "lg dp-recents-header";
  header.innerHTML = `
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true">
      <path d="M3 12a9 9 0 1 0 3-6.7"></path>
      <path d="M3 4v4h4"></path>
    </svg>
    <span class="dp-recents-title"></span>
  `;
  const headerTitle = header.querySelector<HTMLElement>(".dp-recents-title")!;

  const nowCard = document.createElement("div");
  nowCard.className = "lg tilt dp-now-card";
  nowCard.style.transform = "rotateY(22deg)";
  nowCard.style.display = "none";

  const rowsContainer = document.createElement("div");
  rowsContainer.className = "dp-recent-rows";

  container.append(header, nowCard, rowsContainer);

  let currentTrack: MusicTrack | null = null;
  let currentArt: string | null = null;
  let items: PlayerItem[] = [];

  function renderNow() {
    if (!currentTrack || !currentTrack.title) {
      nowCard.style.display = "none";
      nowCard.innerHTML = "";
      return;
    }
    nowCard.style.display = "flex";
    const isPlaying = currentTrack.status === "Playing";
    const artBg = currentArt ? `url('${currentArt}') center/cover` : "rgba(255, 255, 255, 0.12)";
    const artist = currentTrack.artists.join(", ") || t("Artist");
    const src = currentTrack.player || "Coucou";

    nowCard.innerHTML = `
      <div class="cover dp-now-cover" style="background: ${artBg};"></div>
      <div class="dp-now-info">
        <span class="dp-now-title" title="${escapeHtml(currentTrack.title)}">${escapeHtml(currentTrack.title)}</span>
        <span class="dp-now-sub">${escapeHtml(artist)} · ${escapeHtml(src)}</span>
      </div>
      <button type="button" class="dp-now-play" aria-label="${isPlaying ? t("Pause") : t("Play")}">
        ${isPlaying
          ? '<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1"></rect><rect x="14" y="5" width="4" height="14" rx="1"></rect></svg>'
          : '<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 4l13 8-13 8z"></path></svg>'}
      </button>
    `;

    const playBtn = nowCard.querySelector<HTMLButtonElement>(".dp-now-play");
    if (playBtn) {
      playBtn.addEventListener("click", () => {
        void Bridge.musicControl("playpause");
      });
    }
  }

  function renderRows() {
    rowsContainer.innerHTML = "";
    const list = items.slice(0, 6);
    list.forEach((item, i) => {
      const row = document.createElement("div");
      row.className = "lg tilt dp-recent-row";
      const style = recentRowStyle(i);
      row.style.transform = style.transform;
      row.style.opacity = String(style.opacity);

      const badgeColor = item.source === "spotify" ? "#1ed760" : "#ff5c5c";
      const sourceName = item.source === "spotify" ? "Spotify" : "YT Music";

      const coverEl = document.createElement("div");
      coverEl.className = "cover dp-recent-cover";
      coverEl.style.background = "rgba(255, 255, 255, 0.08)";
      if (item.artUrl) {
        void Bridge.playerArt(item.artUrl).then((data) => {
          if (data) coverEl.style.background = `url('${data}') center/cover`;
        }).catch(() => {});
      }

      row.innerHTML = `
        <div class="dp-recent-info">
          <span class="dp-recent-title" title="${escapeHtml(item.title)}">${escapeHtml(item.title)}</span>
          <span class="dp-recent-sub">${escapeHtml(item.subtitle)}</span>
        </div>
        <span class="dp-recent-badge" aria-label="${sourceName}" title="${sourceName}" style="background: ${badgeColor};"></span>
      `;
      row.prepend(coverEl);

      row.addEventListener("click", () => {
        void feedback.start(() => Bridge.playerPlay(item), t("Couldn't play this"), item.source);
      });

      rowsContainer.appendChild(row);
    });
  }

  function relabel() {
    headerTitle.textContent = t("Continue listening");
    renderNow();
  }
  relabel();

  return {
    root: container,
    setTrack(t: MusicTrack | null, artData: string | null) {
      currentTrack = t;
      currentArt = artData;
      renderNow();
    },
    setItems(list: PlayerItem[]) {
      items = list;
      renderRows();
    },
    relabel,
  };
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
