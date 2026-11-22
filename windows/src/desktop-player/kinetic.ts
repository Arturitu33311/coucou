// Full-screen lyrics (shown only while #dp is `immersive`, kinetic.css): the line being sung, word by word.
// Each new line rises in a word at a time; the words light up in the album's accent as the line goes by
// (karaoke-style, paced on the line's length); the line before drifts up and away, the next one waits
// dimly underneath. Only opacity and transforms move (WebKit runs here without GPU compositing), and every
// motion is a CSS animation with a delay: no timers. Paused song, paused animations.

/** The words of a line, with when each one enters and lights up (ms after the line starts). */
export interface WordPlan {
  text: string;
  enterMs: number;
  litMs: number;
}

/** How a line of `words` unfolds over `lineMs` (the time until the next line). */
export function planWords(line: string, lineMs: number): WordPlan[] {
  const words = line.split(/\s+/).filter(Boolean);
  const n = words.length;
  if (!n) return [];
  const span = Math.min(Math.max(lineMs, 800), 12_000);
  // they all arrive within the first part of the line (a long line does not crawl in)...
  const enterStep = Math.min(90, (span * 0.35) / n);
  // ...and light up across most of it, as it is sung
  const litStep = (span * 0.85) / n;
  return words.map((text, i) => ({ text, enterMs: Math.round(i * enterStep), litMs: Math.round(i * litStep) }));
}

export interface KineticView {
  root: HTMLElement;
  /** The line being sung now, the next one ("" for none) and how long until it (ms). */
  show(line: string, next: string, lineMs: number): void;
  setPlaying(playing: boolean): void;
  setOn(on: boolean): void;
}

/** How long the old line takes to leave (kinetic.css `k-out`). */
const OUT_MS = 700;

export function buildKinetic(): KineticView {
  const root = document.createElement("div");
  root.className = "dp-kinetic";
  root.setAttribute("aria-hidden", "true");
  const stage = document.createElement("div");
  stage.className = "k-stage";
  const nextEl = document.createElement("div");
  nextEl.className = "k-next";
  root.append(stage, nextEl);
  let current: HTMLElement | null = null;
  let shown = "";

  function show(line: string, next: string, lineMs: number) {
    if (line === shown) {
      if (nextEl.textContent !== next) nextEl.textContent = next;
      return;
    }
    shown = line;
    if (current) {
      const old = current;
      old.classList.add("out");
      window.setTimeout(() => old.remove(), OUT_MS);
    }
    current = null;
    if (line) {
      const el = document.createElement("div");
      el.className = "k-line";
      for (const w of planWords(line, lineMs)) {
        const span = document.createElement("span");
        span.className = "k-word";
        span.textContent = w.text;
        span.style.setProperty("--enter", `${w.enterMs}ms`);
        span.style.setProperty("--lit", `${w.litMs}ms`);
        el.append(span, " ");
      }
      stage.append(el);
      current = el;
    }
    nextEl.classList.remove("in");
    nextEl.textContent = next;
    // restart its fade-in (a reflow between removing and adding the class)
    void nextEl.offsetWidth;
    if (next) nextEl.classList.add("in");
  }

  return {
    root,
    show,
    setPlaying: (playing) => root.classList.toggle("paused", !playing),
    setOn: (on) => root.classList.toggle("on", on),
  };
}
