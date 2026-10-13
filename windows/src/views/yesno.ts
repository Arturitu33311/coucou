// "Do you want me to…?": when a reply ends on a question that a yes or a no answers, the chat offers
// the two as buttons. The idea (and the rule that it must be ONE question, never a choice) is Plip's
// (hussainn7/plip-oss, MIT): its prompt asks for one suggested next step as a yes/no question and the
// island shows Yes / No. Here it is read off the reply, in Spanish (Jinx) and English.
//
// Deliberately strict: an open question ("¿qué prefieres?") or a choice ("¿A o B?") gets no buttons,
// because a Yes could not answer it.

export interface Offer {
  question: string;
  /** The words the buttons say and send. */
  yes: string;
  no: string;
}

const strip = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** First words of a question that a yes or no answers. Unaccented, lower case. */
const YES_NO_START = new Set([
  // Spanish
  "quieres", "quiere", "queres", "puedo", "podemos", "debo", "deberia", "deberiamos", "hago", "hacemos",
  "procedo", "sigo", "sigamos", "continuo", "continuamos", "confirmas", "confirmo", "necesitas", "tienes",
  "tiene", "te", "le", "les", "lo", "la", "los", "las", "me", "se", "vale", "ok", "listo", "correcto",
  "es", "son", "esta", "estan", "hay", "agendo", "anoto", "creo", "borro", "envio", "mando", "abro",
  "cierro", "pongo", "guardo", "recuerdo", "busco", "reviso", "preparo", "programo", "cancelo", "apruebas",
  "autorizas", "dale", "adelante",
  // English
  "want", "wanna", "should", "shall", "do", "does", "did", "can", "could", "would", "will", "is", "are",
  "was", "were", "have", "has", "may",
]);

/** Words that open a question a yes or no cannot answer. */
const OPEN_START = new Set([
  "que", "cual", "cuales", "cuando", "como", "donde", "quien", "quienes", "por", "cuanto", "cuantos",
  "cuanta", "cuantas", "para", "what", "which", "when", "how", "where", "who", "why", "whom", "whose",
]);

/** The last sentence of `reply` when it is one yes/no question; null otherwise. */
export function offerOf(reply: string): Offer | null {
  const text = reply.replace(/[*_`#>]/g, "").replace(/\s+/g, " ").trim();
  if (!text) return null;
  // The reply must END on the question (a question in the middle was already moved past).
  const tail = text.replace(/["'”’)\]\s]+$/u, "").replace(/[\p{Extended_Pictographic}️\s]+$/u, "");
  if (!tail.endsWith("?")) return null;

  // The sentence: back to the opening ¿ if there is one, else to the previous sentence end.
  const body = tail.slice(0, -1);
  const open = body.lastIndexOf("¿");
  const stop = Math.max(body.lastIndexOf(". "), body.lastIndexOf("! "), body.lastIndexOf("? "), body.lastIndexOf("\n"));
  const start = open >= 0 ? open + 1 : stop + 1;
  const question = body.slice(Math.max(0, start)).trim();
  if (question.length < 3 || question.length > 160) return null;

  const plain = strip(question).replace(/^[^a-z0-9]+/, "");
  const words = plain.split(/[^a-z0-9]+/).filter(Boolean);
  if (words.length === 0) return null;
  // A choice: "¿A o B?", "A or B?" (also "o no?" is still a yes/no, so only a bare " o " between words).
  if (/\b(o|or)\b/.test(plain) && !/\b(o|or) no\b$/.test(plain)) return null;
  if (OPEN_START.has(words[0])) return null;
  if (!YES_NO_START.has(words[0])) return null;

  const spanish = /[¿áéíóúñ]/i.test(text) || /\b(quieres|puedo|que|lo|te|hago|sigo|vale)\b/.test(plain);
  return { question: `${question}?`, yes: spanish ? "Sí" : "Yes", no: "No" };
}
