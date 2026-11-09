// Cover flow of albums (#dp-flow).
// Shows albums for the current artist (or falls back to the last artist from recents).
// If no artist can be found (empty history), the flow is hidden.

import { Bridge } from "../core/bridge";
import type { PlayerItem } from "../core/player";
import { t } from "../i18n/i18n";

/**
 * Pure helper: chooses the artist for the cover flow.
 * Uses the currently playing artist if available, otherwise falls back to the first
 * artist found in the recent history. Returns null if neither is available.
 */
export function flowArtist(currentArtist: string | null, recents: PlayerItem[]): string | null {
  if (currentArtist && currentArtist.trim()) {
    return currentArtist.trim();
  }
  for (const r of recents) {
    const candidate = r.kind === "artist" ? r.title : r.subtitle;
    if (candidate && candidate.trim()) {
      return candidate.trim();
    }
  }
  return null;
}

/** Pure helper: position, size, rotation, z-index and opacity for item `i` when `center` is active. */
export function flowTransform(i: number, center: number) {
  const d = i - center;
  const a = Math.abs(d);
  const size = a === 0 ? 300 : a === 1 ? 240 : 200;
  const x = 520 - size / 2 + (d === 0 ? 0 : Math.sign(d) * (130 + (a - 1) * 95));
  const rot = d === 0 ? 0 : (d < 0 ? 45 : -45);
  return {
    left: x,
    top: 150 - size / 2,
    size,
    rotateY: rot,
    z: 10 - a,
    opacity: a > 3 ? 0 : Math.max(0, 1 - a * 0.12),
  };
}

export interface FlowView {
  root: HTMLElement;
  update(currentArtist: string | null, recents: PlayerItem[]): Promise<void>;
}

export function buildFlow(container: HTMLElement, onPickAlbum?: (album: PlayerItem) => void): FlowView {
  container.className = "dp-flow-container";
  container.innerHTML = "";

  const stage = document.createElement("div");
  stage.className = "dp-flow-stage";

  const caption = document.createElement("div");
  caption.className = "lg dp-flow-caption";
  caption.style.display = "none";

  container.append(stage, caption);

  let activeArtist: string | null = null;
  let albums: PlayerItem[] = [];
  let centerIndex = 0;
  const artCache = new Map<string, string>();

  function renderCovers() {
    stage.innerHTML = "";
    if (albums.length === 0) {
      container.style.display = "none";
      caption.style.display = "none";
      return;
    }
    container.style.display = "block";

    albums.forEach((item, i) => {
      const tf = flowTransform(i, centerIndex);
      const cover = document.createElement("div");
      cover.className = "cover dp-flow-cover";
      cover.style.position = "absolute";
      cover.style.left = `${tf.left}px`;
      cover.style.top = `${tf.top}px`;
      cover.style.width = `${tf.size}px`;
      cover.style.height = `${tf.size}px`;
      cover.style.transform = `rotateY(${tf.rotateY}deg)`;
      cover.style.zIndex = String(tf.z);
      cover.style.opacity = String(tf.opacity);
      cover.style.pointerEvents = tf.opacity > 0 ? "auto" : "none";

      const cached = item.artUrl ? artCache.get(item.artUrl) : null;
      if (cached) {
        cover.style.background = `url('${cached}') center/cover`;
      } else if (item.artUrl) {
        cover.style.background = "rgba(255, 255, 255, 0.1)";
        void Bridge.playerArt(item.artUrl).then((src) => {
          if (src) {
            artCache.set(item.artUrl, src);
            cover.style.background = `url('${src}') center/cover`;
          }
        }).catch(() => {});
      } else {
        cover.style.background = "linear-gradient(135deg, rgba(255,255,255,0.15), rgba(255,255,255,0.03))";
      }

      cover.addEventListener("click", () => {
        if (i === centerIndex) {
          if (onPickAlbum) onPickAlbum(item);
        } else {
          centerIndex = i;
          renderCovers();
        }
      });

      stage.appendChild(cover);
    });

    const curr = albums[centerIndex];
    if (curr) {
      caption.style.display = "flex";
      caption.innerHTML = `
        <span class="dp-flow-title" title="${escapeHtml(curr.title)}">${escapeHtml(curr.title)}</span>
        <span class="dp-flow-sub">${escapeHtml(curr.subtitle || activeArtist || "")} · ${t("Album")}</span>
      `;
    } else {
      caption.style.display = "none";
    }
  }

  container.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (albums.length <= 1) return;
    if (e.deltaY > 0 || e.deltaX > 0) {
      if (centerIndex < albums.length - 1) {
        centerIndex++;
        renderCovers();
      }
    } else if (e.deltaY < 0 || e.deltaX < 0) {
      if (centerIndex > 0) {
        centerIndex--;
        renderCovers();
      }
    }
  }, { passive: false });

  async function update(artistName: string | null, recents: PlayerItem[]) {
    const chosen = flowArtist(artistName, recents);
    if (!chosen) {
      activeArtist = null;
      albums = [];
      renderCovers();
      return;
    }
    if (chosen === activeArtist && albums.length > 0) {
      return;
    }
    activeArtist = chosen;
    try {
      const outcome = await Bridge.playerArtist("both", "", chosen);
      if (outcome.ok && outcome.value.albums.length > 0) {
        albums = outcome.value.albums;
      } else {
        // Fallback: search for albums by this artist
        const searchRes = await Bridge.playerSearch("both", chosen, 0, 1);
        albums = searchRes?.items.filter((it) => it.kind === "album") || [];
      }
    } catch {
      albums = [];
    }
    centerIndex = Math.min(albums.length > 0 ? Math.floor(albums.length / 2) : 0, albums.length - 1);
    renderCovers();
  }

  return {
    root: container,
    update,
  };
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
