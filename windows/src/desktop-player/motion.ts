// The desktop player's small motions (desktop.css, "Motion"). Everything here is a state-triggered
// transition or a one-shot CSS animation of opacity and transform on a small element: nothing loops
// (the equalizer's bars are the one exception, and CSS pauses them), so the page stays at 0 % CPU
// when nothing changes. WebKitGTK repaints every animated frame on the CPU (no compositing).

/** Gap between two staggered items, ms. */
export const STAGGER_STEP = 35;
/** After this many items the rest arrive together: a long list must not take seconds to appear. */
export const STAGGER_CAP = 8;

/** The delay of item `i` in a staggered entrance. */
export function staggerDelay(i: number, step = STAGGER_STEP, cap = STAGGER_CAP): number {
  return Math.min(Math.max(0, Math.floor(i)), cap) * step;
}

/** What identifies a song for the player bar and the "now" card (a new key plays the song-change motion). */
export function trackKey(t: { title?: string; artists?: string[] } | null | undefined): string {
  if (!t || !t.title) return "";
  return `${t.title}\u0001${(t.artists ?? []).join(",")}`;
}

/** Whether going from the song `prev` to `next` is a change worth a motion (never the first song shown). */
export function songChanged(prev: string, next: string): boolean {
  return prev !== "" && next !== "" && prev !== next;
}

/** Which of `keys` are not in `seen`: those rows slide in, the ones already there stay still. */
export function freshFlags(seen: ReadonlySet<string>, keys: string[]): boolean[] {
  return keys.map((k) => !seen.has(k));
}

/** Plays the one-shot animation `cls` again (remove, reflow, add). */
export function replay(el: Element | null | undefined, cls: string): void {
  if (!el) return;
  el.classList.remove(cls);
  void (el as HTMLElement).offsetWidth;
  el.classList.add(cls);
}

export interface Motion {
  /** The panels are built: the page's entrance plays (once; again each time the page is shown after being hidden). */
  enter(): void;
}

/**
 * Sets up the page-level motion on `root` (#dp): the entrance (hidden until `enter`, with a fallback so a
 * failed boot never leaves the page blank) and `dp-hidden` while the page is hidden, which pauses the
 * equalizer.
 */
export function initMotion(root: HTMLElement, doc: Document = document): Motion {
  root.classList.add("dp-pre");
  let entered = false;
  const syncHidden = () => root.classList.toggle("dp-hidden", doc.hidden);
  syncHidden();

  const enter = () => {
    entered = true;
    root.classList.remove("dp-pre");
    replay(root, "dp-enter");
  };
  const fallback = setTimeout(() => {
    if (!entered) enter();
  }, 4000);

  doc.addEventListener("visibilitychange", () => {
    syncHidden();
    // shown again after being hidden: the panels come in once more
    if (!doc.hidden && entered) replay(root, "dp-enter");
  });

  return {
    enter() {
      clearTimeout(fallback);
      if (!entered) enter();
    },
  };
}
