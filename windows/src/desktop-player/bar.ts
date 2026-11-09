// Player bar at the bottom (#dp-bar).
// Shows media controls (prev/play/next), mini cover, track title & source,
// progress bar with time, and DJ / lyrics buttons.

import { Bridge } from "../core/bridge";
import type { MusicInfo, MusicTrack } from "../core/state";
import { musicPosition } from "../island/music";
import { t } from "../i18n/i18n";

export interface PlayerBarView {
  root: HTMLElement;
  setTrack(track: MusicTrack | null, artData: string | null): void;
  tick(): void;
}

export function formatTime(ms: number): string {
  if (ms <= 0 || !Number.isFinite(ms)) return "0:00";
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${s < 10 ? "0" : ""}${s}`;
}

export function buildPlayerBar(container: HTMLElement): PlayerBarView {
  container.className = "lg dp-player-bar";
  container.innerHTML = `
    <div class="dp-bar-controls">
      <button type="button" class="dp-bar-btn dp-btn-prev" aria-label="${t("Previous")}">
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
          <path d="M19 5L9 12l10 7V5z"></path>
          <path d="M5 5v14"></path>
        </svg>
      </button>
      <button type="button" class="dp-bar-play-btn" aria-label="${t("Play")}">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M7 4l13 8-13 8z"></path>
        </svg>
      </button>
      <button type="button" class="dp-bar-btn dp-btn-next" aria-label="${t("Next")}">
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
          <path d="M5 5l10 7-10 7V5z"></path>
          <path d="M19 5v14"></path>
        </svg>
      </button>
    </div>
    <div class="cover dp-bar-cover"></div>
    <div class="dp-bar-middle">
      <div class="dp-bar-meta">
        <span class="dp-bar-title">${t("Nothing is playing")}</span>
        <span class="dp-bar-time">0:00 / 0:00</span>
      </div>
      <div class="dp-bar-track" role="progressbar" aria-valuenow="0" aria-valuemin="0" aria-valuemax="100">
        <div class="dp-bar-fill" style="width: 0%;"></div>
      </div>
      <span class="dp-bar-sub">Coucou</span>
    </div>
    <div class="dp-bar-actions">
      <button type="button" class="dp-bar-btn dp-btn-lyrics" aria-label="${t("Lyrics")}">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
          <path d="M4 6h16M4 12h10M4 18h13"></path>
        </svg>
      </button>
      <button type="button" class="dp-bar-btn dp-btn-dj" aria-label="DJ" title="${t("Music picked for you by Spotify's DJ")}">DJ</button>
    </div>
  `;

  const prevBtn = container.querySelector<HTMLButtonElement>(".dp-btn-prev")!;
  const playBtn = container.querySelector<HTMLButtonElement>(".dp-bar-play-btn")!;
  const nextBtn = container.querySelector<HTMLButtonElement>(".dp-btn-next")!;
  const djBtn = container.querySelector<HTMLButtonElement>(".dp-btn-dj")!;
  const coverEl = container.querySelector<HTMLElement>(".dp-bar-cover")!;
  const titleEl = container.querySelector<HTMLElement>(".dp-bar-title")!;
  const timeEl = container.querySelector<HTMLElement>(".dp-bar-time")!;
  const fillEl = container.querySelector<HTMLElement>(".dp-bar-fill")!;
  const trackEl = container.querySelector<HTMLElement>(".dp-bar-track")!;
  const subEl = container.querySelector<HTMLElement>(".dp-bar-sub")!;

  let currentTrack: MusicTrack | null = null;
  let currentArt: string | null = null;

  prevBtn.addEventListener("click", () => void Bridge.musicControl("previous"));
  playBtn.addEventListener("click", () => void Bridge.musicControl("playpause"));
  nextBtn.addEventListener("click", () => void Bridge.musicControl("next"));
  djBtn.addEventListener("click", () => void Bridge.playerDj());

  trackEl.addEventListener("click", (e) => {
    if (!currentTrack || currentTrack.lengthMs <= 0) return;
    const rect = trackEl.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const seekMs = Math.round(ratio * currentTrack.lengthMs);
    void Bridge.musicControl("seek", seekMs);
  });

  function updateProgress() {
    if (!currentTrack || currentTrack.lengthMs <= 0) {
      timeEl.textContent = "0:00 / 0:00";
      fillEl.style.width = "0%";
      return;
    }
    const pos = musicPosition(currentTrack as MusicInfo);
    const len = currentTrack.lengthMs;
    const pct = Math.max(0, Math.min(100, (pos / len) * 100));
    timeEl.textContent = `${formatTime(pos)} / ${formatTime(len)}`;
    fillEl.style.width = `${pct}%`;
    trackEl.setAttribute("aria-valuenow", String(Math.round(pct)));
  }

  function setTrack(tInfo: MusicTrack | null, artData: string | null) {
    currentTrack = tInfo;
    currentArt = artData;

    const isPlaying = tInfo?.status === "Playing";
    playBtn.setAttribute("aria-label", isPlaying ? t("Pause") : t("Play"));
    playBtn.innerHTML = isPlaying
      ? '<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="1"></rect><rect x="14" y="5" width="4" height="14" rx="1"></rect></svg>'
      : '<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M7 4l13 8-13 8z"></path></svg>';

    if (tInfo && tInfo.title) {
      titleEl.textContent = tInfo.title;
      titleEl.title = tInfo.title;
      const artist = tInfo.artists.join(", ") || t("Artist");
      const src = tInfo.player ? `${artist} · ${tInfo.player}` : artist;
      subEl.textContent = src;
      subEl.title = src;
    } else {
      titleEl.textContent = t("Nothing is playing");
      titleEl.title = "";
      subEl.textContent = "Coucou";
      subEl.title = "";
    }

    if (currentArt) {
      coverEl.style.background = `url('${currentArt}') center/cover`;
    } else {
      coverEl.style.background = "linear-gradient(135deg, rgba(255,255,255,0.15), rgba(255,255,255,0.05))";
    }

    updateProgress();
  }

  return {
    root: container,
    setTrack,
    tick: updateProgress,
  };
}
