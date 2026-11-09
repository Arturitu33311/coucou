// The desktop page's small rules, kept apart from the page (main.ts builds the
// DOM at load) so tests/desktop-player.test.mjs can check them.

import type { PlayerItem, PlayerSourceName } from "../core/player";

/** A source's name as the player shows it. */
export function sourceName(source: PlayerSourceName): string {
  return source === "spotify" ? "Spotify" : "YT Music";
}

function sameTitle(a: string, b: string): boolean {
  return a.trim().toLowerCase() === b.trim().toLowerCase();
}

/**
 * The song the player itself started, when it is the one playing: the newest
 * track in the history (`player_recents`, newest first) with the same title.
 * Its cover and its source stand in for what MPRIS lacks (mpv has neither).
 */
export function playedMatch(title: string | null | undefined, recents: PlayerItem[]): PlayerItem | null {
  if (!title || !title.trim()) return null;
  return recents.find((r) => r.kind === "track" && sameTitle(r.title, title)) ?? null;
}

/**
 * The source shown beside the artist: the matched song's ("YT Music" / "Spotify"),
 * else a friendly name for a known MPRIS player: Spotify's app or spotifyd →
 * "Spotify"; mpv → "YT Music" when the player's last play was on YT Music (that
 * mpv is the player's own); anything else as MPRIS names it ("" → "Coucou").
 */
export function sourceLabel(player: string, match: PlayerItem | null, recents: PlayerItem[]): string {
  if (match) return sourceName(match.source);
  const bus = player.trim().toLowerCase().split(".")[0];
  if (bus === "spotify" || bus === "spotifyd") return "Spotify";
  if (bus === "mpv") {
    const last = recents.find((r) => r.kind === "track" || r.kind === "album");
    if (last?.source === "ytmusic") return "YT Music";
  }
  return player || "Coucou";
}

/**
 * Whether the history should be read once more for this song: the player may
 * write its entry only after the song started (mpv's settle), so the first read
 * can miss it. Once per song, only for a song with no cover of its own (mpv's)
 * that is not matched yet: a Spotify song from its app never asks again.
 */
export function rereadWanted(match: PlayerItem | null, alreadyRetried: boolean, missingArt: boolean): boolean {
  return missingArt && !match && !alreadyRetried;
}

/** The progress bar's timer runs only while a song plays and the page is seen. */
export function progressTimerWanted(status: string | null | undefined, hidden: boolean): boolean {
  return status === "Playing" && !hidden;
}

/** The line being sung shows when there is one and the lyrics button is on. */
export function lyricShown(text: string, enabled: boolean): boolean {
  return enabled && text !== "";
}

/**
 * The recents and the cover flow are read once per song, not on every `music`
 * event (play state, seek) or cover load: `changed` says whether `key` is a new
 * song ("" = nothing playing). The first call always is.
 */
export class SongLists {
  private key: string | null = null;
  changed(key: string): boolean {
    if (key === this.key) return false;
    this.key = key;
    return true;
  }
}
