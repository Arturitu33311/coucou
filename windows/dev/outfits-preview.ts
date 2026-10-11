// Dev harness: every outfit on a Mochi in a plain browser, in a few states. Not part of the app bundle.

import { BotEngine } from "../src/mochi/engine";
import { OUTFIT_CHOICES, type Outfit } from "../src/mochi/outfits";
import type { BotStateName } from "../src/core/layout";

const grid = document.getElementById("grid")!;
const SIZE = 160;
const dpr = window.devicePixelRatio || 1;
const cells: { engine: BotEngine; ctx: CanvasRenderingContext2D }[] = [];

const rows: [BotStateName, Outfit | null][] = [["idle", null]];
for (const c of OUTFIT_CHOICES) {
  if (c.value === "auto" || c.value === "none") continue;
  rows.push([c.value === "nightcap" ? "sleeping" : "idle", c.value]);
  rows.push(["working", c.value]);
}

for (const [state, outfit] of rows) {
  const canvas = document.createElement("canvas");
  canvas.width = SIZE * dpr;
  canvas.height = SIZE * dpr;
  canvas.style.width = `${SIZE}px`;
  canvas.style.height = `${SIZE}px`;
  const fig = document.createElement("figure");
  const cap = document.createElement("figcaption");
  cap.textContent = `${outfit ?? "bare"} · ${state}`;
  fig.append(canvas, cap);
  grid.append(fig);
  const engine = new BotEngine();
  engine.setState(state, true);
  engine.outfit = outfit;
  cells.push({ engine, ctx: canvas.getContext("2d")! });
}

let last = performance.now();
function loop(now: number) {
  const dt = Math.min(0.05, (now - last) / 1000);
  last = now;
  for (const { engine, ctx } of cells) {
    engine.update(dt);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, SIZE, SIZE);
    engine.draw(ctx, SIZE, SIZE);
  }
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
