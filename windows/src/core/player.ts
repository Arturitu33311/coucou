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
