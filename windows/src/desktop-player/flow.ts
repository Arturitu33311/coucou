// Cover flow of albums (#dp-flow).
// The albums of the current artist (or, with nothing playing, of the last artist
// played), placed with the notch's cover-flow rule (views/coverflow.ts) at the
// desktop's scale. Hidden when no artist can be found.
//
// Each artist's albums are asked for once (an empty answer is remembered too)
// and only when the artist changes; an answer for an artist no longer shown is
// dropped (QueryGate).

import { Bridge } from "../core/bridge";
import type { PlayerItem } from "../core/player";
import { coverflowTransform } from "../views/coverflow";
import { QueryGate } from "../views/player";
import { t } from "../i18n/i18n";

/** The first of a subtitle's artists ("Daft Punk, Pharrell Williams" → "Daft Punk"). */
export function firstArtist(subtitle: string): string {
  return subtitle.split(", ")[0].trim();
}

/**
 * The artist for the cover flow: the one playing (its first artist), else the
 * last one in the history — skipping the DJ and lists, which have no artist.
 * Null when there is neither.
 */
export function flowArtist(currentArtist: string | null, recents: PlayerItem[]): string | null {
  if (currentArtist && currentArtist.trim()) return currentArtist.trim();
  for (const r of recents) {
    if (r.kind === "context") continue;
    const candidate = r.kind === "artist" ? r.title.trim() : firstArtist(r.subtitle);
    if (candidate) return candidate;
  }
  return null;
}

/** The desktop's cover flow is the notch's (views/coverflow.ts) at this scale. */
export const FLOW_SCALE = 1.7;
/** Front cover's side, and where its centre is on the panel. */
const COVER = 300;
const CENTER_X = 520;
const CENTER_Y = 150;

/** Where cover `i` sits when cover `center` is in front: the notch's rule, scaled. */
export function flowTransform(i: number, center: number) {
  const p = coverflowTransform(i, center);
  const size = COVER * p.scale;
  return {
    left: CENTER_X + p.x * FLOW_SCALE - size / 2,
    top: CENTER_Y - size / 2,
    size,
    rotateY: p.rotateY,
    z: p.z * FLOW_SCALE,
    zIndex: 10 - Math.abs(i - center),
    opacity: p.opacity,
  };
}

/**
 * Albums per artist, asked for once each: an empty or failed answer is kept too,
 * so an artist with none is not asked again on every song. `albumsFor` resolves
 * to null when a newer artist was asked for meanwhile.
 */
export class AlbumSource {
  private readonly cache = new Map<string, PlayerItem[]>();
  private readonly gate = new QueryGate();
  private readonly fetch: (artist: string) => Promise<PlayerItem[]>;
  constructor(fetch: (artist: string) => Promise<PlayerItem[]>) {
    this.fetch = fetch;
  }

  async albumsFor(artist: string): Promise<PlayerItem[] | null> {
    const seq = this.gate.next();
    const known = this.cache.get(artist);
    if (known) return known;
    let albums: PlayerItem[];
    try {
      albums = await this.fetch(artist);
    } catch {
      albums = [];
    }
    this.cache.set(artist, albums);
    return this.gate.isCurrent(seq) ? albums : null;
  }
}

/** The bridge calls the album lookup uses (injectable for the tests). */
export interface AlbumLookup {
  playerSearch: typeof Bridge.playerSearch;
  playerArtist: typeof Bridge.playerArtist;
}

/**
 * The app's way to find an artist's albums. A search of both sources first: its
 * albums are Spotify's (YT Music's search returns only songs and artists). With
 * none, the artist's page on YT Music: the search's YT Music artist (the one whose
 * name matches, else the first), or one more search on YT Music alone, then
 * `playerArtist("ytmusic", id, name)`. ("both" is never asked for an artist page:
 * Rust refuses it, an artist belongs to one source.)
 */
export async function fetchArtistAlbums(artist: string, bridge: AlbumLookup = Bridge): Promise<PlayerItem[]> {
  const found = await bridge.playerSearch("both", artist, 0, 0);
  const albums = found?.items.filter((it) => it.kind === "album") ?? [];
  if (albums.length > 0) return albums;
  let yt = ytArtist(found?.items ?? [], artist);
  if (!yt) {
    const again = await bridge.playerSearch("ytmusic", artist, 0, 0);
    yt = ytArtist(again?.items ?? [], artist);
  }
  if (!yt) return [];
  const page = await bridge.playerArtist("ytmusic", yt.id, yt.title || artist);
  return page.ok ? page.value.albums : [];
}

/** A YT Music artist among search results: the one named `name`, else the first. */
export function ytArtist(items: PlayerItem[], name: string): PlayerItem | null {
  const artists = items.filter((it) => it.kind === "artist" && it.source === "ytmusic" && it.id);
  const want = name.trim().toLowerCase();
  return artists.find((a) => a.title.trim().toLowerCase() === want) ?? artists[0] ?? null;
}

export interface FlowView {
  root: HTMLElement;
  /** The artist to show (flowArtist); asks for the albums only when it changed. */
  update(artist: string | null): Promise<void>;
  relabel(): void;
}

export function buildFlow(container: HTMLElement, source: AlbumSource, onPickAlbum?: (album: PlayerItem) => void): FlowView {
  container.className = "dp-flow-container";
  container.replaceChildren();

  const stage = document.createElement("div");
  stage.className = "dp-flow-stage";

  const caption = document.createElement("div");
  caption.className = "lg dp-flow-caption";
  caption.style.display = "none";
  const capTitle = document.createElement("span");
  capTitle.className = "dp-flow-title";
  const capSub = document.createElement("span");
  capSub.className = "dp-flow-sub";
  caption.append(capTitle, capSub);

  container.append(stage, caption);

  /** undefined until the first update, so a first "no artist" still paints. */
  let activeArtist: string | null | undefined = undefined;
  let albums: PlayerItem[] = [];
  let centerIndex = 0;
  const artCache = new Map<string, string>();

  function renderCovers() {
    stage.replaceChildren();
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
      cover.title = item.title;
      cover.style.position = "absolute";
      cover.style.left = `${tf.left}px`;
      cover.style.top = `${tf.top}px`;
      cover.style.width = `${tf.size}px`;
      cover.style.height = `${tf.size}px`;
      cover.style.transform = `translateZ(${tf.z}px) rotateY(${tf.rotateY}deg)`;
      cover.style.zIndex = String(tf.zIndex);
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

      stage.append(cover);
    });

    const curr = albums[centerIndex];
    caption.style.display = "flex";
    capTitle.textContent = curr.title;
    capTitle.title = curr.title;
    capSub.textContent = `${curr.subtitle || activeArtist || ""} · ${t("Album")}`;
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

  async function update(artist: string | null) {
    if (artist === activeArtist) return;
    activeArtist = artist;
    if (!artist) {
      albums = [];
      renderCovers();
      return;
    }
    const found = await source.albumsFor(artist);
    if (found === null || activeArtist !== artist) return; // another artist meanwhile
    albums = found;
    centerIndex = albums.length > 0 ? Math.floor(albums.length / 2) : 0;
    renderCovers();
  }

  return {
    root: container,
    update,
    relabel: renderCovers,
  };
}
