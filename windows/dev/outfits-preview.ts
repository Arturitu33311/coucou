// Dev harness: every outfit on a Mochi in a plain browser. Each one looks around (so the 3-D
// shows), and every few seconds rolls once. Not part of the app bundle.
// ?yaw=0.6 freezes the look direction (for screenshots); ?roll=1 starts a roll at once.

import { BotEngine } from "../src/mochi/engine";
import { OUTFIT_CHOICES, type Outfit } from "../src/mochi/outfits";
import type { BotStateName } from "../src/core/layout";

const grid = document.getElementById("grid")!;
const SIZE = 150;
const dpr = window.devicePixelRatio || 1;
const q = new URLSearchParams(location.search);
const fixedYaw = q.has("yaw") ? Number(q.get("yaw")) : null;
const cells: { engine: BotEngine; ctx: CanvasRenderingContext2D; phase: number }[] = [];

const rows: [BotStateName, Outfit | null][] = [["idle", null]];
for (const c of OUTFIT_CHOICES) {
  if (c.value === "auto" || c.value === "none") continue;
  rows.push([c.value === "nightcap" ? "sleeping" : "idle", c.value]);
}

for (const [state, outfit] of rows) {
  const canvas = document.createElement("canvas");
  canvas.width = SIZE * dpr;
  canvas.height = (SIZE + 40) * dpr;
  canvas.style.width = `${SIZE}px`;
  canvas.style.height = `${SIZE + 40}px`;
  const fig = document.createElement("figure");
  const cap = document.createElement("figcaption");
  cap.textContent = `${outfit ?? "bare"} · ${state}`;
  fig.append(canvas, cap);
  grid.append(fig);
  const engine = new BotEngine();
  engine.particleOverhang = 40;
  engine.setState(state, true);
  engine.setOutfit(outfit, false);
  cells.push({ engine, ctx: canvas.getContext("2d")!, phase: cells.length * 0.7 });
}

let last = performance.now();
let nextRoll = performance.now() + (q.has("roll") ? 100 : 6000);
function loop(now: number) {
  const dt = Math.min(0.05, (now - last) / 1000);
  last = now;
  const sec = now / 1000;
  if (now > nextRoll) {
    nextRoll = now + 7000;
    for (const c of cells) c.engine.doRoll(1100, 1);
  }
  for (const c of cells) {
    c.engine.lookX = fixedYaw ?? Math.sin(sec * 0.9 + c.phase) * 1.2;
    c.engine.lookY = 0;
    c.engine.update(dt);
    c.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.ctx.clearRect(0, 0, SIZE, SIZE + 40);
    c.engine.draw(c.ctx, SIZE, SIZE + 40);
  }
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
