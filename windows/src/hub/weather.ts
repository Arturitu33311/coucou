// The Hub's Weather: now and the next days for up to two cities. Off until turned on in
// Settings → Hub (it asks open-meteo.com), and nothing is asked while the tab is closed.

import { Bridge } from "../core/bridge";
import { State } from "../core/state";
import { h, clear } from "../views/dom";
import type { HubHost, HubTool } from "./types";

interface Day {
  date: string;
  min: number;
  max: number;
  rain: number;
  code: number;
}
interface Weather {
  city: string;
  temp: number;
  feels: number;
  humidity: number;
  windKmh: number;
  code: number;
  rainNext6h: number;
  days: Day[];
}

/** WMO weather code → a symbol and words. */
function look(code: number): [string, string] {
  if (code === 0) return ["☀️", "Clear"];
  if (code <= 2) return ["⛅", "Partly cloudy"];
  if (code === 3) return ["☁️", "Overcast"];
  if (code === 45 || code === 48) return ["🌫️", "Fog"];
  if (code >= 51 && code <= 57) return ["🌦️", "Drizzle"];
  if (code >= 61 && code <= 67) return ["🌧️", "Rain"];
  if (code >= 71 && code <= 77) return ["❄️", "Snow"];
  if (code >= 80 && code <= 82) return ["🌦️", "Showers"];
  if (code >= 85 && code <= 86) return ["🌨️", "Snow showers"];
  if (code >= 95) return ["⛈️", "Thunderstorm"];
  return ["🌡️", "—"];
}

const weekday = (date: string) => new Date(`${date}T12:00:00`).toLocaleDateString([], { weekday: "short" });

function card(w: Weather): HTMLElement {
  const [sym, words] = look(w.code);
  const days = w.days.slice(1, 4).map((d) =>
    h("div", { class: "wx-day" }, h("span", { text: weekday(d.date) }), h("span", { text: look(d.code)[0] }), h("span", { text: `${Math.round(d.max)}° / ${Math.round(d.min)}°` }), h("span", { class: "dim", text: d.rain >= 20 ? `${d.rain}%` : "" })),
  );
  return h(
    "div",
    { class: "wx-card" },
    h("div", { class: "wx-city", text: w.city }),
    h("div", { class: "wx-now" }, h("span", { class: "wx-sym", text: sym }), h("span", { class: "wx-temp", text: `${Math.round(w.temp)}°` }), h("span", { class: "wx-words", text: words })),
    h("div", { class: "wx-sub", text: `feels ${Math.round(w.feels)}° · ${w.humidity}% · ${Math.round(w.windKmh)} km/h${w.rainNext6h >= 20 ? ` · rain ${w.rainNext6h}% soon` : ""}` }),
    h("div", { class: "wx-days" }, ...days),
  );
}

function build(): HubHost {
  const body = h("div", { class: "wx" });
  const el = h("div", { class: "wx-wrap" }, body);
  let timer: number | null = null;
  let busy = false;

  async function load() {
    if (busy) return;
    busy = true;
    try {
      const s = State.settings;
      const cities = [s.weatherCity, s.weatherCity2].map((c) => (c ?? "").trim()).filter(Boolean);
      if (!s.weatherOn || cities.length === 0) {
        clear(body);
        body.append(h("div", { class: "hub-hint", text: !s.weatherOn ? "Weather is off. Turn it on in Settings → Hub and name a city (it asks open-meteo.com, only while this tab is open)." : "Name a city in Settings → Hub." }));
        return;
      }
      const results = await Promise.allSettled(cities.map((c) => Bridge.weatherGet(c)));
      clear(body);
      results.forEach((r, i) => {
        if (r.status === "fulfilled") body.append(card(r.value as Weather));
        else body.append(h("div", { class: "hub-err", text: `${cities[i]}: ${String(r.reason).replace(/^Error:\s*/, "")}` }));
      });
    } finally {
      busy = false;
    }
  }

  return {
    el,
    sync() {},
    start() {
      void load();
      timer = window.setInterval(() => void load(), 15 * 60_000);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      timer = null;
    },
  };
}

export const weatherTool: HubTool = { id: "weather", label: "Weather", build };
