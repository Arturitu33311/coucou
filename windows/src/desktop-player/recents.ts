// "Continue listening" panel on the left (#dp-recents).
// Shows the currently playing track as a large glass card, followed by
// recent tracks turned towards the centre (rotateY(22deg), or its 2D likeness
// where the engine draws no 3D: tilt.ts). The list itself is read by main.ts
// once per song (the same read feeds the cover flow).
//
// The column scrolls (wheel, or a drag with the mouse) and its cards can travel
// up over the clock, as in the reference; the slot is a tall viewport whose top
// and bottom fade (desktop.css), and the header rests where the mockup puts it.

import { Bridge } from "../core/bridge";
import type { PlayerItem } from "../core/player";
import type { MusicTrack } from "../core/state";
import { t } from "../i18n/i18n";
import type { Feedback } from "./feedback";
import { ROW_PERSPECTIVE, ROW_TURN, turnTransform } from "./tilt";

/**
 * A row's (and the "now" card's) style: turned towards the centre, never sunk
 * (no translateZ). Every row is opaque: the column's masks fade it at its edges,
 * wherever the scroll has taken it.
 */
export function recentRowStyle(flat = false): { transform: string; opacity: number } {
  return {
    transform: turnTransform(ROW_TURN, flat, { perspective: ROW_PERSPECTIVE }),
    opacity: 1,
  };
}

/** How far (screen px) the pointer must move before a press becomes a drag (and no click). */
export const DRAG_SLOP = 6;

/** The scroll position for a drag of `dy` screen px from `startTop`, on a stage scaled by `scale`. */
export function dragTop(startTop: number, dy: number, scale: number): number {
  return Math.max(0, startTop - dy / (scale > 0 ? scale : 1));
}

export interface RecentsView {
  root: HTMLElement;
  /** `source`: the friendly source name (logic.ts `sourceLabel`). */
  setTrack(track: MusicTrack | null, artData: string | null, source?: string): void;
  /** What was played lately, newest first. */
  setItems(items: PlayerItem[]): void;
  relabel(): void;
}

export function buildRecents(scroller: HTMLElement, feedback: Feedback, flat = false): RecentsView {
  scroller.className = "dp-recents-scroll";
  scroller.innerHTML = "";
  const container = document.createElement("div");
  container.className = "stack dp-recents-container";
  scroller.append(container);
  dragToScroll(scroller);
  const tilt = recentRowStyle(flat);

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
  nowCard.style.transform = tilt.transform;
  nowCard.style.display = "none";

  const rowsContainer = document.createElement("div");
  rowsContainer.className = "dp-recent-rows";

  container.append(header, nowCard, rowsContainer);

  let currentTrack: MusicTrack | null = null;
  let currentArt: string | null = null;
  let currentSource: string | undefined;
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
    const src = currentSource || currentTrack.player || "Coucou";

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
    // all of them (the history keeps 20): the older ones are a scroll away
    items.forEach((item) => {
      const row = document.createElement("div");
      row.className = "lg tilt dp-recent-row";
      row.style.transform = tilt.transform;
      row.style.opacity = String(tilt.opacity);

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
    root: scroller,
    setTrack(t: MusicTrack | null, artData: string | null, source?: string) {
      currentTrack = t;
      currentArt = artData;
      currentSource = source;
      renderNow();
    },
    setItems(list: PlayerItem[]) {
      items = list;
      renderRows();
    },
    relabel,
  };
}

/**
 * A press and a vertical drag scrolls the column (the wheel scrolls it natively).
 * Past DRAG_SLOP it is a drag: the pointer is captured and the click that ends
 * it is swallowed, so a drag never plays the row it started on.
 */
function dragToScroll(el: HTMLElement) {
  let drag: { id: number; y: number; top: number; moved: boolean } | null = null;
  let swallow = false;
  /** Screen px per layout px (the stage is scaled to the screen). */
  const scale = () => (el.offsetHeight > 0 ? el.getBoundingClientRect().height / el.offsetHeight : 1);

  el.addEventListener("pointerdown", (e) => {
    swallow = false;
    if (e.button !== 0) return;
    drag = { id: e.pointerId, y: e.clientY, top: el.scrollTop, moved: false };
  });
  el.addEventListener("pointermove", (e) => {
    if (!drag || e.pointerId !== drag.id) return;
    const dy = e.clientY - drag.y;
    if (!drag.moved) {
      if (Math.abs(dy) < DRAG_SLOP) return;
      drag.moved = true;
      el.classList.add("dragging");
      try {
        el.setPointerCapture(e.pointerId);
      } catch {
        // the pointer is gone already: the drag ends with it
      }
    }
    el.scrollTop = dragTop(drag.top, dy, scale());
  });
  const end = (e: PointerEvent) => {
    if (!drag || e.pointerId !== drag.id) return;
    swallow = drag.moved;
    drag = null;
    el.classList.remove("dragging");
  };
  el.addEventListener("pointerup", end);
  el.addEventListener("pointercancel", end);
  el.addEventListener(
    "click",
    (e) => {
      if (!swallow) return;
      swallow = false;
      e.stopPropagation();
      e.preventDefault();
    },
    true,
  );
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
