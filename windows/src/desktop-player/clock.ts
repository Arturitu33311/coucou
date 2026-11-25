// The desktop player's clock: linux-wallpaper/wallpaper.html's clock (its
// `ensureClock`), on its own canvas and without the car. The same 12 h text, the
// same condensed bold face stretched 1.9× in height, the same gradient fill with
// a light stroke, and the same thin outline "echo" a few pixels behind it. Only
// the size differs: the wallpaper sized it by the screen's width, the desktop
// player sizes it to the mockup's slot (top left, above "Continue listening").

import type { RGB } from "./scene";
import { css, mix } from "./scene";

/** The wallpaper's clock face. */
export const CLOCK_FACE = '"Oswald","DejaVu Sans Condensed","Arial Narrow","Liberation Sans Narrow",sans-serif';
/** Vertical stretch of the digits (the wallpaper's `sy`). */
const SY = 1.9;
const PAD = 12;
/** Where the echo sits, behind and below the digits (the wallpaper's classic layout, without its sway). */
const ECHO_DX = -5;
const ECHO_DY = 6;

/** The time as the clock shows it: 12 h, no leading zero on the hour ("7:05", "12:00"). */
export function clockText(d: Date): string {
  const h12 = d.getHours() % 12 || 12;
  return `${h12}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** Milliseconds until the next minute starts, when the clock next changes. */
export function msToNextMinute(d: Date): number {
  return 60_000 - (d.getSeconds() * 1000 + d.getMilliseconds());
}

/**
 * Paints `text` on `canvas`: digits `glyphH` stage pixels tall in `col`, at `ratio` device pixels per stage pixel.
 * Does nothing when nothing changed (the wallpaper's `clockKey`).
 */
export class Clock {
  private key = "";
  private readonly canvas: HTMLCanvasElement;
  private readonly glyphH: number;

  constructor(canvas: HTMLCanvasElement, glyphH: number) {
    this.canvas = canvas;
    this.glyphH = glyphH;
  }

  draw(text: string, col: RGB, ratio: number) {
    const key = `${text}|${col.map((v) => v | 0).join(",")}|${ratio.toFixed(3)}`;
    if (key === this.key) return;
    const probe = document.createElement("canvas").getContext("2d");
    const g = this.canvas.getContext("2d");
    if (!probe || !g) return;
    this.key = key;

    // the size that makes the digits glyphH tall once stretched
    probe.font = `bold 100px ${CLOCK_FACE}`;
    const asc100 = probe.measureText("0").actualBoundingBoxAscent || 72;
    const size = (100 * this.glyphH) / (asc100 * SY);
    probe.font = `bold ${size}px ${CLOCK_FACE}`;
    const m = probe.measureText(text);
    const asc = m.actualBoundingBoxAscent * SY;
    const bw = Math.ceil(m.width + PAD * 2);
    const bh = Math.ceil(asc + PAD * 2 + 4);
    const w = bw + Math.abs(ECHO_DX);
    const h = bh + ECHO_DY;

    this.canvas.width = Math.ceil(w * ratio);
    this.canvas.height = Math.ceil(h * ratio);
    this.canvas.style.width = `${w}px`;
    this.canvas.style.height = `${h}px`;
    g.setTransform(ratio, 0, 0, ratio, 0, 0);
    g.clearRect(0, 0, w, h);

    const paint = (x: number, y: number, fill: boolean) => {
      g.save();
      g.translate(x + PAD, y + PAD + asc);
      g.scale(1, SY);
      g.font = `bold ${size}px ${CLOCK_FACE}`;
      g.lineJoin = "round";
      if (fill) {
        const gr = g.createLinearGradient(0, -asc / SY, 0, 0);
        gr.addColorStop(0, css(mix(col, [255, 255, 255], 0.75), 0.95));
        gr.addColorStop(0.6, css(col, 0.62));
        gr.addColorStop(1, css(col, 0.3));
        g.fillStyle = gr;
        g.fillText(text, 0, 0);
        g.lineWidth = 3.4 / SY;
        g.strokeStyle = css(mix(col, [255, 255, 255], 0.25), 0.96);
        g.strokeText(text, 0, 0);
      } else {
        g.lineWidth = 1.8 / SY;
        g.strokeStyle = css(col, 0.32);
        g.strokeText(text, 0, 0);
      }
      g.restore();
    };
    // the echo first, behind; then the digits
    paint(0, ECHO_DY, false);
    paint(-ECHO_DX, 0, true);
  }

  /** Forget the last drawing, so the next `draw` paints again (fonts loaded late). */
  invalidate() {
    this.key = "";
  }
}
