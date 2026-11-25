// Jinx on the glasses as a pure state machine: the screen, what the user did, what to do next. No SDK, no network.
// Double-tap is handled outside (it always exits, the store's rule); everything else navigates.

import { paginate, withFooter } from "./layout.ts";

export interface Notice { id: number; text: string; source?: string }

export type Screen =
  | { kind: "home"; note?: string }
  | { kind: "listening" }
  | { kind: "working"; heard?: string }
  | { kind: "answer"; pages: string[]; page: number }
  | { kind: "notices"; index: number }
  | { kind: "error"; message: string };

export interface Model {
  screen: Screen;
  /** Notices not read yet, oldest first. */
  unread: Notice[];
  /** Highest notice id already seen (the next fetch asks for what is newer). */
  seen: number;
  /** Bumped on each question so a late answer to a cancelled one is ignored. */
  ticket: number;
}

export type Msg =
  | { t: "tap" } | { t: "up" } | { t: "down" } | { t: "pressStart" } | { t: "pressEnd" }
  | { t: "background" }
  | { t: "answer"; ticket: number; text: string }
  | { t: "heard"; ticket: number; text: string }
  | { t: "failed"; ticket: number; message: string }
  | { t: "empty"; ticket: number }
  | { t: "notices"; items: Notice[] };

export type Effect =
  | { e: "startMic" }
  | { e: "cancelMic" }
  | { e: "sendVoice"; ticket: number }
  | { e: "cancelWork" }
  | { e: "markRead"; upTo: number };

export interface Step { model: Model; effects: Effect[] }

export const initial = (): Model => ({ screen: { kind: "home" }, unread: [], seen: 0, ticket: 0 });

const go = (m: Model, screen: Screen, effects: Effect[] = []): Step => ({ model: { ...m, screen }, effects });

export function reduce(m: Model, msg: Msg): Step {
  const s = m.screen;

  // Things that can arrive on any screen.
  if (msg.t === "notices") {
    const known = new Set(m.unread.map((n) => n.id));
    const fresh = msg.items.filter((n) => n.id > m.seen && !known.has(n.id));
    return { model: { ...m, unread: [...m.unread, ...fresh].slice(-20) }, effects: [] };
  }
  if (msg.t === "background") {
    if (s.kind === "listening") return go({ ...m, ticket: m.ticket + 1 }, { kind: "home" }, [{ e: "cancelMic" }]);
    return { model: m, effects: [] };
  }
  if (msg.t === "answer" || msg.t === "heard" || msg.t === "failed" || msg.t === "empty") {
    if (msg.ticket !== m.ticket || s.kind !== "working") return { model: m, effects: [] }; // a cancelled question
    if (msg.t === "heard") return go(m, { kind: "working", heard: msg.text });
    if (msg.t === "answer") return go(m, { kind: "answer", pages: paginate(msg.text), page: 0 });
    if (msg.t === "empty") return go(m, { kind: "home", note: "No te entendí. Mantén pulsado y habla." });
    return go(m, { kind: "error", message: msg.message });
  }

  // Long press: push-to-talk from anywhere that is not already talking or waiting.
  if (msg.t === "pressStart") {
    if (s.kind === "listening" || s.kind === "working") return { model: m, effects: [] };
    return go(m, { kind: "listening" }, [{ e: "startMic" }]);
  }
  if (msg.t === "pressEnd") {
    if (s.kind !== "listening") return { model: m, effects: [] };
    const ticket = m.ticket + 1;
    return go({ ...m, ticket }, { kind: "working" }, [{ e: "sendVoice", ticket }]);
  }

  switch (s.kind) {
    case "home":
      if (msg.t === "tap" && m.unread.length) return go(m, { kind: "notices", index: 0 });
      return go(m, { kind: "home" });
    case "listening":
      return { model: m, effects: [] };
    case "working":
      if (msg.t === "tap") return go({ ...m, ticket: m.ticket + 1 }, { kind: "home" }, [{ e: "cancelWork" }]);
      return { model: m, effects: [] };
    case "answer": {
      const last = s.pages.length - 1;
      if (msg.t === "down") return go(m, { ...s, page: Math.min(last, s.page + 1) });
      if (msg.t === "up") return go(m, { ...s, page: Math.max(0, s.page - 1) });
      if (msg.t === "tap") return s.page < last ? go(m, { ...s, page: s.page + 1 }) : go(m, { kind: "home" });
      return { model: m, effects: [] };
    }
    case "notices": {
      const last = m.unread.length - 1;
      if (msg.t === "up") return go(m, { kind: "notices", index: Math.max(0, s.index - 1) });
      if (msg.t === "down") return go(m, { kind: "notices", index: Math.min(last, s.index + 1) });
      if (msg.t === "tap") {
        if (s.index < last) return go(m, { kind: "notices", index: s.index + 1 });
        const upTo = Math.max(m.seen, ...m.unread.map((n) => n.id));
        return go({ ...m, unread: [], seen: upTo }, { kind: "home" }, [{ e: "markRead", upTo }]);
      }
      return { model: m, effects: [] };
    }
    case "error":
      return go(m, { kind: "home" });
  }
}

/** The text for the one container on the glasses. */
export function render(m: Model): string {
  const s = m.screen;
  switch (s.kind) {
    case "home": {
      const latest = m.unread[m.unread.length - 1];
      const lines = ["Jinx"];
      if (s.note) lines.push("", s.note);
      if (latest) {
        lines.push("", ...paginate(latest.text, 4)[0].split("\n"));
        lines.push("", m.unread.length > 1 ? `Toca: ${m.unread.length} avisos` : "Toca: ver aviso");
      } else if (!s.note) lines.push("", "Mantén pulsado y habla");
      return lines.join("\n");
    }
    case "listening": return "Te escucho…\n\nSuelta al terminar";
    case "working": return s.heard ? `“${paginate(s.heard, 3)[0].replace(/\n/g, " ")}”\n\nPensando…` : "Pensando…";
    case "answer": return withFooter(s.pages, s.page);
    case "notices": {
      const n = m.unread[Math.min(s.index, m.unread.length - 1)];
      if (!n) return "Sin avisos";
      const body = paginate(n.text, 6)[0];
      return `${n.source ? n.source + "\n" : ""}${body}\n${s.index + 1}/${m.unread.length}`;
    }
    case "error": return `No pude:\n${paginate(s.message, 6)[0]}`;
  }
}
