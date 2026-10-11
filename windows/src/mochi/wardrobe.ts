// Mochi's wardrobe, drawn in 3-D like the Mac app's: a port of
// NotchBuddy/Sources/CoucouKit/MochiOutfitDrawing.swift to Canvas 2D. The head is a superellipse
// "ball" (exponent 2.7); every accessory is built from points on its surface, projected with his
// yaw and pitch, so hats turn with his head, rims follow the skull and glasses sit on his eyes.
//
// Two passes, as on the Mac: `drawOutfitBehind` before the body (back of a crown or brim, bunny
// ears) and `drawOutfitFront` after his eyes. PC-only additions in the same style: the night cap
// (Santa's hat in indigo) and headphones.

import { Ease } from "../core/anim";
import type { Outfit } from "./outfits";

// ── Geometry ─────────────────────────────────────────────────────────────────

const EXP = 2.7;
const VIEW_TILT = -0.3;
const ACC_PITCH = 0.4;
const EYE_SP = 0.37;
const EYE_P = -0.12;

/** Head geometry and physics (MochiH). */
export interface MochiH {
  R: number;
  rx: number;
  ry: number;
  yaw: number;
  pitch: number;
  view: number;
  physDx: number;
  physDy: number;
  roll: number;
}

export function mochiH(R: number, yaw = 0, pitch = 0, physDx = 0, physDy = 0, roll = 0): MochiH {
  return { R, rx: R * 1.14, ry: R * 0.88, yaw, pitch, view: VIEW_TILT, physDx, physDy, roll };
}

interface P3 { x: number; y: number; z: number }
type V3 = [number, number, number];

interface EyeFrame { sd: number; x: number; y: number; fx: number; fy: number; visible: boolean }

function eyeFrames(H: MochiH): EyeFrame[] {
  return [-1, 1].map((sd) => {
    const eyeYaw = sd * EYE_SP + H.yaw;
    const eyePitch = EYE_P + H.pitch;
    const cp = Math.cos(eyePitch);
    return {
      sd,
      x: Math.sin(eyeYaw) * cp * H.rx,
      y: -Math.sin(eyePitch) * H.ry,
      fx: Math.max(0.18, Math.cos(eyeYaw)),
      fy: Math.max(0.18, cp),
      visible: Math.cos(eyeYaw) * cp > 0.04,
    };
  });
}

/** Radius of the horizontal ring at head-local height y. */
function ringR(y: number): number {
  const a = Math.min(1, Math.abs(y));
  return Math.pow(1 - Math.pow(a, EXP), 1 / EXP);
}

function rot3(p: V3, yaw: number, pitch: number): V3 {
  const [x, y, z] = p;
  const cy = Math.cos(yaw), sy = Math.sin(yaw);
  const x1 = x * cy + z * sy;
  const z1 = -x * sy + z * cy;
  const cp = Math.cos(pitch), sp = Math.sin(pitch);
  return [x1, y * cp + z1 * sp, -y * sp + z1 * cp];
}

function proj(H: MochiH, p: V3): P3 {
  const r = rot3(p, H.yaw, H.view + H.pitch * ACC_PITCH);
  return { x: r[0] * H.rx, y: -r[1] * H.ry, z: r[2] };
}

function projRoll(H: MochiH, p: V3): P3 {
  const r = rot3(p, H.yaw, H.view + H.pitch * ACC_PITCH + H.roll);
  return { x: r[0] * H.rx, y: -r[1] * H.ry, z: r[2] };
}

/** A point on the head's surface at height y and longitude lon (s scales the ring). */
function surf(y: number, lon: number, s = 1): V3 {
  const r = ringR(y) * s;
  return [r * Math.sin(lon), y, r * Math.cos(lon)];
}

/** The front half of a closed projected ring, ordered left → right (silhouette tangents). */
function frontSilhouetteArc(pts: P3[]): P3[] {
  const n = pts.length;
  if (n <= 1) return pts;
  let minI = 0, maxI = 0;
  for (let i = 1; i < n; i++) {
    if (pts[i].x < pts[minI].x) minI = i;
    if (pts[i].x > pts[maxI].x) maxI = i;
  }
  if (minI === maxI) return [pts[minI]];
  const arcA: P3[] = [];
  let i = minI;
  for (;;) {
    arcA.push(pts[i]);
    if (i === maxI) break;
    i = (i + 1) % n;
    if (arcA.length > n) break;
  }
  const arcB: P3[] = [];
  i = minI;
  for (;;) {
    arcB.push(pts[i]);
    if (i === maxI) break;
    i = (i - 1 + n) % n;
    if (arcB.length > n) break;
  }
  const zA = arcA.reduce((s, p) => s + p.z, 0) / Math.max(1, arcA.length);
  const zB = arcB.reduce((s, p) => s + p.z, 0) / Math.max(1, arcB.length);
  return zA >= zB ? arcA : arcB;
}

function ring(H: MochiH, y: number, s: number, project: (H: MochiH, p: V3) => P3, n = 120): P3[] {
  const out: P3[] = [];
  for (let i = 0; i < n; i++) {
    const lon = -Math.PI + (i / n) * 2 * Math.PI;
    out.push(project(H, surf(y, lon, s)));
  }
  return out;
}

const frontArc = (H: MochiH, y: number, s: number) => frontSilhouetteArc(ring(H, y, s, proj));
const frontArcRoll = (H: MochiH, y: number, s: number) => frontSilhouetteArc(ring(H, y, s, projRoll));

/** The region above the front arc of ring y: what a cap covers. */
function capClip(H: MochiH, y: number, s: number, extraTop = 3): Path2D {
  const arc = frontArc(H, y, s);
  const p = new Path2D();
  if (arc.length === 0) return p;
  p.moveTo(arc[0].x - H.rx, arc[0].y);
  for (const q of arc) p.lineTo(q.x, q.y);
  p.lineTo(arc[arc.length - 1].x + H.rx, arc[arc.length - 1].y);
  p.lineTo(H.rx * 2, -H.ry * extraTop);
  p.lineTo(-H.rx * 2, -H.ry * extraTop);
  p.closePath();
  return p;
}

/** Complement of `p` inside a large rectangle (clip it with "evenodd"). */
function invert(p: Path2D, H: MochiH): Path2D {
  const q = new Path2D();
  q.rect(-H.rx * 4, -H.ry * 4, H.rx * 8, H.ry * 8);
  q.addPath(p);
  return q;
}

/** The head outline, a clean superellipse (mochiOutfitPath). */
function outfitPath(rx: number, ry: number): Path2D {
  const n = 96;
  const e = 2 / EXP;
  const p = new Path2D();
  for (let i = 0; i <= n; i++) {
    const a = (i / n) * Math.PI * 2;
    const ca = Math.cos(a), sa = Math.sin(a);
    const x = rx * (ca >= 0 ? Math.pow(ca, e) : -Math.pow(-ca, e));
    const y = ry * (sa >= 0 ? Math.pow(sa, e) : -Math.pow(-sa, e));
    if (i === 0) p.moveTo(x, y);
    else p.lineTo(x, y);
  }
  p.closePath();
  return p;
}

// ── Paint helpers ────────────────────────────────────────────────────────────

type Stop = [number, string];
type Ctx = CanvasRenderingContext2D;

function lin(x: Ctx, x0: number, y0: number, x1: number, y1: number, stops: Stop[]): CanvasGradient {
  const g = x.createLinearGradient(x0, y0, x1, y1);
  for (const [o, c] of stops) g.addColorStop(o, c);
  return g;
}

function rad(x: Ctx, cx: number, cy: number, r: number, stops: Stop[]): CanvasGradient {
  const g = x.createRadialGradient(cx, cy, 0, cx, cy, Math.max(0.0001, r));
  for (const [o, c] of stops) g.addColorStop(o, c);
  return g;
}

function polyline(pts: { x: number; y: number }[]): Path2D {
  const p = new Path2D();
  pts.forEach((q, i) => (i === 0 ? p.moveTo(q.x, q.y) : p.lineTo(q.x, q.y)));
  return p;
}

function fillRect(x: Ctx, H: MochiH, color: string | CanvasGradient) {
  x.fillStyle = color;
  x.fillRect(-H.rx * 4, -H.ry * 4, H.rx * 8, H.ry * 8);
}

function stroke(x: Ctx, p: Path2D, color: string, width: number, cap: CanvasLineCap = "round") {
  x.strokeStyle = color;
  x.lineWidth = width;
  x.lineCap = cap;
  x.lineJoin = "round";
  x.stroke(p);
}

function ellipse(_x: Ctx, cx: number, cy: number, rx: number, ry: number): Path2D {
  const p = new Path2D();
  p.ellipse(cx, cy, Math.abs(rx), Math.abs(ry), 0, 0, Math.PI * 2);
  return p;
}

function rrect(x0: number, y0: number, w: number, h: number, r: number): Path2D {
  const p = new Path2D();
  p.roundRect(x0, y0, w, h, r);
  return p;
}

// ── Pompom and fuzzy trim ────────────────────────────────────────────────────

function drawPompom(x: Ctx, px: number, py: number, r: number, base = "#FFFFFF", shade = "#D5D9E2") {
  x.save();
  x.translate(px, py);
  const n = 11;
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2;
    const br = r * (0.34 + 0.06 * Math.sin(i * 2.3));
    const bx = Math.cos(a) * r * 0.78;
    const by = Math.sin(a) * r * 0.78;
    x.fillStyle = rad(x, bx - br * 0.4, by - br * 0.5, br * 1.3, [[0, base], [1, shade]]);
    x.fill(ellipse(x, bx, by, br, br));
  }
  x.fillStyle = rad(x, -r * 0.3, -r * 0.35, r * 1.05, [[0, base], [0.7, base], [1, shade]]);
  x.fill(ellipse(x, 0, 0, r * 0.86, r * 0.86));
  x.restore();
}

function drawFuzzyBand(x: Ctx, arc: P3[], thick: number, base = "#FFFFFF", shade = "#DADDE4") {
  if (arc.length < 2) return;
  const path = polyline(arc);
  stroke(x, path, shade, thick);
  stroke(x, path, base, thick * 0.78);
  const step = Math.max(2, Math.floor(arc.length / 16));
  for (let i = 0; i < arc.length; i += step) {
    const q = arc[i];
    const r = thick * (0.32 + 0.1 * Math.sin(i * 1.7));
    x.fillStyle = rad(x, q.x - r * 0.3, q.y - thick * 0.35 - r * 0.3, r * 1.2, [[0, base], [1, shade]]);
    x.fill(ellipse(x, q.x, q.y - thick * 0.32, r, r));
  }
}

// ── Hats ─────────────────────────────────────────────────────────────────────

function drawBeanie(x: Ctx, H: MochiH, bodyPath: Path2D, simplified: boolean) {
  const s = 1.035, yEdge = 0.42, yCuff = 0.58;
  const head = outfitPath(H.rx * s, H.ry * s);

  // Shadow on the head under the cuff.
  x.save();
  x.clip(bodyPath);
  x.clip(capClip(H, yEdge - 0.12, 1));
  fillRect(x, H, "rgba(30,40,70,0.10)");
  x.restore();

  // Knit body.
  x.save();
  x.clip(capClip(H, yCuff, s));
  x.fillStyle = lin(x, H.rx * 0.5, -H.ry * 1.1, -H.rx * 0.6, H.ry * 0.2, [[0, "#7DB6FF"], [1, "#2F6FE0"]]);
  x.fill(head);
  if (!simplified) {
    x.save();
    x.clip(head);
    for (let k = -6; k <= 6; k++) {
      const lon = k * 0.24;
      const pts: P3[] = [];
      for (let i = 0; i <= 16; i++) {
        const y = yCuff + ((1.05 - yCuff) * i) / 16;
        const q = proj(H, surf(y, lon, s));
        if (q.z > 0) pts.push(q);
      }
      if (pts.length < 2) continue;
      stroke(x, polyline(pts), "rgba(20,50,140,0.16)", H.R * 0.045);
    }
    x.restore();
  }
  x.restore();

  // Cuff: the folded band between yEdge and yCuff.
  x.save();
  x.clip(capClip(H, yEdge, s * 1.04));
  x.clip(invert(capClip(H, yCuff, s * 1.04), H), "evenodd");
  const cuffHead = outfitPath(H.rx * s * 1.04, H.ry * s * 1.04);
  x.fillStyle = lin(x, 0, -H.ry * 0.6, 0, -H.ry * 0.2, [[0, "#3C7BEA"], [1, "#2257C4"]]);
  x.fill(cuffHead);
  x.save();
  x.clip(cuffHead);
  for (let k = -14; k <= 14; k++) {
    const lon = k * 0.115;
    const a = proj(H, surf(yEdge, lon, s * 1.04));
    const b = proj(H, surf(yCuff, lon, s * 1.04));
    if (a.z < 0) continue;
    stroke(x, polyline([a, b]), "rgba(10,30,100,0.22)", H.R * 0.035, "butt");
  }
  x.restore();
  x.restore();

  // Top highlight.
  x.save();
  x.clip(capClip(H, yCuff, s));
  x.clip(head);
  x.fillStyle = rad(x, H.rx * 0.3, -H.ry * 0.85, H.R * 0.45, [[0, "rgba(255,255,255,0.35)"], [1, "rgba(255,255,255,0)"]]);
  x.fill(head);
  x.restore();

  const top = proj(H, [0, 1.08 * s, 0]);
  drawPompom(x, top.x + H.physDx * H.rx * 0.25, top.y - H.R * 0.12 + H.physDy * H.ry * 0.15, H.R * 0.24);
}

interface CapPalette {
  bag: [string, string, string];
  shadow: string;
  fold: string;
  trim: string;
  trimShade: string;
}

const SANTA: CapPalette = {
  bag: ["#FF6B6B", "#E53935", "#B71C1C"],
  shadow: "rgba(120,10,10,0.10)",
  fold: "rgba(90,0,0,0.20)",
  trim: "#FFFFFF",
  trimShade: "#DADDE4",
};

/** PC only: the night cap is Santa's hat in indigo, with a pale trim. */
const NIGHTCAP: CapPalette = {
  bag: ["#818CF8", "#4F46E5", "#312E81"],
  shadow: "rgba(30,20,110,0.12)",
  fold: "rgba(20,10,80,0.22)",
  trim: "#F4F5F9",
  trimShade: "#C7CAE0",
};

function drawSoftCap(x: Ctx, H: MochiH, bodyPath: Path2D, pal: CapPalette) {
  const s = 1.05, yEdge = 0.52;
  const arc = frontArc(H, yEdge, s);
  if (arc.length === 0) return;
  const L = arc[0];
  const Rt = arc[arc.length - 1];
  const crown = proj(H, [0, 1.05, 0]);
  const side = 1;
  const tip = { x: crown.x + side * H.rx * (0.95 + H.physDx * 0.35), y: crown.y + H.ry * (0.05 + H.physDy * 0.2) };
  const peak = { x: crown.x + side * H.rx * 0.25, y: crown.y - H.ry * 0.62 };

  const bag = new Path2D();
  bag.moveTo(L.x, L.y);
  bag.bezierCurveTo(L.x - H.rx * 0.05, L.y - H.ry * 0.7, peak.x - H.rx * 0.55, peak.y - H.ry * 0.05, peak.x, peak.y);
  bag.quadraticCurveTo(tip.x - H.rx * 0.05, peak.y - H.ry * 0.02, tip.x, tip.y);
  bag.quadraticCurveTo(tip.x - H.rx * 0.12, tip.y - H.ry * 0.22, peak.x + H.rx * 0.18, peak.y + H.ry * 0.32);
  bag.bezierCurveTo(Rt.x + H.rx * 0.05, peak.y + H.ry * 0.45, Rt.x + H.rx * 0.08, Rt.y - H.ry * 0.35, Rt.x, Rt.y);
  for (let i = arc.length - 1; i >= 0; i--) bag.lineTo(arc[i].x, arc[i].y);
  bag.closePath();

  // Shadow on the head.
  x.save();
  x.clip(bodyPath);
  x.clip(capClip(H, yEdge - 0.14, 1));
  fillRect(x, H, pal.shadow);
  x.restore();

  x.fillStyle = lin(x, -H.rx * 0.6, -H.ry * 1.6, H.rx * 0.7, -H.ry * 0.3, [[0, pal.bag[0]], [0.55, pal.bag[1]], [1, pal.bag[2]]]);
  x.fill(bag);

  // Folds and sheen.
  x.save();
  x.clip(bag);
  const folds: [number, number, number][] = [[0.15, 0.55, 0.1], [0.45, 0.85, 0.08]];
  for (const [a, b, w] of folds) {
    const f = new Path2D();
    f.moveTo(peak.x - H.rx * 0.1 + (Rt.x - L.x) * a * 0.3, peak.y + H.ry * 0.15);
    f.quadraticCurveTo(peak.x + H.rx * 0.35, peak.y + H.ry * (0.05 + a * 0.3), tip.x - H.rx * (0.45 - b * 0.3), tip.y - H.ry * 0.12);
    stroke(x, f, pal.fold, H.R * w);
  }
  x.fillStyle = rad(x, peak.x - H.rx * 0.25, peak.y + H.ry * 0.05, H.R * 0.5, [[0, "rgba(255,255,255,0.32)"], [1, "rgba(255,255,255,0)"]]);
  x.fill(bag);
  x.restore();

  drawFuzzyBand(x, arc, H.R * 0.3, pal.trim, pal.trimShade);
  drawPompom(x, tip.x, tip.y + H.R * 0.04, H.R * 0.22, pal.trim, pal.trimShade);
}

/** The ring's front run, as the party hat's base. */
function drawPartyHat(x: Ctx, H: MochiH, simplified: boolean) {
  const baseY = 0.82, baseR = 0.42;
  const lean = -0.24 + H.physDx * 0.12;
  const c = proj(H, [0.16, baseY + 0.06, 0]);
  const rg: P3[] = [];
  for (let i = 0; i <= 48; i++) {
    const a = (i / 48) * Math.PI * 2;
    rg.push(proj(H, [0.16 + baseR * Math.sin(a), baseY + 0.06, baseR * Math.cos(a)]));
  }
  let left = rg[0], right = rg[0];
  for (const q of rg) {
    if (q.x < left.x) left = q;
    if (q.x > right.x) right = q;
  }
  const h = H.ry * 1.6;
  const apex = { x: c.x + Math.sin(lean) * h, y: c.y - Math.cos(lean) * h };
  const front = frontSilhouetteArc(rg);

  const cone = new Path2D();
  cone.moveTo(left.x, left.y);
  cone.quadraticCurveTo((left.x + apex.x) / 2 - H.rx * 0.06, (left.y + apex.y) / 2, apex.x - H.R * 0.05, apex.y + H.R * 0.06);
  cone.quadraticCurveTo(apex.x, apex.y - H.R * 0.03, apex.x + H.R * 0.05, apex.y + H.R * 0.06);
  cone.quadraticCurveTo((right.x + apex.x) / 2 + H.rx * 0.06, (right.y + apex.y) / 2, right.x, right.y);
  for (let i = front.length - 1; i >= 0; i--) cone.lineTo(front[i].x, front[i].y);
  cone.closePath();

  x.fillStyle = lin(x, left.x, apex.y, right.x, left.y, [[0, "#FF9BD0"], [0.5, "#F15BAE"], [1, "#C2187A"]]);
  x.fill(cone);

  x.save();
  x.clip(cone);
  if (!simplified) {
    const dots: [number, number][] = [[0.25, -0.35], [0.3, 0.3], [0.55, -0.05], [0.72, 0.28], [0.8, -0.3], [0.45, 0.6], [0.48, -0.65]];
    for (const [t, u] of dots) {
      const bx = left.x + (right.x - left.x) * (0.5 + u * 0.5);
      const by = left.y + (right.y - left.y) * (0.5 + u * 0.5);
      const px = bx + (apex.x - bx) * (1 - t);
      const py = by + (apex.y - by) * (1 - t);
      const r = H.R * 0.075 * (0.6 + t * 0.5);
      x.fillStyle = "rgba(255,255,255,0.92)";
      x.fill(ellipse(x, px, py, r, r * 0.9));
    }
  }
  x.fillStyle = lin(x, left.x, 0, right.x, 0, [[0, "rgba(255,255,255,0.28)"], [0.35, "rgba(255,255,255,0)"], [1, "rgba(80,0,40,0.18)"]]);
  x.fill(cone);
  x.restore();

  if (front.length > 0) stroke(x, polyline(front), "#FFD84D", H.R * 0.07);
  drawPompom(x, apex.x, apex.y - H.R * 0.04, H.R * 0.16, "#FFE27A", "#F2B705");
}

const CROWN_YB = 0.46;

function drawCrownPart(x: Ctx, H: MochiH, side: number, simplified: boolean) {
  const s = 1.06, yb = CROWN_YB, yt = 0.66;
  const n = 8, spikeH = 0.42, N = 120;
  interface Seg { b: P3; tt: P3; z: number }
  const seg: Seg[] = [];
  for (let i = 0; i <= N; i++) {
    const lon = -Math.PI + (i / N) * 2 * Math.PI;
    const b = proj(H, surf(yb, lon, s));
    const phase = ((lon + Math.PI) / (2 * Math.PI)) * n;
    const f = phase - Math.floor(phase);
    const spike = Math.pow(Math.max(0, 1 - Math.abs(f - 0.5) * 2), 1.6);
    const topY = yt + spikeH * spike;
    const sp = surf(yt, lon, s);
    const tt = proj(H, [sp[0] * (1 - 0.08 * spike), topY, sp[2] * (1 - 0.08 * spike)]);
    seg.push({ b, tt, z: b.z });
  }
  const keep = seg.filter((q) => (side > 0 ? q.z >= 0 : q.z < 0.02)).sort((a, b) => a.b.x - b.b.x);
  if (keep.length < 2) return;

  const shape = new Path2D();
  shape.moveTo(keep[0].tt.x, keep[0].tt.y);
  for (let i = 1; i < keep.length; i++) shape.lineTo(keep[i].tt.x, keep[i].tt.y);
  for (let i = keep.length - 1; i >= 0; i--) shape.lineTo(keep[i].b.x, keep[i].b.y);
  shape.closePath();

  const dark = side < 0;
  x.fillStyle = dark
    ? lin(x, 0, -H.ry * 1.05, 0, -H.ry * 0.45, [[0, "#C98A12"], [1, "#8A5A06"]])
    : lin(x, 0, -H.ry * 1.05, 0, -H.ry * 0.45, [[0, "#FFE58A"], [0.5, "#FBBF24"], [1, "#D08A0B"]]);
  x.fill(shape);
  if (dark) return;

  x.save();
  x.clip(shape);
  x.fillStyle = lin(x, -H.rx, 0, H.rx, 0, [[0, "rgba(120,70,0,0.25)"], [0.45, "rgba(255,255,255,0)"], [0.62, "rgba(255,255,255,0.35)"], [1, "rgba(120,70,0,0.25)"]]);
  x.fill(shape);
  x.restore();

  if (simplified) return;
  const gems = ["#EF4444", "#3B82F6", "#22C55E", "#A855F7"];
  for (let k = 0; k < n; k++) {
    const lon = -Math.PI + ((k + 0.5) / n) * 2 * Math.PI;
    const sp = surf(yt, lon, s);
    const tipP = proj(H, [sp[0] * 0.92, yt + spikeH, sp[2] * 0.92]);
    const mid = proj(H, surf((yb + yt) / 2, lon, s * 1.01));
    if (mid.z <= 0.12) continue;
    const r = H.R * 0.055;
    x.fillStyle = rad(x, tipP.x - r * 0.3, tipP.y - r, r * 1.2, [[0, "#FFF6CC"], [1, "#E0A21A"]]);
    x.fill(ellipse(x, tipP.x, tipP.y - r * 0.5, r, r));
    const gr = H.R * 0.075;
    const gw = gr * Math.max(0.35, mid.z);
    x.fillStyle = gems[k % gems.length];
    x.fill(ellipse(x, mid.x, mid.y, gw, gr));
    x.fillStyle = "rgba(255,255,255,0.75)";
    x.fill(ellipse(x, mid.x - gr * 0.25 * mid.z, mid.y - gr * 0.35, gr * 0.28, gr * 0.28));
  }
}

function drawCrownFront(x: Ctx, H: MochiH, bodyPath: Path2D, simplified: boolean) {
  x.save();
  x.clip(bodyPath);
  x.clip(capClip(H, CROWN_YB - 0.1, 1));
  x.clip(invert(capClip(H, CROWN_YB, 1), H), "evenodd");
  fillRect(x, H, "rgba(80,50,0,0.12)");
  x.restore();
  drawCrownPart(x, H, 1, simplified);
}

function witchBrim(H: MochiH): P3[] {
  const y = 0.7, rr = 1.42;
  const out: P3[] = [];
  for (let i = 0; i <= 120; i++) {
    const a = -Math.PI + (i / 120) * 2 * Math.PI;
    const wob = 1 + 0.035 * Math.sin(a * 3 + 0.6);
    const droop = -0.1 * Math.pow(Math.abs(Math.sin(a)), 2);
    out.push(proj(H, [rr * wob * Math.sin(a), y + droop, rr * wob * Math.cos(a)]));
  }
  return out;
}

function drawWitchHatBack(x: Ctx, H: MochiH) {
  const pts = witchBrim(H);
  if (!pts.some((q) => q.z < 0.05)) return;
  const ell = polyline(pts);
  ell.closePath();
  x.fillStyle = lin(x, 0, -H.ry, 0, -H.ry * 0.4, [[0, "#2A0A4F"], [1, "#3B0F6B"]]);
  x.fill(ell);
}

function drawWitchHatFront(x: Ctx, H: MochiH, bodyPath: Path2D) {
  const all = witchBrim(H);
  const brim = polyline(all);
  brim.closePath();
  const fr = all.filter((q) => q.z >= 0).sort((a, b) => a.x - b.x);

  x.save();
  x.clip(bodyPath);
  x.clip(capClip(H, 0.5, 1));
  fillRect(x, H, "rgba(40,0,70,0.10)");
  x.restore();

  x.fillStyle = lin(x, 0, -H.ry * 0.9, 0, -H.ry * 0.3, [[0, "#5B21B6"], [1, "#3B0764"]]);
  x.fill(brim);
  if (fr.length > 0) stroke(x, polyline(fr), "rgba(190,150,255,0.35)", H.R * 0.035);

  const baseR = 0.62, by = 0.74;
  const bl = proj(H, [-baseR, by, 0]);
  const br = proj(H, [baseR, by, 0]);
  const c = proj(H, [0, by, 0]);
  const lean = 0.1 + H.physDx * 0.15;
  const top = { x: c.x + H.rx * 0.18 + Math.sin(lean) * H.ry * 0.3, y: c.y - H.ry * 1.25 };
  const tip = { x: top.x + H.rx * (0.45 + H.physDx * 0.25), y: top.y + H.ry * (0.22 + H.physDy * 0.1) };
  const capFront = frontArc(H, by, baseR / ringR(by)).filter((q) => q.x >= bl.x - 1 && q.x <= br.x + 1);

  const cone = new Path2D();
  cone.moveTo(bl.x, bl.y);
  cone.bezierCurveTo(bl.x + H.rx * 0.12, bl.y - H.ry * 0.5, top.x - H.rx * 0.28, top.y + H.ry * 0.25, top.x - H.rx * 0.02, top.y - H.ry * 0.02);
  cone.quadraticCurveTo(top.x + H.rx * 0.25, top.y - H.ry * 0.08, tip.x, tip.y);
  cone.quadraticCurveTo(top.x + H.rx * 0.22, top.y + H.ry * 0.08, top.x + H.rx * 0.14, top.y + H.ry * 0.22);
  cone.bezierCurveTo(br.x - H.rx * 0.18, c.y - H.ry * 0.45, br.x - H.rx * 0.02, br.y - H.ry * 0.2, br.x, br.y);
  for (let i = capFront.length - 1; i >= 0; i--) cone.lineTo(capFront[i].x, capFront[i].y);
  cone.closePath();

  x.fillStyle = lin(x, bl.x, top.y, br.x, bl.y, [[0, "#7C3AED"], [0.55, "#4C1D95"], [1, "#2E1065"]]);
  x.fill(cone);

  x.save();
  x.clip(cone);
  x.fillStyle = lin(x, bl.x, 0, br.x, 0, [[0, "rgba(255,255,255,0.22)"], [0.4, "rgba(255,255,255,0)"], [1, "rgba(0,0,0,0.15)"]]);
  x.fill(cone);
  const crease = new Path2D();
  crease.moveTo(top.x - H.rx * 0.05, top.y + H.ry * 0.05);
  crease.quadraticCurveTo(top.x + H.rx * 0.1, top.y + H.ry * 0.12, top.x + H.rx * 0.2, top.y + H.ry * 0.06);
  stroke(x, crease, "rgba(20,0,40,0.35)", H.R * 0.05);

  const fc = proj(H, [0, by, baseR]);
  const lift = H.ry * 0.11;
  const band = new Path2D();
  band.moveTo(bl.x - 2, bl.y - lift);
  band.quadraticCurveTo(fc.x, 2 * (fc.y - lift) - (bl.y + br.y) / 2, br.x + 2, br.y - lift);
  stroke(x, band, "#F97316", H.ry * 0.17, "butt");
  x.restore();

  const bk0 = proj(H, [0, by, baseR]);
  const bkx = bk0.x, bky = bk0.y - H.ry * 0.11;
  const bw = H.R * 0.2, bh = H.R * 0.16;
  x.fillStyle = "#FCD34D";
  x.fill(rrect(bkx - bw / 2, bky - bh / 2, bw, bh, bh * 0.25));
  x.fillStyle = "#C2410C";
  x.fill(rrect(bkx - bw / 2 + bw * 0.24, bky - bh / 2 + bh * 0.28, bw * 0.52, bh * 0.44, bh * 0.1));
}

function drawBunnyEars(x: Ctx, H: MochiH, rollProgress: number) {
  const R = H.R;
  const earH = R * 0.85;
  for (const sd of [-1, 1]) {
    const root = proj(H, [sd * 0.45, 0.92, 0]);
    const rl = proj(H, [sd * 0.45 - 0.22, 0.92, 0]);
    const rr = proj(H, [sd * 0.45 + 0.22, 0.92, 0]);
    const visHW = Math.max(R * 0.04, Math.abs(rr.x - rl.x) / 2);
    const flatten = Math.sin(rollProgress * Math.PI);
    const effH = earH * (1 - 0.8 * flatten);
    const tilt = sd * 0.6 * flatten;
    x.save();
    x.translate(root.x, root.y - effH * 0.65 + effH * 0.5);
    x.rotate(tilt);
    x.fillStyle = "#F9F0F0";
    x.fill(ellipse(x, 0, 0, visHW, effH / 2));
    stroke(x, (() => { const p = new Path2D(); p.ellipse(0, 0, visHW, effH / 2, 0, 0, Math.PI * 2); return p; })(), "rgba(0,0,0,0.06)", 0.8);
    x.fillStyle = "rgba(252,165,165,0.70)";
    x.fill(ellipse(x, 0, -effH / 2 + R * 0.1 + (effH * 0.65) / 2, visHW * 0.5, (effH * 0.65) / 2));
    x.restore();
  }
}

// ── Face and neck ────────────────────────────────────────────────────────────

function drawSunglasses(x: Ctx, H: MochiH, bodyPath: Path2D) {
  const eyes = eyeFrames(mochiH(H.R, H.yaw, H.pitch + H.roll, H.physDx, H.physDy));
  const w = H.R * 0.62, h = H.R * 0.46;
  x.save();
  x.clip(bodyPath);
  const [le, re] = eyes;
  if (le.visible && re.visible) {
    const b = new Path2D();
    b.moveTo(le.x + (w / 2) * le.fx * 0.9, le.y - h * 0.18);
    b.quadraticCurveTo((le.x + re.x) / 2, (le.y + re.y) / 2 - h * 0.42, re.x - (w / 2) * re.fx * 0.9, re.y - h * 0.18);
    stroke(x, b, "#111317", H.R * 0.07);
  }
  for (const e of eyes) {
    if (!e.visible) continue;
    const t = new Path2D();
    t.moveTo(e.x + e.sd * (w / 2) * e.fx, e.y - h * 0.2);
    t.lineTo(e.sd * H.rx * 1.05, e.y - h * 0.35);
    stroke(x, t, "#111317", H.R * 0.06);
  }
  for (const e of eyes) {
    if (!e.visible) continue;
    x.save();
    x.translate(e.x, e.y);
    x.scale(e.fx, e.fy);
    const lens = rrect(-w / 2, -h / 2, w, h, h * 0.42);
    x.fillStyle = "rgba(17,19,23,0.82)";
    x.fill(lens);
    stroke(x, lens, "#0B0C0F", H.R * 0.05, "butt");
    const glare = new Path2D();
    glare.moveTo(-w * 0.28, -h * 0.05);
    glare.lineTo(-w * 0.05, -h * 0.3);
    stroke(x, glare, "rgba(255,255,255,0.45)", H.R * 0.05);
    x.restore();
  }
  x.restore();
}

function drawRoundGlasses(x: Ctx, H: MochiH, bodyPath: Path2D) {
  const eyes = eyeFrames(mochiH(H.R, H.yaw, H.pitch + H.roll, H.physDx, H.physDy));
  const d = H.R * 0.56;
  x.save();
  x.clip(bodyPath);
  const [le, re] = eyes;
  if (le.visible && re.visible) {
    const b = new Path2D();
    b.moveTo(le.x + (d / 2) * le.fx, le.y - d * 0.08);
    b.quadraticCurveTo((le.x + re.x) / 2, (le.y + re.y) / 2 - d * 0.3, re.x - (d / 2) * re.fx, re.y - d * 0.08);
    stroke(x, b, "#8A4B12", H.R * 0.055);
  }
  for (const e of eyes) {
    if (!e.visible) continue;
    const t = new Path2D();
    t.moveTo(e.x + e.sd * (d / 2) * e.fx, e.y - d * 0.1);
    t.lineTo(e.sd * H.rx * 1.05, e.y - d * 0.25);
    stroke(x, t, "#8A4B12", H.R * 0.05);
  }
  for (const e of eyes) {
    if (!e.visible) continue;
    x.save();
    x.translate(e.x, e.y);
    x.scale(e.fx, e.fy);
    const c = ellipse(x, 0, 0, d / 2, d / 2);
    x.fillStyle = "rgba(190,225,255,0.18)";
    x.fill(c);
    stroke(x, c, "#9A5A1A", H.R * 0.065);
    const arc = new Path2D();
    arc.arc(0, 0, d / 2 - H.R * 0.03, Math.PI * 1.1, Math.PI * 1.45, false);
    stroke(x, arc, "rgba(255,255,255,0.55)", H.R * 0.03);
    x.restore();
  }
  x.restore();
}

function drawScarf(x: Ctx, H: MochiH) {
  const s = 1.05, y0 = -0.34, y1 = -0.66;
  const top = frontArcRoll(H, y0, s);
  const bot = frontArcRoll(H, y1, s);
  if (top.length === 0 || bot.length === 0) return;
  const band = polyline(top);
  for (let i = bot.length - 1; i >= 0; i--) band.lineTo(bot[i].x, bot[i].y);
  band.closePath();

  x.save();
  x.clip(outfitPath(H.rx * s, H.ry * s));
  x.fillStyle = lin(x, 0, -H.ry * 0.2, 0, H.ry * 0.7, [[0, "#F87171"], [1, "#B91C1C"]]);
  x.fill(band);
  x.save();
  x.clip(band);
  for (const lon of [-1.0, -0.45, 0.1, 0.65, 1.2]) {
    const a = proj(H, surf(y0, lon, s));
    const b = proj(H, surf(y1, lon, s));
    if (a.z < 0) continue;
    const st = new Path2D();
    st.moveTo(a.x, a.y - 4);
    st.lineTo(b.x, b.y + 4);
    stroke(x, st, "rgba(255,255,255,0.85)", H.R * 0.09 * Math.max(0.3, a.z));
  }
  x.restore();
  x.fillStyle = lin(x, 0, -H.ry * 0.5, 0, H.ry * 0.3, [[0, "rgba(255,255,255,0.18)"], [1, "rgba(0,0,0,0.10)"]]);
  x.fill(band);
  x.restore();

  // The hanging end.
  const k = proj(H, surf((y0 + y1) / 2, -0.55, s * 1.03));
  if (k.z <= 0) return;
  const sw = H.physDx * H.rx * 0.12;
  const end = new Path2D();
  end.moveTo(k.x - H.R * 0.16, k.y);
  end.quadraticCurveTo(k.x - H.R * 0.24 + sw, k.y + H.ry * 0.35, k.x - H.R * 0.2 + sw * 1.4, k.y + H.ry * 0.62);
  end.lineTo(k.x + H.R * 0.06 + sw * 1.4, k.y + H.ry * 0.6);
  end.quadraticCurveTo(k.x + H.R * 0.02 + sw, k.y + H.ry * 0.3, k.x + H.R * 0.12, k.y);
  end.closePath();
  x.fillStyle = lin(x, 0, k.y, 0, k.y + H.ry * 0.6, [[0, "#EF4444"], [1, "#B91C1C"]]);
  x.fill(end);
  x.save();
  x.clip(end);
  for (const t of [0.35, 0.7]) {
    x.fillStyle = "rgba(255,255,255,0.85)";
    x.fillRect(k.x - H.R * 0.4 + sw, k.y + H.ry * 0.62 * t, H.R * 0.8, H.R * 0.07);
  }
  x.restore();
  for (let i = 0; i < 4; i++) {
    const fx = k.x - H.R * 0.17 + sw * 1.4 + i * H.R * 0.075;
    const f = new Path2D();
    f.moveTo(fx, k.y + H.ry * 0.6);
    f.lineTo(fx, k.y + H.ry * 0.72);
    stroke(x, f, "#DC2626", H.R * 0.035);
  }
  x.save();
  x.translate(k.x, k.y);
  x.rotate(0.2);
  x.fillStyle = rad(x, -H.R * 0.05, -H.R * 0.05, H.R * 0.2, [[0, "#F87171"], [1, "#B91C1C"]]);
  x.fill(ellipse(x, 0, 0, H.R * 0.17, H.R * 0.14));
  x.restore();
}

function drawPumpkin(x: Ctx, H: MochiH, bodyPath: Path2D, simplified: boolean) {
  // The body is recoloured by the engine; this draws ribs, stem, leaf and tendril.
  if (!simplified) {
    x.save();
    x.clip(bodyPath);
    for (const lon of [-1.15, -0.55, 0.0, 0.55, 1.15]) {
      const pts: P3[] = [];
      for (let i = 0; i <= 30; i++) {
        const y = -0.98 + (1.96 * i) / 30;
        const q = projRoll(H, surf(y, lon, 1));
        if (q.z > 0) pts.push(q);
      }
      if (pts.length < 2) continue;
      const zz = pts[Math.floor(pts.length / 2)].z;
      stroke(x, polyline(pts), `rgba(150,50,0,${0.22 * zz})`, H.R * 0.12);
      stroke(x, polyline(pts.map((q) => ({ x: q.x + H.R * 0.07, y: q.y }))), `rgba(255,220,170,${0.18 * zz})`, H.R * 0.04);
    }
    x.restore();
  }
  const t = projRoll(H, [0.02, 1.0, 0]);
  const stem = new Path2D();
  stem.moveTo(t.x - H.R * 0.09, t.y + H.R * 0.04);
  stem.quadraticCurveTo(t.x - H.R * 0.08, t.y - H.R * 0.22, t.x + H.R * 0.08, t.y - H.R * 0.3);
  stem.lineTo(t.x + H.R * 0.13, t.y - H.R * 0.22);
  stem.quadraticCurveTo(t.x + H.R * 0.04, t.y - H.R * 0.15, t.x + H.R * 0.08, t.y + H.R * 0.04);
  stem.closePath();
  x.fillStyle = lin(x, t.x - H.R * 0.1, 0, t.x + H.R * 0.1, 0, [[0, "#65A30D"], [1, "#3F6212"]]);
  x.fill(stem);

  x.save();
  x.translate(t.x - H.R * 0.06, t.y - H.R * 0.02);
  x.rotate(-0.5);
  const leaf = new Path2D();
  leaf.moveTo(0, 0);
  leaf.quadraticCurveTo(-H.R * 0.18, -H.R * 0.2, -H.R * 0.38, -H.R * 0.02);
  leaf.quadraticCurveTo(-H.R * 0.18, H.R * 0.1, 0, 0);
  x.fillStyle = lin(x, 0, -H.R * 0.15, -H.R * 0.3, 0, [[0, "#84CC16"], [1, "#4D7C0F"]]);
  x.fill(leaf);
  const vein = new Path2D();
  vein.moveTo(-H.R * 0.02, -H.R * 0.01);
  vein.quadraticCurveTo(-H.R * 0.18, -H.R * 0.08, -H.R * 0.32, -H.R * 0.03);
  stroke(x, vein, "rgba(30,60,0,0.4)", H.R * 0.02);
  x.restore();

  if (!simplified) {
    const tendril = new Path2D();
    tendril.moveTo(t.x + H.R * 0.1, t.y - H.R * 0.12);
    tendril.bezierCurveTo(t.x + H.R * 0.3, t.y - H.R * 0.25, t.x + H.R * 0.35, t.y - H.R * 0.02, t.x + H.R * 0.22, t.y - H.R * 0.06);
    stroke(x, tendril, "#4D7C0F", H.R * 0.03);
  }
}

function drawBow(x: Ctx, H: MochiH) {
  const a = projRoll(H, surf(0.86, 0.55, 1.02));
  if (a.z < -0.2) return;
  const s = H.R * 0.26;
  const sq = Math.max(0.45, Math.cos(0.55 + H.yaw));
  x.save();
  x.translate(a.x, a.y);
  x.rotate(0.35 + H.yaw * 0.3);
  x.scale(sq, 1);
  for (const sd of [-1, 1]) {
    const wing = new Path2D();
    wing.moveTo(0, 0);
    wing.bezierCurveTo(sd * s * 0.6, -s * 0.85, sd * s * 1.35, -s * 0.55, sd * s * 1.15, 0);
    wing.bezierCurveTo(sd * s * 1.35, s * 0.55, sd * s * 0.6, s * 0.85, 0, 0);
    x.fillStyle = lin(x, 0, -s, 0, s, [[0, "#FF8CC6"], [1, "#DB2777"]]);
    x.fill(wing);
    const crease = new Path2D();
    crease.moveTo(sd * s * 0.25, -s * 0.05);
    crease.quadraticCurveTo(sd * s * 0.7, -s * 0.15, sd * s * 0.95, -s * 0.05);
    stroke(x, crease, "rgba(140,10,70,0.35)", s * 0.08);
  }
  x.fillStyle = rad(x, -s * 0.06, -s * 0.1, s * 0.35, [[0, "#FFB3D9"], [1, "#C2185B"]]);
  x.fill(ellipse(x, 0, 0, s * 0.24, s * 0.3));
  x.restore();
}

// ── Headphones (PC only) ─────────────────────────────────────────────────────

const PHONE_DARK = "#2B2E38";

/**
 * The band arches over the head in the plane through both ears; a cup sits on each ear. Parts
 * that are behind the head (z < 0) are drawn in the behind pass, the rest in front.
 */
function drawHeadphones(x: Ctx, H: MochiH, front: boolean, t: number) {
  const pts: P3[] = [];
  for (let i = 0; i <= 40; i++) {
    const a = (i / 40) * Math.PI;
    pts.push(proj(H, [1.06 * Math.cos(a), 0.02 + 1.04 * Math.sin(a), 0]));
  }
  // Runs of consecutive points on the wanted side.
  let run: P3[] = [];
  const flush = () => {
    if (run.length >= 2) stroke(x, polyline(run), PHONE_DARK, H.R * 0.15);
    run = [];
  };
  for (const q of pts) {
    if (q.z >= 0 === front) run.push(q);
    else flush();
  }
  flush();

  const pulse = 1 + 0.04 * Math.sin(t * 7);
  const wide = 0.6 + 0.4 * Math.abs(Math.sin(H.yaw));
  for (const sd of [-1, 1]) {
    const c = proj(H, [sd * 1.06, 0.0, 0]);
    if (c.z >= 0 !== front) continue;
    x.save();
    x.translate(c.x, c.y);
    x.scale(pulse * wide, pulse);
    x.fillStyle = PHONE_DARK;
    x.fill(rrect(-H.R * 0.17, -H.R * 0.33, H.R * 0.34, H.R * 0.66, H.R * 0.15));
    x.fillStyle = "#FF5C7A";
    x.fill(rrect(-H.R * 0.07 - sd * H.R * 0.04, -H.R * 0.2, H.R * 0.14, H.R * 0.4, H.R * 0.07));
    x.restore();
  }
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

export interface WardrobeState {
  outfit: Outfit;
  H: MochiH;
  cx: number;
  cy: number;
  tilt: number;
  sx: number;
  sy: number;
  morph: number;
  presence: number;
  rollTurns: number;
  /** Seconds, for what moves on its own (the headphone cups). */
  t: number;
}

let scratch: HTMLCanvasElement | null = null;

/** Draws into an offscreen copy and composites it at `alpha`, so overlapping parts do not ghost. */
function layer(x: Ctx, alpha: number, fn: (c: Ctx) => void) {
  if (alpha >= 0.995) {
    fn(x);
    return;
  }
  const cv = x.canvas;
  if (!scratch) scratch = document.createElement("canvas");
  if (scratch.width !== cv.width || scratch.height !== cv.height) {
    scratch.width = cv.width;
    scratch.height = cv.height;
  }
  const s = scratch.getContext("2d");
  if (!s) return;
  s.setTransform(1, 0, 0, 1, 0, 0);
  s.clearRect(0, 0, scratch.width, scratch.height);
  s.setTransform(x.getTransform());
  fn(s);
  x.save();
  x.setTransform(1, 0, 0, 1, 0, 0);
  x.globalAlpha *= alpha;
  x.drawImage(scratch, 0, 0);
  x.restore();
}

const isHat = (o: Outfit) => o === "beanie" || o === "santaHat" || o === "partyHat" || o === "crown" || o === "witchHat" || o === "nightcap";
const followsRoll = (o: Outfit) => o === "sunglasses" || o === "roundGlasses" || o === "bow" || o === "scarf" || o === "pumpkin";

/** Does this outfit turn with the body when he rolls (so the engine turns the whole canvas)? */
export function outfitFollowsRoll(o: Outfit | null): boolean {
  return !!o && followsRoll(o);
}

function body(x: Ctx, w: WardrobeState, fn: () => void) {
  x.save();
  x.translate(w.cx, w.cy);
  if (w.tilt !== 0) x.rotate(w.tilt);
  x.scale(w.sx, w.sy);
  fn();
  x.restore();
}

/** What is behind the body: the back of a crown or a brim, bunny ears, a glasses-side that turned away. */
export function drawOutfitBehind(x: Ctx, w: WardrobeState) {
  const { outfit, H } = w;
  const morphFade = 1 - Math.min(1, Math.max(0, (w.morph - 0.3) / 0.2));
  const alpha = morphFade * Math.min(1, w.presence * 2.5);
  if (morphFade <= 0.01 || alpha <= 0.005) return;
  const simplified = H.R < 16;
  const bodyPath = outfitPath(H.rx, H.ry);
  const posP = Ease.back(w.presence);
  const hatScale = 0.85 + 0.15 * posP;
  const rolling = Math.abs(H.roll) > 0.01;
  const u = Math.min(1, Math.abs(H.roll) / (2 * Math.PI * Math.max(1, w.rollTurns)));

  if (followsRoll(outfit)) {
    if (!(projRoll(H, [0, 0, 1]).z < 0)) return;
    layer(x, alpha, (c) => body(c, w, () => drawFollowing(c, outfit, H, bodyPath, simplified)));
    return;
  }
  if (outfit === "headphones") {
    layer(x, alpha, (c) => body(c, w, () => drawHeadphones(c, H, false, w.t)));
    return;
  }
  const lift = (c: Ctx) => {
    if (rolling && outfit !== "bunnyEars") {
      c.translate(H.physDx * H.rx * 0.2 * Math.sin(u * Math.PI), -H.ry * 0.45 * Math.sin(u * Math.PI));
      c.rotate(Math.sin(2 * Math.PI * u) * 0.35);
    } else {
      c.translate(0, -(1 - posP) * H.ry);
      c.scale(hatScale, hatScale);
    }
  };
  switch (outfit) {
    case "bunnyEars":
      layer(x, alpha, (c) => body(c, w, () => { c.save(); lift(c); drawBunnyEars(c, H, rolling ? u : 0); c.restore(); }));
      break;
    case "crown":
      layer(x, alpha, (c) => body(c, w, () => { c.save(); lift(c); drawCrownPart(c, H, -1, simplified); c.restore(); }));
      break;
    case "witchHat":
      layer(x, alpha, (c) => body(c, w, () => { c.save(); lift(c); drawWitchHatBack(c, H); c.restore(); }));
      break;
    default:
      break;
  }
}

function drawFollowing(c: Ctx, outfit: Outfit, H: MochiH, bodyPath: Path2D, simplified: boolean) {
  switch (outfit) {
    case "sunglasses": drawSunglasses(c, H, bodyPath); break;
    case "roundGlasses": drawRoundGlasses(c, H, bodyPath); break;
    case "scarf": drawScarf(c, H); break;
    case "pumpkin": drawPumpkin(c, H, bodyPath, simplified); break;
    case "bow": drawBow(c, H); break;
    default: break;
  }
}

/** What is in front of the body, after his eyes. */
export function drawOutfitFront(x: Ctx, w: WardrobeState) {
  const { outfit, H } = w;
  const morphFade = 1 - Math.min(1, Math.max(0, (w.morph - 0.3) / 0.2));
  const alpha = morphFade * Math.min(1, w.presence * 2.5);
  if (morphFade <= 0.01 || alpha <= 0.005) return;
  const simplified = H.R < 16;
  const bodyPath = outfitPath(H.rx, H.ry);
  const p = w.presence;
  const posP = Ease.back(p);
  const hatScale = 0.85 + 0.15 * posP;

  if (outfit === "bunnyEars") return;
  if (outfit === "headphones") {
    layer(x, alpha, (c) => body(c, w, () => drawHeadphones(c, H, true, w.t)));
    return;
  }
  if (followsRoll(outfit)) {
    if (projRoll(H, [0, 0, 1]).z < 0) return;
    layer(x, alpha, (c) =>
      body(c, w, () => {
        c.save();
        if (outfit === "bow") c.scale(Math.max(0.001, posP), Math.max(0.001, posP));
        else if (outfit !== "pumpkin") c.translate(0, (1 - p) * (outfit === "scarf" ? 0.3 : 0.25) * H.ry);
        drawFollowing(c, outfit, H, bodyPath, simplified);
        c.restore();
      }),
    );
    return;
  }

  // Hats. During a roll they fly up and swing; otherwise they settle in with a little overshoot.
  const rolling = Math.abs(H.roll) > 0.01;
  const u = Math.min(1, Math.abs(H.roll) / (2 * Math.PI * Math.max(1, w.rollTurns)));
  layer(x, alpha, (c) =>
    body(c, w, () => {
      c.save();
      if (rolling && isHat(outfit)) {
        c.translate(H.physDx * H.rx * 0.2 * Math.sin(u * Math.PI), -H.ry * 0.45 * Math.sin(u * Math.PI));
        c.rotate(Math.sin(2 * Math.PI * u) * 0.35);
      } else {
        c.translate(0, -(1 - posP) * H.ry);
        c.scale(hatScale, hatScale);
      }
      switch (outfit) {
        case "beanie": drawBeanie(c, H, bodyPath, simplified); break;
        case "santaHat": drawSoftCap(c, H, bodyPath, SANTA); break;
        case "nightcap": drawSoftCap(c, H, bodyPath, NIGHTCAP); break;
        case "partyHat": drawPartyHat(c, H, simplified); break;
        case "crown": drawCrownFront(c, H, bodyPath, simplified); break;
        case "witchHat": drawWitchHatFront(c, H, bodyPath); break;
        default: break;
      }
      c.restore();
    }),
  );
}
