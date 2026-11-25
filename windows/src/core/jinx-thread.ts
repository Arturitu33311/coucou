// Jinx's shared thread (jinx/jinx-assist/jinx_thread.py), as the chat shows it. The same conversation
// is held on Telegram, the two phones and this PC: each line says where it was written.

import type { ChatMessage } from "./state";

export interface ThreadLine {
  id: string;
  ts: number;
  /** tg | s21 | s22 | pc | g2 | jinx */
  src: string;
  text: string;
}

/** Where a line was written, as the chat labels it; nothing for Jinx's own answers. */
export function sourceLabel(src: string): string | undefined {
  switch (src) {
    case "jinx": return undefined;
    case "tg": return "Telegram";
    case "s21": return "S21";
    case "s22": return "S22";
    case "pc": return "PC";
    case "g2": return "Gafas";
    default: return src.toUpperCase();
  }
}

/** The thread as chat bubbles: Jinx's lines are hers, everyone else's are Alan's. `firstId` numbers them. */
export function threadMessages(lines: ThreadLine[], firstId: number): ChatMessage[] {
  return lines
    .filter((l) => typeof l.text === "string" && l.text.trim() !== "")
    .map((l, i) => ({
      id: firstId + i,
      role: l.src === "jinx" ? "assistant" : "user",
      content: l.text,
      via: sourceLabel(l.src),
    }));
}

/** Which brain answered, in words for the chat's status line. */
export function modeLabel(mode: string | null): string {
  switch (mode) {
    case "hermes": return "Jinx (Hermes)";
    case "or": return "Jinx de respaldo · nube";
    case "local": return "Jinx de respaldo · local";
    case "cached": return "respuesta ya guardada";
    default: return "";
  }
}
