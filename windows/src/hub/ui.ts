// Small pieces the Hub's panels are made of: a gauge tile, a sparkline, number formats.

import { h } from "../views/dom";

export interface Tile {
  el: HTMLElement;
  /** `pct` fills the bar (null: no bar); `sub` is the small line under the value. */
  set(value: string, pct: number | null, sub?: string, tone?: "ok" | "warn" | "crit" | ""): void;
}

/** A labelled value with a thin bar under it. */
export function tile(label: string, extra?: HTMLElement): Tile {
  const value = h("div", { class: "t-val", text: "–" });
  const sub = h("div", { class: "t-sub", text: "" });
  const fill = h("i");
  const bar = h("div", { class: "t-bar" }, fill);
  const el = h("div", { class: "hub-tile" }, h("div", { class: "t-lab", text: label }), value, bar, sub);
  if (extra) el.append(extra);
  return {
    el,
    set(v, pct, s = "", tone = "") {
      value.textContent = v;
      sub.textContent = s;
      bar.style.visibility = pct == null ? "hidden" : "visible";
      fill.style.width = `${Math.max(0, Math.min(100, pct ?? 0))}%`;
      el.className = `hub-tile ${tone}`;
    },
  };
}

/** A rolling line of the last `len` values, drawn small. */
export class Spark {
  readonly canvas = h("canvas", { class: "t-spark" }) as HTMLCanvasElement;
  private values: number[] = [];
  constructor(private len = 60, private max = 100) {}

  push(v: number) {
    this.values.push(v);
    if (this.values.length > this.len) this.values.shift();
    this.draw();
  }

  private draw() {
    const c = this.canvas;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth || 120;
    const hh = c.clientHeight || 16;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(hh * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(hh * dpr);
    }
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, hh);
    if (this.values.length < 2) return;
    ctx.strokeStyle = "rgba(130,200,255,0.9)";
    ctx.lineWidth = 1.2;
    ctx.beginPath();
    this.values.forEach((v, i) => {
      const x = (i / (this.len - 1)) * w;
      const y = hh - 1 - (Math.min(v, this.max) / this.max) * (hh - 2);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  }
}

export function tone(pct: number, warn = 75, crit = 90): "ok" | "warn" | "crit" {
  return pct >= crit ? "crit" : pct >= warn ? "warn" : "ok";
}

/** Bytes per second, short. */
export function rate(bps: number): string {
  if (bps < 1024) return `${Math.round(bps)} B/s`;
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(bps < 10240 ? 1 : 0)} KB/s`;
  return `${(bps / 1024 / 1024).toFixed(1)} MB/s`;
}

export function uptime(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const hrs = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return d > 0 ? `${d}d ${hrs}h` : hrs > 0 ? `${hrs}h ${m}m` : `${m}m`;
}

export function gb(mb: number): string {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
}
