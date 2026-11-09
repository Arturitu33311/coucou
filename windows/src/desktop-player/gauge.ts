// The desktop player's gauge: the battery and the internet as a glass pill, top
// right (the mockup's ring with its number, the state, the network). The data is
// what the island already has from sysstate.rs (`SysInfo`): battery, mains,
// charging and online. There is no Wi-Fi level or wired flag there, so the
// network line says only online or offline.

import type { SysInfo } from "../core/state";
import { t } from "../i18n/i18n";

/** The ring's length: a circle of radius 18 (2πr), its `stroke-dasharray`. */
export const RING = 2 * Math.PI * 18;

export interface GaugeLabel {
  /** The number inside the ring ("" with no battery). */
  pct: string;
  /** Charging / Plugged in / On battery. */
  state: string;
  /** Online / Offline. */
  net: string;
  /** The ring's `stroke-dashoffset`: 0 full, RING empty. */
  offset: number;
  /** At most 20 % and not charging: drawn red, as the wallpaper does. */
  low: boolean;
  hasBattery: boolean;
}

/** What the gauge says for `sys`, or null before the first report (the pill stays hidden). */
export function gaugeLabel(sys: SysInfo | null): GaugeLabel | null {
  if (!sys) return null;
  const hasBattery = typeof sys.battery === "number" && Number.isFinite(sys.battery);
  const pct = hasBattery ? Math.round(Math.min(100, Math.max(0, sys.battery as number))) : 100;
  return {
    pct: hasBattery ? String(pct) : "",
    state: sys.charging ? t("Charging") : sys.plugged || !hasBattery ? t("Plugged in") : t("On battery"),
    net: sys.online ? t("Online") : t("Offline"),
    offset: RING * (1 - pct / 100),
    low: hasBattery && pct <= 20 && !sys.charging,
    hasBattery,
  };
}

const SVG = "http://www.w3.org/2000/svg";

function circle(stroke: string): SVGCircleElement {
  const c = document.createElementNS(SVG, "circle");
  c.setAttribute("cx", "22");
  c.setAttribute("cy", "22");
  c.setAttribute("r", "18");
  c.setAttribute("fill", "none");
  c.setAttribute("stroke", stroke);
  c.setAttribute("stroke-width", "4");
  return c;
}

/** The pill itself; `set` repaints it from the last report (null hides it). */
export function buildGauge(): { root: HTMLElement; set: (sys: SysInfo | null) => void } {
  const root = document.createElement("div");
  root.className = "lg dp-gauge";
  root.hidden = true;

  const ringBox = document.createElement("div");
  ringBox.className = "dp-gauge-ring";
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("width", "44");
  svg.setAttribute("height", "44");
  svg.setAttribute("viewBox", "0 0 44 44");
  svg.setAttribute("aria-hidden", "true");
  const track = circle("rgba(255,255,255,0.15)");
  const arc = circle("currentColor");
  arc.setAttribute("class", "dp-gauge-arc");
  arc.setAttribute("stroke-linecap", "round");
  arc.setAttribute("stroke-dasharray", RING.toFixed(2));
  arc.setAttribute("transform", "rotate(-90 22 22)");
  svg.append(track, arc);
  const num = document.createElement("div");
  num.className = "dp-gauge-num";
  ringBox.append(svg, num);

  const text = document.createElement("div");
  text.className = "dp-gauge-text";
  const state = document.createElement("span");
  state.className = "dp-gauge-state";
  const net = document.createElement("span");
  net.className = "dp-gauge-net";
  text.append(state, net);
  root.append(ringBox, text);

  return {
    root,
    set(sys) {
      const g = gaugeLabel(sys);
      root.hidden = !g;
      if (!g) return;
      arc.setAttribute("stroke-dashoffset", g.offset.toFixed(2));
      root.classList.toggle("low", g.low);
      root.classList.toggle("offline", !sys?.online);
      num.textContent = g.pct;
      state.textContent = g.state;
      net.textContent = g.net;
    },
  };
}
