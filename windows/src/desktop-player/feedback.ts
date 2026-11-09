// What the desktop player says back, the way the notch's player foot does
// (views/player.ts `start` / `report` / `paintFoot`, whose pure parts it shares):
// "Starting…" while a play or the DJ is on its way (one at a time), why it
// failed, "Played through the Spotify app", each source's search error
// ("Spotify: …"), and one "Connect Spotify" button when a sign-in is wanted.
//
// One line under the results, above the search field; search, recents, the
// album list and the bar's DJ all report through it. Text only (textContent):
// the messages come from Rust and the services.

import { Bridge } from "../core/bridge";
import type { PlayerFailure, PlayerOutcome, PlayerSource, PlayerSourceName, PlayResult, SourceError } from "../core/player";
import { footContent, playReport, type PlayerNotice } from "../views/player";
import { t } from "../i18n/i18n";

/** How long a notice stays (the notch's `say`). */
export const NOTICE_MS = 4000;

/** What the search contributes to the line: its query, its per-source errors, its source. */
export interface SearchSide {
  query(): string;
  errors(): SourceError[];
  source(): PlayerSource;
  /** Signed in to Spotify: ask again. */
  rerun(): void;
}

export class Feedback {
  readonly el: HTMLElement;
  notice: PlayerNotice | null = null;
  /** Something the user did failed for want of a Spotify sign-in. */
  loginNeeded = false;
  /** Spotify said it has no session (checked when the search is opened). */
  spotifyOff = false;
  /** A play or the DJ is on its way: further presses wait for it. */
  starting = false;
  side: SearchSide | null = null;
  private readonly now: () => number;

  constructor(now: () => number = () => Date.now()) {
    this.now = now;
    this.el = document.createElement("div");
    this.el.className = "lg dp-feedback";
    this.el.setAttribute("role", "status");
  }

  /** The line's content, as the notch's foot would show it. */
  content() {
    const side = this.side;
    return footContent({
      notice: this.notice,
      now: this.now(),
      query: side?.query() ?? "",
      errors: side?.errors() ?? [],
      loadingMore: false,
      loginNeeded: this.loginNeeded,
      spotifyOff: this.spotifyOff,
      source: side?.source() ?? "both",
    });
  }

  /** A play or the DJ: one at a time, "Starting…" meanwhile, then what happened. */
  async start(run: () => Promise<PlayerOutcome<PlayResult>>, failText: string, source: PlayerSourceName) {
    if (this.starting) {
      this.paint(); // the press is ignored: the first one is still starting
      return;
    }
    this.starting = true;
    this.notice = { text: t("Starting…"), tone: "", until: Infinity };
    this.paint();
    try {
      const res = playReport(await run(), failText);
      this.notice = null;
      if (!res.ok) {
        if (res.loginNeeded) this.loginNeeded = true;
        this.say(res.text, "err");
        return;
      }
      this.loginNeeded = false;
      if (source === "spotify") this.spotifyOff = false;
      if (res.text) this.say(res.text);
      else this.paint();
    } finally {
      this.starting = false;
    }
  }

  /** Says why something failed, and offers to sign in to Spotify when that is why. */
  report(e: PlayerFailure, fallback: string) {
    if (e.loginNeeded) this.loginNeeded = true;
    this.say(e.message || fallback, "err");
  }

  say(text: string, tone: "" | "err" = "") {
    this.notice = { text, tone, until: this.now() + NOTICE_MS };
    this.paint();
    window.setTimeout(() => this.paint(), NOTICE_MS + 100);
  }

  /** Whether Spotify has a session, so "Connect Spotify" can be offered before anything fails. */
  async checkSpotify() {
    const on = await Bridge.playerSpotifyConnected();
    if (on === null) return; // not known (outside the app)
    this.spotifyOff = !on;
    this.paint();
  }

  async connect() {
    const err = await Bridge.playerSpotifyLogin();
    if (err) {
      this.say(err, "err");
      return;
    }
    this.loginNeeded = false;
    this.spotifyOff = false;
    this.paint();
    if (this.side?.query()) this.side.rerun();
  }

  paint() {
    const { lines, offerLogin } = this.content();
    const el = this.el;
    el.replaceChildren();
    for (const l of lines) {
      const s = document.createElement("span");
      s.className = l.err ? "dp-msg err" : "dp-msg";
      s.textContent = l.text;
      el.append(s);
    }
    if (offerLogin) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "dp-search-login-btn";
      b.textContent = t("Connect Spotify");
      b.addEventListener("mousedown", (e) => e.preventDefault());
      b.addEventListener("click", () => void this.connect());
      el.append(b);
    }
    el.classList.toggle("on", lines.length > 0 || offerLogin);
  }
}
