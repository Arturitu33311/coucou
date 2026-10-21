// The glue: glasses events → state machine → screen, with the network and the microphone behind small interfaces
// so the whole flow runs in tests with a fake pair of glasses.

import { createRecorder } from "./audio.ts";
import { describe, type Client } from "./client.ts";
import { decode, type RawEvent } from "./events.ts";
import { initial, reduce, render, type Effect, type Model, type Msg, type Notice } from "./state.ts";

/** What the app needs from the glasses (sdk.ts implements it with the real SDK). */
export interface Glasses {
  show(text: string): Promise<void>;
  mic(on: boolean): Promise<void>;
  exit(): void;
}

export interface AppDeps {
  glasses: Glasses;
  client: Client;
  /** Where the last seen notice id lives between runs (the bridge's inbox ids only grow). */
  store: { get(key: string): Promise<string>; set(key: string, value: string): Promise<void> };
  /** Long-poll seconds for notices; 0 in tests so they do not wait. */
  noticeWait?: number;
  /** Pause between failed notice polls (ms). */
  retryMs?: number;
  sleep?: (ms: number) => Promise<void>;
}

export function createApp(deps: AppDeps) {
  const { glasses, client, store } = deps;
  const sleep = deps.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const rec = createRecorder();
  let model: Model = initial();
  let shown = "";
  let work: AbortController | null = null;
  let closed = false;
  let foreground = true;

  async function paint() {
    const text = render(model);
    if (text === shown) return;
    shown = text;
    try { await glasses.show(text); } catch { /* the display can fail while the glasses reconnect */ }
  }

  function dispatch(msg: Msg) {
    const step = reduce(model, msg);
    model = step.model;
    for (const e of step.effects) void run(e);
    return paint();
  }

  async function run(e: Effect) {
    switch (e.e) {
      case "startMic":
        rec.clear();
        try { await glasses.mic(true); } catch { /* sin micrófono: el envío dirá que el audio es muy corto */ }
        break;
      case "cancelMic":
        rec.clear();
        try { await glasses.mic(false); } catch { /* ya está cerrado */ }
        break;
      case "cancelWork":
        work?.abort();
        break;
      case "markRead":
        try { await store.set("seen", String(e.upTo)); } catch { /* se vuelven a ver, no pasa nada */ }
        break;
      case "sendVoice": {
        try { await glasses.mic(false); } catch { /* ya está cerrado */ }
        const pcm = rec.take();
        work?.abort();
        const ctl = (work = new AbortController());
        try {
          const r = await client.voice(pcm, ctl.signal);
          if ((r as any).status === "empty") return void dispatch({ t: "empty", ticket: e.ticket });
          if (r.heard) await dispatch({ t: "heard", ticket: e.ticket, text: r.heard });
          await dispatch({ t: "answer", ticket: e.ticket, text: r.text ?? "" });
        } catch (err) {
          if (ctl.signal.aborted) return;
          const bad = (err as { message?: string })?.message === "audio demasiado corto";
          void (bad ? dispatch({ t: "empty", ticket: e.ticket })
                    : dispatch({ t: "failed", ticket: e.ticket, message: describe(err) }));
        }
        break;
      }
    }
  }

  async function pollNotices() {
    const stored = Number(await store.get("seen").catch(() => "0")) || 0;
    model = { ...model, seen: stored };
    let cursor = stored; // what the bridge has already given us this run (read or not)
    while (!closed) {
      if (!foreground) { await sleep(deps.retryMs ?? 3000); continue; }
      try {
        const items: Notice[] = await client.notices(cursor, deps.noticeWait ?? 20);
        const fresh = items.filter((n) => n.id > cursor);
        if (fresh.length) {
          cursor = Math.max(cursor, ...fresh.map((n) => n.id));
          await dispatch({ t: "notices", items: fresh });
        } else if (items.length || deps.noticeWait === 0) {
          await sleep(deps.retryMs ?? 3000); // a bridge that answers at once with nothing new: do not spin
        }
      } catch {
        await sleep(deps.retryMs ?? 3000); // sin red o sin bridge: avisos en pausa, nada que decir
      }
    }
  }

  return {
    get model() { return model; },
    start() { shown = ""; return paint(); },
    /** Feed one raw glasses event. Double-tap always asks to exit, whatever the screen. */
    async onEvent(event: RawEvent) {
      for (const input of decode(event)) {
        switch (input.t) {
          case "double": glasses.exit(); break;
          case "exit": closed = true; work?.abort(); rec.clear(); void glasses.mic(false).catch(() => {}); break;
          case "foreground": foreground = true; break;
          case "background": foreground = false; await dispatch({ t: "background" }); break;
          case "pcm": if (model.screen.kind === "listening") rec.push(input.data); break;
          default: await dispatch(input as Msg);
        }
      }
    },
    pollNotices,
    stop() { closed = true; work?.abort(); },
  };
}
