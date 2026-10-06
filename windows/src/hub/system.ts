// The Hub's System and Server tabs: the vitals of this computer, and of the server over ssh.

import { Bridge } from "../core/bridge";
import { h } from "../views/dom";
import { State } from "../core/state";
import type { HubHost, HubTool } from "./types";
import { Spark, gb, rate, tile, tone, uptime } from "./ui";

export interface Vitals {
  cpu: number;
  memPct: number;
  memUsedMb: number;
  memTotalMb: number;
  swapPct: number;
  load: [number, number, number];
  uptimeS: number;
  rxBps: number;
  txBps: number;
  diskReadBps: number;
  diskWriteBps: number;
  tempC: number | null;
  battery: { pct: number; charging: boolean } | null;
  disks: { mount: string; pct: number; sizeGb: number }[];
  services: { name: string; active: boolean }[];
  containers: [number, number] | null;
}

const temp = (c: number | null) => (c == null ? "–" : `${Math.round(c)}°C`);

function buildSystem(): HubHost {
  const spark = new Spark(60, 100);
  const cpu = tile("CPU", spark.canvas);
  const mem = tile("Memory");
  const heat = tile("Temperature");
  const net = tile("Network");
  const disk = tile("Disk");
  const power = tile("Battery");
  const foot = h("div", { class: "hub-hint", text: "" });
  const err = h("div", { class: "hub-err" });
  const el = h("div", { class: "hub-sys" }, h("div", { class: "hub-grid g3" }, cpu.el, mem.el, heat.el, net.el, disk.el, power.el), foot, err);

  let timer: number | null = null;
  let busy = false;

  async function poll() {
    if (busy) return;
    busy = true;
    try {
      const v = await Bridge.sysSample();
      if (!v) return;
      err.textContent = "";
      cpu.set(`${Math.round(v.cpu)}%`, v.cpu, `load ${v.load[0].toFixed(2)}`, tone(v.cpu));
      spark.push(v.cpu);
      mem.set(`${Math.round(v.memPct)}%`, v.memPct, `${gb(v.memUsedMb)} of ${gb(v.memTotalMb)}${v.swapPct > 1 ? ` · swap ${Math.round(v.swapPct)}%` : ""}`, tone(v.memPct));
      heat.set(temp(v.tempC), v.tempC == null ? null : (v.tempC / 100) * 100, v.tempC == null ? "no sensor" : "CPU package", v.tempC == null ? "" : tone(v.tempC, 70, 85));
      net.set(`↓ ${rate(v.rxBps)}`, null, `↑ ${rate(v.txBps)}`);
      const root = v.disks[0];
      disk.set(root ? `${Math.round(root.pct)}%` : "–", root?.pct ?? null, `r ${rate(v.diskReadBps)} · w ${rate(v.diskWriteBps)}`, root ? tone(root.pct, 80, 92) : "");
      if (v.battery) power.set(`${Math.round(v.battery.pct)}%`, v.battery.pct, v.battery.charging ? "charging" : "on battery", v.battery.pct < 15 && !v.battery.charging ? "crit" : "");
      else power.set("AC", null, "no battery");
      foot.textContent = `up ${uptime(v.uptimeS)} · load ${v.load.map((x) => x.toFixed(2)).join(" ")}`;
    } catch (e) {
      err.textContent = String(e).replace(/^Error:\s*/, "");
    } finally {
      busy = false;
    }
  }

  return {
    el,
    sync() {},
    start() {
      void poll();
      timer = window.setInterval(() => void poll(), 1500);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      timer = null;
    },
  };
}

function buildServer(): HubHost {
  const spark = new Spark(60, 100);
  const cpu = tile("CPU", spark.canvas);
  const mem = tile("Memory");
  const disks = tile("Disks");
  const net = tile("Network");
  const svc = tile("Services");
  const dock = tile("Containers");
  const foot = h("div", { class: "hub-hint", text: "" });
  const err = h("div", { class: "hub-err" });
  const grid = h("div", { class: "hub-grid g3" }, cpu.el, mem.el, disks.el, net.el, svc.el, dock.el);
  const setup = h("div", { class: "hub-hint", text: "Set the server's ssh name in Settings → Hub (a name from ~/.ssh/config, with a key)." });
  const el = h("div", { class: "hub-sys" }, grid, foot, err, setup);

  let timer: number | null = null;
  let busy = false;

  async function poll() {
    if (busy) return;
    if (!State.settings.serverHost?.trim()) {
      grid.style.display = "none";
      setup.style.display = "";
      return;
    }
    grid.style.display = "";
    setup.style.display = "none";
    busy = true;
    try {
      const v = await Bridge.serverSample();
      if (!v) return;
      err.textContent = "";
      cpu.set(`${Math.round(v.cpu)}%`, v.cpu, `${temp(v.tempC)} · load ${v.load[0].toFixed(2)}`, tone(v.cpu));
      spark.push(v.cpu);
      mem.set(`${Math.round(v.memPct)}%`, v.memPct, `${gb(v.memUsedMb)} of ${gb(v.memTotalMb)}`, tone(v.memPct));
      const worst = v.disks.reduce((m, d) => Math.max(m, d.pct), 0);
      disks.set(v.disks.length ? `${Math.round(worst)}%` : "–", v.disks.length ? worst : null, v.disks.filter((d) => d.sizeGb > 2).map((d) => `${d.mount} ${Math.round(d.pct)}%`).join(" · "), tone(worst, 80, 92));
      net.set(`↓ ${rate(v.rxBps)}`, null, `↑ ${rate(v.txBps)}`);
      const up = v.services.filter((s) => s.active).length;
      const down = v.services.filter((s) => !s.active).map((s) => s.name);
      svc.set(v.services.length ? `${up}/${v.services.length}` : "–", v.services.length ? (up / v.services.length) * 100 : null, down.length ? `down: ${down.join(", ")}` : "all running", down.length ? "crit" : "ok");
      if (v.containers) {
        dock.set(`${v.containers[0]}`, null, v.containers[1] ? `${v.containers[1]} unhealthy` : "all healthy", v.containers[1] ? "warn" : "ok");
      } else {
        dock.set("–", null, "docker not readable");
      }
      const bat = v.battery ? ` · battery ${Math.round(v.battery.pct)}%` : "";
      foot.textContent = `${State.settings.serverHost} · up ${uptime(v.uptimeS)}${bat}`;
    } catch (e) {
      const msg = String(e).replace(/^Error:\s*/, "");
      err.textContent = msg === "not-configured" ? "" : msg;
    } finally {
      busy = false;
    }
  }

  return {
    el,
    sync() {},
    start() {
      void poll();
      timer = window.setInterval(() => void poll(), 3000);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      timer = null;
    },
  };
}

export const systemTool: HubTool = { id: "system", label: "System", build: buildSystem };
export const serverTool: HubTool = { id: "server", label: "Server", build: buildServer };
