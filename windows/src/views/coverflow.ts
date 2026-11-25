// The artist page's cover flow (views/player.ts): the albums as a row of glass
// covers, the centre one flat and in front, its neighbours turned in towards it.
//
// Every cover is placed once per move by a CSS transform, and the movement itself
// is a CSS transition: nothing runs per frame, and nothing at all while it rests.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import type { PlayerItem } from "../core/player";

export interface CoverPlace {
  /** px from the centre, along the row. */
  x: number;
  scale: number;
  /** Degrees: the covers on the left turn right (+), the ones on the right turn left (−). */
  rotateY: number;
  /** px towards the viewer (translateZ): 0 for the centre, further back the further out. */
  z: number;
  opacity: number;
}

/** First neighbour's distance from the centre, then the step between the ones behind it. */
const NEAR_X = 78;
const STEP_X = 28;
const NEAR_Z = -70;
const STEP_Z = -30;
/** By distance from the centre: seen in full up to the neighbours, gone past 4. */
const FADE = [1, 1, 0.85, 0.6, 0.35];

/** Where cover `i` sits when cover `center` is in front (tests/player.test.mjs). */
export function coverflowTransform(i: number, center: number): CoverPlace {
  const d = i - center;
  const a = Math.abs(d);
  if (a === 0) return { x: 0, scale: 1, rotateY: 0, z: 0, opacity: 1 };
  const side = d < 0 ? -1 : 1;
  return {
    x: side * (NEAR_X + (a - 1) * STEP_X),
    scale: a === 1 ? 0.8 : 0.65,
    rotateY: side < 0 ? 45 : -45,
    z: NEAR_Z + (a - 1) * STEP_Z,
    opacity: FADE[a] ?? 0,
  };
}

export interface Coverflow {
  el: HTMLElement;
  /** Moves the centre by `delta` covers, held at both ends. */
  move(delta: number): void;
  /** The index in front, -1 when there are no covers. */
  center(): number;
}

/** Wheel travel (px) that makes one step. */
const WHEEL_STEP = 40;

/**
 * The row itself. The wheel and the left/right arrows move it; a click on a side
 * cover brings it to the front; a click or Enter on the front one calls `onPick`.
 * `art` turns a web cover URL into something an <img> may show (the page's CSP
 * keeps it from loading remote images itself).
 */
export function buildCoverflow(
  items: PlayerItem[],
  onPick: (it: PlayerItem) => void,
  art?: (url: string) => Promise<string | null>,
): Coverflow {
  let current = items.length > 0 ? 0 : -1;
  const caption = h("div", { class: "cf-caption" });
  const covers = items.map((item, i) => {
    const face = h("div", { class: "cf-face" }, svg(ICONS.note, 22));
    const cover = h("div", { class: "cf-cover", "data-idx": i, title: item.title }, face, h("i", { class: "cf-sheen" }));
    if (item.artUrl && art) {
      void art(item.artUrl).then((src) => {
        if (!src) return;
        clear(face);
        face.append(h("img", { src, alt: "", draggable: false }));
      });
    }
    cover.addEventListener("mousedown", (e) => e.preventDefault()); // the focus stays where it is
    cover.addEventListener("click", () => {
      if (i === current) onPick(item);
      else move(i - current);
    });
    return cover;
  });
  const stage = h("div", { class: "cf-stage" }, ...covers);
  const el = h("div", { class: "cf", tabindex: "0" }, stage, caption);

  function place() {
    for (let i = 0; i < covers.length; i++) {
      const p = coverflowTransform(i, current);
      const s = covers[i].style;
      s.transform = `translateX(${p.x}px) translateZ(${p.z}px) rotateY(${p.rotateY}deg) scale(${p.scale})`;
      s.opacity = String(p.opacity);
      // translateZ alone does not settle overlaps everywhere: the order says it too.
      s.zIndex = String(100 - Math.abs(i - current));
      s.pointerEvents = p.opacity > 0 ? "" : "none";
      covers[i].classList.toggle("front", i === current);
    }
    const it = items[current];
    clear(caption);
    if (it) {
      caption.append(h("div", { class: "cf-title", text: it.title }));
      if (it.subtitle) caption.append(h("div", { class: "cf-sub", text: it.subtitle }));
    }
  }

  function move(delta: number) {
    if (items.length === 0) return;
    const next = Math.max(0, Math.min(items.length - 1, current + Math.trunc(delta)));
    if (next === current) return;
    current = next;
    place();
  }

  el.addEventListener("keydown", (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      move(e.key === "ArrowLeft" ? -1 : 1);
    } else if (e.key === "Enter") {
      const it = items[current];
      if (!it) return;
      e.preventDefault();
      onPick(it);
    }
  });

  // A trackpad sends a stream of small deltas, a wheel a few large ones: both add up to steps.
  let travel = 0;
  el.addEventListener("wheel", (e: WheelEvent) => {
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (!delta) return;
    e.preventDefault();
    travel += delta;
    while (Math.abs(travel) >= WHEEL_STEP) {
      const step = travel > 0 ? 1 : -1;
      travel -= step * WHEEL_STEP;
      move(step);
    }
  });

  place();
  return { el, move, center: () => current };
}
