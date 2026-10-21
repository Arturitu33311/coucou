import { createApp } from "./app.ts";
import { createClient, describe } from "./client.ts";
import { loadSettings, saveSettings, type Settings } from "./config.ts";
import { EV, type RawEvent } from "./events.ts";
import { connect } from "./sdk.ts";

const $ = (id: string) => document.getElementById(id) as HTMLInputElement;
const say = (text: string, ok = true) => {
  const el = document.getElementById("msg")!;
  el.textContent = text;
  el.className = ok ? "ok" : "bad";
};

const { bridge, glasses } = await connect();
const store = {
  get: async (k: string) => (await bridge.getLocalStorage(k)) || "",
  set: async (k: string, v: string) => { await bridge.setLocalStorage(k, v); },
};

let app: ReturnType<typeof createApp> | null = null;

function launch(settings: Settings) {
  app?.stop();
  const client = createClient(settings, (url, init) => fetch(url, init as RequestInit));
  app = createApp({ glasses, client, store });
  void app.start();
  void app.pollNotices();
}

// Events from the glasses go to whichever app is running; before settings exist there is only "double-tap exits".
bridge.onEvenHubEvent((event) => {
  if (app) void app.onEvent(event as RawEvent);
  else if ((event as RawEvent).sysEvent?.eventType === EV.DOUBLE_CLICK) glasses.exit();
});

// Taking the glasses off pauses everything (no microphone, no notice polling), like leaving the app.
bridge.onDeviceStatusChanged((status) => {
  if (status.isWearing === undefined || !app) return;
  void app.onEvent({ sysEvent: { eventType: status.isWearing ? EV.FOREGROUND_ENTER : EV.FOREGROUND_EXIT } });
});

// Dev only (the simulator): http://localhost:5173/?base=https://…&key=… seeds the settings the phone form would save.
if (import.meta.env.DEV && !(await loadSettings(store))) {
  const q = new URLSearchParams(location.search);
  if (q.get("base") && q.get("key")) await saveSettings(store, q.get("base")!, q.get("key")!);
}

const saved = await loadSettings(store);
if (saved) {
  $("base").value = saved.base;
  launch(saved);
} else {
  await glasses.show("Jinx\n\nPon la dirección y la llave\nen el teléfono");
}

document.getElementById("save")!.addEventListener("click", async () => {
  const s = await saveSettings(store, $("base").value, $("key").value);
  if (!s) return say("Falta la dirección o la llave.", false);
  try {
    const res = await fetch(s.base + "/v1/models", { headers: { Authorization: "Bearer " + s.key } });
    if (res.status === 401) return say("El puente contesta, pero la llave no es esa.", false);
    if (!res.ok) return say(`El puente contestó ${res.status}.`, false);
  } catch (e) {
    return say(describe(e) || "No llego al puente: ¿Tailscale y la dirección?", false);
  }
  say("Listo: el puente contesta.");
  $("key").value = "";
  launch(s);
});
