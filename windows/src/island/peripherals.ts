// Batteries of paired and plugged devices (headphones, mouse, keyboard, game controller). The
// system says when they come, go or change level (peripherals.rs); this keeps the list in State
// for the Today tab and says, in the pill, when one falls to 20, 10 or 5 %.
//
// Each step is announced once per fall: the level has to climb 5 points above it (or the device
// to start charging) before it can be announced again, so a battery bouncing 20 ↔ 21 never nags.

import { Bridge, onEvent } from "../core/bridge";
import { State, type Peripheral } from "../core/state";
import type { Island } from "./island";

const STEPS = [20, 10, 5];
const HYSTERESIS = 5;
const FLASH_MS = 5000;

const ICONS: Record<string, string> = {
  headphones: "🎧", mouse: "🖱️", keyboard: "⌨️", gamepad: "🎮", speaker: "🔊", phone: "📱", wearable: "⌚", pen: "🖊️",
};
export const iconOf = (kind: string) => ICONS[kind] ?? "🔋";

/** The step a level has fallen to (20, 10, 5), or null above them. */
export function stepOf(percent: number): number | null {
  return STEPS.filter((s) => percent <= s).pop() ?? null;
}

/**
 * Given what was announced before and the devices now, which one (if any) to say something about,
 * and the new memory. `armed[id]` is the lowest step already announced for that device.
 */
export function lowBattery(
  armed: Record<string, number>,
  list: Peripheral[],
): { armed: Record<string, number>; say: { device: Peripheral; step: number } | null } {
  const next: Record<string, number> = {};
  let say: { device: Peripheral; step: number } | null = null;
  for (const d of list) {
    const was = armed[d.id];
    const step = stepOf(d.percent);
    // Charging, or climbed clear of the step it was announced at: armed again.
    const reset = d.charging || (was != null && d.percent >= was + HYSTERESIS);
    const memory = reset ? undefined : was;
    if (memory != null) next[d.id] = memory;
    if (d.charging || step == null) continue;
    if (memory == null || step < memory) {
      next[d.id] = step;
      if (!say || step < say.step) say = { device: d, step };
    }
  }
  return { armed: next, say };
}

export function registerPeripherals(island: Island) {
  let armed: Record<string, number> = {};
  let first = true;
  let clear: number | null = null;

  const apply = (list: Peripheral[]) => {
    State.peripherals = list;
    const r = lowBattery(armed, list);
    armed = r.armed;
    // What is already low when the island starts is not news; the Today tab lists it.
    if (!first && r.say && !State.paused) {
      const { device, step } = r.say;
      const crit = step <= 10;
      // A warning waits for the island to be on screen; a critical level brings it up.
      if (crit || State.mode !== "hidden") {
        State.flash = { icon: iconOf(device.kind), text: `${device.name} at ${device.percent}%`, tone: crit ? "crit" : "warn" };
        island.refreshStrip();
        if (clear != null) window.clearTimeout(clear);
        clear = window.setTimeout(() => {
          State.flash = null;
          clear = null;
          island.refreshStrip();
          State.notify();
        }, FLASH_MS);
      }
    }
    first = false;
    State.notify();
  };

  void Bridge.peripherals().then((l) => {
    if (l == null) return; // not under Tauri (a plain browser): nothing to say
    // The first look arms what is already low without announcing it.
    first = true;
    apply(l);
  });
  void onEvent<Peripheral[]>("peripherals", (l) => apply(l ?? []));
}
