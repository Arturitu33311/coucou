// The wardrobe view's buttons (src/views/wardrobe.ts, from upstream): a small Mochi wearing each
// outfit, drawn by this fork's own engine and wardrobe so the icons match the island exactly.

import { SCRIPT_FONTS } from "../core/fonts";
import { BotEngine } from "./engine";
import type { OutfitSelection, WornOutfit } from "./outfits";

type Ctx = CanvasRenderingContext2D;

/** One engine for every icon: it is only ever drawn still. */
let painter: BotEngine | null = null;

function iconMochi(ctx: Ctx, size: number, outfit: WornOutfit) {
  painter ??= new BotEngine();
  painter.setOutfit(outfit, false);
  ctx.save();
  // The engine draws Mochi with radius 0.3·W; the icon wants him a little larger than the button.
  const scale = 1.25;
  ctx.translate(size / 2, size / 2);
  ctx.scale(scale, scale);
  ctx.translate(-size / 2, -size / 2 + size * 0.04);
  painter.draw(ctx, size, size);
  ctx.restore();
}

function roundRect(ctx: Ctx, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

/** The icon of one wardrobe button: "none" is a struck circle, "auto" the season's outfit with a badge. */
export function drawWardrobeIcon(ctx: Ctx, size: number, selection: OutfitSelection, seasonal: WornOutfit, autoLabel: string) {
  ctx.clearRect(0, 0, size, size);
  if (selection === "none") {
    const r = 6.5;
    ctx.save();
    ctx.translate(size / 2, size / 2);
    ctx.strokeStyle = "#454850";
    ctx.lineWidth = 1.4;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.arc(0, 0, r * 0.82, 0, Math.PI * 2);
    ctx.moveTo(-r * 0.56, r * 0.56);
    ctx.lineTo(r * 0.56, -r * 0.56);
    ctx.stroke();
    ctx.restore();
    return;
  }
  iconMochi(ctx, size, selection === "auto" ? seasonal : selection);
  if (selection !== "auto") return;
  const R = 10;
  const by = size / 2 + R * 0.62 + R * 0.88 * 0.72;
  const bw = 14;
  const bh = 6.5;
  ctx.save();
  ctx.translate(size / 2, by);
  roundRect(ctx, -bw / 2, -bh / 2, bw, bh, bh / 2);
  ctx.fillStyle = "rgba(0,0,0,0.6)";
  ctx.fill();
  ctx.fillStyle = "#FFFFFF";
  // A longer translation of "AUTO" gets a smaller font rather than overflowing the badge.
  let fontPx = 4.2;
  ctx.font = `600 ${fontPx}px system-ui, "Segoe UI", ${SCRIPT_FONTS}, sans-serif`;
  while (ctx.measureText(autoLabel).width > bw - 2 && fontPx > 2.6) {
    fontPx -= 0.2;
    ctx.font = `600 ${fontPx}px system-ui, "Segoe UI", ${SCRIPT_FONTS}, sans-serif`;
  }
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(autoLabel, 0, 0.2);
  ctx.restore();
}
