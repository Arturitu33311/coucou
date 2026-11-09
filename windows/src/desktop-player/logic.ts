// The desktop page's small rules, kept apart from the page (main.ts builds the
// DOM at load) so tests/desktop-player.test.mjs can check them.

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
