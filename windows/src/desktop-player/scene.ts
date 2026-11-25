// The desktop player's scene colours: the cover's while a track has one, the
// hour's otherwise. The hour's are linux-wallpaper/wallpaper.html's (the same
// keys, the same mix towards its teal base), so the desktop keeps the colours
// the wallpaper had; the cover's go through the island's own `musicTint` and
// `readableAccent`, so the desktop and the notch wear the same colour.

import { musicTint, readableAccent } from "../island/music";
import type { MusicInfo } from "../core/state";

export type RGB = [number, number, number];

/** The wallpaper's hour keys and its two tints (dawn teal-blue, noon teal, evening violet-teal, night blue). */
export const KEYS = [0, 5, 8, 13, 18, 21, 24];
export const TINT_A: RGB[] = [[14, 40, 78], [40, 120, 150], [40, 170, 160], [30, 170, 140], [110, 80, 170], [30, 60, 110], [14, 40, 78]];
export const TINT_B: RGB[] = [[10, 30, 60], [90, 120, 170], [60, 190, 170], [20, 130, 120], [60, 130, 160], [20, 50, 90], [10, 30, 60]];
const BASE_A: RGB = [20, 120, 110];
const BASE_B: RGB = [30, 90, 70];
const WHITE: RGB = [255, 255, 255];

const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
export const mix = (a: RGB, b: RGB, k: number): RGB => [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];

function hsv2rgb(h: number, s: number, v: number): RGB {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return [f(5) * 255, f(3) * 255, f(1) * 255];
}

function rgb2hsv(c: RGB): RGB {
  const r = c[0] / 255, g = c[1] / 255, b = c[2] / 255;
  const mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn;
  let h = 0;
  if (d) h = mx === r ? ((g - b) / d) % 6 : mx === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [(h * 60 + 360) % 360, mx ? d / mx : 0, mx];
}

/** The wallpaper's `saturate`: as colourful as the colour is, a touch more, and bright enough to glow. */
export function saturate(c: RGB): RGB {
  const [h, s, v] = rgb2hsv(c);
  return hsv2rgb(h, clamp(s * 1.35, 0, 1), clamp(v * 1.15, 0.65, 1));
}

/** The wallpaper's `hourTint`, at a given hour of the day (0–24, fractional). */
export function hourTint(set: RGB[], hour: number): RGB {
  const hr = clamp(hour, 0, 24);
  let i = 0;
  while (i < KEYS.length - 2 && hr >= KEYS[i + 1]) i++;
  return mix(set[i], set[i + 1], clamp((hr - KEYS[i]) / (KEYS[i + 1] - KEYS[i]), 0, 1));
}

export const css = (c: RGB, a = 1) => `rgba(${c.map((v) => Math.round(clamp(v, 0, 255))).join(",")},${a})`;

function hex(c: RGB): string {
  return `#${c.map((v) => Math.round(clamp(v, 0, 255)).toString(16).padStart(2, "0")).join("")}`;
}

/** `#rrggbb` → rgba(…, a), as the mockup's `hex(h, a)`. */
function alpha(color: string, a: number): string {
  const v = parseInt(color.replace("#", ""), 16);
  return `rgba(${(v >> 16) & 255},${(v >> 8) & 255},${v & 255},${a})`;
}

export interface SceneColors {
  /** The two lights of the scene (top-left and bottom-right). */
  a: RGB;
  b: RGB;
  /** The edge light, the gauge's ring, the progress: `#rrggbb`. */
  accent: string;
  /** The accent at 0.55 and 0.22, for the edge glow and the scene's halo (mockup `accentGlow` / `accentSoft`). */
  glow: string;
  soft: string;
  /** The clock's colour: the wallpaper's `tint()`. */
  clock: RGB;
  /** The cover's tint (`musicTint().bg`) laid over the blurred cover, or null with no cover. */
  wash: string | null;
}

/**
 * The scene for a cover colour (`MusicInfo.rgb`, the island's average of the art) or, with none, for the hour.
 * With a cover the hour plays no part: mixing the two gives mud (the wallpaper's own rule).
 */
export function sceneColors(tint: RGB | null, hour: number): SceneColors {
  if (tint) {
    const a = saturate(tint);
    const b = saturate(mix(tint, [0, 0, 0], 0.35));
    const accent = readableAccent(tint);
    return {
      a,
      b,
      accent,
      glow: alpha(accent, 0.55),
      soft: alpha(accent, 0.22),
      clock: mix(a, WHITE, 0.25),
      wash: musicTint({ rgb: tint } as MusicInfo)?.bg ?? null,
    };
  }
  const a = mix(BASE_A, hourTint(TINT_A, hour), 0.9);
  const b = mix(BASE_B, hourTint(TINT_B, hour), 0.9);
  const clock = mix(saturate(a), WHITE, 0.25);
  const accent = hex(clock);
  return { a, b, accent, glow: alpha(accent, 0.55), soft: alpha(accent, 0.22), clock, wash: null };
}

/** The hour of `d` as a fraction (13:30 → 13.5), what `sceneColors` takes. */
export function hourOf(d: Date): number {
  return d.getHours() + d.getMinutes() / 60;
}
