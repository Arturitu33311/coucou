// The glass player's shapes, as the Rust `player_*` commands send them
// (src-tauri/src/player, camelCase). The commands themselves are in bridge.ts.

export type PlayerSourceName = "spotify" | "ytmusic";

/** What the selector searches: one source or both at once. */
export type PlayerSource = PlayerSourceName | "both";

export interface PlayerItem {
  source: PlayerSourceName;
  /** "context" is a DJ or a list. */
  kind: "track" | "artist" | "album" | "context";
  id: string;
  title: string;
  /** The artist(s) for a track or an album; empty for an artist. */
  subtitle: string;
  /** A web URL, or "" when there is none. */
  artUrl: string;
  /** A Spotify track's album ("spotify:album:…"): playing the track carries on with it. */
  context?: string;
}

/** One source failed; the other's results are still there. */
export interface SourceError {
  source: PlayerSourceName;
  message: string;
  /** Spotify needs the user to sign in (again). */
  loginNeeded: boolean;
}

export interface SearchResult {
  /** The `seq` the search was sent with, to drop answers to older queries. */
  seq: number;
  items: PlayerItem[];
  errors: SourceError[];
}

export interface ArtistPage {
  tracks: PlayerItem[];
  albums: PlayerItem[];
}

export interface PlayResult {
  /** Played through the official Spotify app because the headless device was not there. */
  fallbackUsed: boolean;
}

/** Why a play, the DJ or an artist/album load failed (Rust's `PlayerError`). */
export interface PlayerFailure {
  /** Already translated; "" when nothing better than the view's own words is known. */
  message: string;
  /** Spotify needs the user to sign in (again). */
  loginNeeded: boolean;
}

export type PlayerOutcome<T> = { ok: true; value: T } | { ok: false; error: PlayerFailure };

/** Reads what a failed `player_*` command threw: Rust's `{ message, loginNeeded }`, or a bare string. */
export function playerFailure(err: unknown): PlayerFailure {
  if (err && typeof err === "object") {
    const o = err as { message?: unknown; loginNeeded?: unknown };
    return { message: typeof o.message === "string" ? o.message : "", loginNeeded: o.loginNeeded === true };
  }
  return { message: typeof err === "string" ? err : "", loginNeeded: false };
}
