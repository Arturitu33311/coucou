// Jinx's mood, read from what she just said: which of Mochi's emotes her reply carries.
// The phone has the same rules (android …/coucou/assist/JinxMood.kt); both must pass
// windows/scripts/phone/jinx-mood-vectors.json (scripts/check-contracts.sh keeps the two copies equal).
//
// Rules are tried in order and the first one that matches wins, so a reply that laughs and apologises
// laughs. A rule matches on an emoji anywhere, or on a word: "llor*" is a word start (llorar, lloro…),
// "uy" a whole word, "por fin" a phrase. Accents and case don't count. Nothing matches → null (no emote).

import type { BotEmoteName } from "./layout";

interface Rule {
  emote: BotEmoteName;
  emoji: string[];
  words: string[];
}

export const JINX_MOOD_RULES: Rule[] = [
  { emote: "crying", emoji: ["😭"], words: ["llor*", "estoy llorando"] },
  { emote: "laughing", emoji: ["😂", "🤣", "😆", "😹"], words: ["jaja*", "jeje*", "jiji*", "jsjs*", "xd", "lol", "haha*"] },
  { emote: "love", emoji: ["❤", "♥", "😍", "🥰", "😘", "💕", "💖", "💗", "💜", "💚", "💙", "🫶"], words: ["te quiero", "te amo", "te adoro", "<3"] },
  { emote: "scared", emoji: ["😱", "😨", "😰", "🙀"], words: ["miedo", "aterr*", "peligro", "cuidado"] },
  { emote: "grumpy", emoji: ["😠", "😡", "😤", "🙄", "💢"], words: ["enoj*", "molest*", "ugh", "harta", "harto", "otra vez"] },
  { emote: "sad", emoji: ["😢", "😞", "😔", "🥺", "💔", "😿"], words: ["triste*", "lo siento", "lo lamento", "lament*", "no pude", "no puedo", "fall*"] },
  { emote: "flustered", emoji: ["😅", "😵", "💦", "🫣"], words: ["ups", "uy", "oops", "perdon", "me equivoque"] },
  { emote: "shy", emoji: ["🙈", "😳", "☺"], words: ["verguenza", "me sonroj*", "que pena"] },
  { emote: "smug", emoji: ["😏", "😎"], words: ["obvio", "obviamente", "te lo dije", "por supuesto"] },
  { emote: "confused", emoji: ["🤔", "❓", "😕"], words: ["no entiendo", "no entendi", "no lo se", "ni idea", "no estoy segur*", "a que te refieres", "confund*", "confus*"] },
  { emote: "surprised", emoji: ["😮", "😲", "😯", "🤯", "‼"], words: ["wow", "guau", "no puede ser", "en serio?", "increible*"] },
  { emote: "excited", emoji: ["🎉", "🥳", "🤩", "✨", "🚀"], words: ["genial*", "emocion*", "que emocion", "yay", "siii*"] },
  { emote: "proud", emoji: ["✅", "🏆", "⭐", "🌟"], words: ["listo", "ya quedo", "ya esta hecho", "lo logr*", "terminad*", "completad*"] },
  { emote: "relieved", emoji: ["😌", "😮‍💨"], words: ["por fin", "menos mal", "que alivio", "alivio", "uf", "uff*"] },
  { emote: "determined", emoji: ["💪", "🔥", "⚡"], words: ["vamos", "a darle", "manos a la obra", "me encargo", "dejamelo", "yo me ocupo"] },
  { emote: "curious", emoji: ["👀", "🧐"], words: ["cuentame", "interesante", "curios*", "y eso?", "como asi?"] },
  { emote: "sleepy", emoji: ["😴", "🥱", "💤"], words: ["sueno", "dormir", "buenas noches", "a mimir", "cansad*"] },
  { emote: "bored", emoji: ["😑", "😐"], words: ["aburr*", "meh"] },
  { emote: "happy", emoji: ["😊", "🙂", "😄", "😁", "😃", "😀", "🤗", ":)", ":D"], words: ["hola", "buenos dias", "buenas tardes", "feliz", "me alegra*", "que bien", "perfecto", "excelente", "claro"] },
];

/** Lower case, no accents (ñ → n too: "sueño" and "sueno" are the same word to a reader in a hurry). */
export function foldText(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** A word rule as a regex over folded text: a letter can't come right before it, nor after it unless it ends in "*". */
function wordRegex(word: string): RegExp {
  const stem = word.endsWith("*");
  const body = escape(foldText(stem ? word.slice(0, -1) : word));
  return new RegExp(`(?<![\\p{L}\\p{N}])${body}${stem ? "" : "(?![\\p{L}\\p{N}])"}`, "u");
}

const COMPILED = JINX_MOOD_RULES.map((r) => ({ emote: r.emote, emoji: r.emoji, words: r.words.map(wordRegex) }));

/** The emote Jinx's reply carries, or null when it reads neutral. */
export function jinxEmote(text: string | null | undefined): BotEmoteName | null {
  if (!text) return null;
  const folded = foldText(text);
  for (const r of COMPILED) {
    if (r.emoji.some((e) => text.includes(e)) || r.words.some((re) => re.test(folded))) return r.emote;
  }
  return null;
}
