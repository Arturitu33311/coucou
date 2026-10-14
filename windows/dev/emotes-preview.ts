// Dev harness: every emote on a Mochi in a plain browser. Each one fires its one-shot animation
// every few seconds (with the eyes it brings); ?hold=1 keeps the eyes as a permanent emote, which is
// what a mini bot does. ?outfit=wizardHat puts an outfit on all of them. Not part of the app bundle.

import { BotEngine } from "../src/mochi/engine";
import type { BotEmoteName } from "../src/core/layout";
import type { Outfit } from "../src/mochi/outfits";

const EMOTES: BotEmoteName[] = [
  "love", "surprised", "proud", "wink", "yawn", "happy", "annoyed", "dance",
  "sleepy", "confused", "excited", "shy", "sad",
  "curious", "bored", "scared", "grumpy", "smug",
  "laughing", "crying", "relieved", "determined", "flustered",
];

const grid = document.getElementById("grid")!;
const SIZE = 150;
const dpr = window.devicePixelRatio || 1;
const q = new URLSearchParams(location.search);
const hold = q.has("hold");
const outfit = (q.get("outfit") as Outfit | null) ?? null;
const cells: { engine: BotEngine; ctx: CanvasRenderingContext2D; emote: BotEmoteName }[] = [];

for (const emote of EMOTES) {
  const canvas = document.createElement("canvas");
  canvas.width = SIZE * dpr;
  canvas.height = (SIZE + 40) * dpr;
  canvas.style.width = `${SIZE}px`;
  canvas.style.height = `${SIZE + 40}px`;
  const fig = document.createElement("figure");
  const cap = document.createElement("figcaption");
  cap.textContent = emote;
  fig.append(canvas, cap);
  grid.append(fig);
  const engine = new BotEngine();
  engine.particleOverhang = 40;
  engine.setState("idle", true);
  engine.setOutfit(outfit, false);
  if (hold) engine.setPermanentEmote(emote);
  cells.push({ engine, ctx: canvas.getContext("2d")!, emote });
}

let last = performance.now();
let nextFire = performance.now() + 300;
function loop(now: number) {
  const dt = Math.min(0.05, (now - last) / 1000);
  last = now;
  if (!hold && now > nextFire) {
    nextFire = now + 3500;
    for (const c of cells) c.engine.triggerEmote(c.emote, 3);
  }
  for (const c of cells) {
    c.engine.update(dt);
    c.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.ctx.clearRect(0, 0, SIZE, SIZE + 40);
    c.engine.draw(c.ctx, SIZE, SIZE + 40);
  }
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
