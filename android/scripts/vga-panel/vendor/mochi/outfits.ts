// Which outfit Mochi wears. The ids, the seasons and the "auto" rule are the Mac app's
// (NotchBuddy/Sources/CoucouKit/MochiWardrobe.swift); the drawing is wardrobe.ts, a port of
// MochiOutfitDrawing.swift. Added on the PC only, in "auto" when no season applies: a night cap
// while he sleeps, headphones while a player plays, sunglasses during a Pomodoro focus period.
// Mini bots never wear anything.

/** The Mac's wardrobe, then the two the PC adds. */
export type Outfit =
  | "partyHat" | "beanie" | "crown" | "sunglasses" | "roundGlasses" | "bow" | "scarf"
  | "witchHat" | "pumpkin" | "santaHat" | "bunnyEars"
  | "headphones" | "nightcap";

/** Settings value: "auto" lets the rules below pick, "none" keeps him bare. */
export type OutfitSetting = "auto" | "none" | Outfit;

export const OUTFIT_CHOICES: readonly { value: OutfitSetting; label: string }[] = [
  { value: "auto", label: "Automatic (seasons, music, focus)" },
  { value: "none", label: "None" },
  { value: "partyHat", label: "Party hat" },
  { value: "beanie", label: "Beanie" },
  { value: "crown", label: "Crown" },
  { value: "sunglasses", label: "Sunglasses" },
  { value: "roundGlasses", label: "Round glasses" },
  { value: "bow", label: "Bow" },
  { value: "scarf", label: "Scarf" },
  { value: "witchHat", label: "Witch hat" },
  { value: "pumpkin", label: "Pumpkin" },
  { value: "santaHat", label: "Santa hat" },
  { value: "bunnyEars", label: "Bunny ears" },
  { value: "headphones", label: "Headphones" },
  { value: "nightcap", label: "Night cap" },
];

const KNOWN = new Set<string>(OUTFIT_CHOICES.map((c) => c.value).filter((v) => v !== "auto" && v !== "none"));

/** Meeus/Jones/Butcher: month (1–12) and day of Easter Sunday. */
function easter(year: number): [number, number] {
  const a = year % 19;
  const b = Math.floor(year / 100);
  const c = year % 100;
  const d = Math.floor(b / 4);
  const e = b % 4;
  const f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3);
  const hh = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4);
  const k = c % 4;
  const l = (32 + 2 * e + 2 * i - hh - k) % 7;
  const m = Math.floor((a + 11 * hh + 22 * l) / 451);
  return [Math.floor((hh + l - 7 * m + 114) / 31), ((hh + l - 7 * m + 114) % 31) + 1];
}

/** The season's outfit for a local date — MochiWardrobe.swift's `seasonal`, rule for rule. */
export function seasonal(date: Date): Outfit | null {
  const day = date.getDate();
  const month = date.getMonth() + 1;
  const year = date.getFullYear();
  if ((month === 12 && day === 31) || (month === 1 && day <= 2)) return "partyHat";
  if (month === 12 && day <= 26) return "santaHat";
  if (month === 10 || (month === 11 && day === 1)) return "witchHat";
  const [em, ed] = easter(year);
  const days = Math.round((Date.UTC(year, month - 1, day) - Date.UTC(year, em - 1, ed)) / 86_400_000);
  if (days >= -2 && days <= 1) return "bunnyEars";
  if ((month === 6 && day >= 21) || month === 7 || month === 8) return "sunglasses";
  return null;
}

export interface OutfitContext {
  sleeping: boolean;
  musicPlaying: boolean;
  focusing: boolean;
  /** His eyes are saying something (an approval, dizzy, a heart): glasses would hide it. */
  eyesBusy: boolean;
  date: Date;
}

/** What he wears right now. Pure, so it can be tested. */
export function pickOutfit(setting: OutfitSetting | string | undefined, ctx: OutfitContext): Outfit | null {
  if (setting === "none") return null;
  if (setting && setting !== "auto") return KNOWN.has(setting) ? (setting as Outfit) : null;
  if (ctx.sleeping) return "nightcap";
  let pick: Outfit | null = seasonal(ctx.date);
  if (!pick && ctx.musicPlaying) pick = "headphones";
  if (!pick && ctx.focusing) pick = "sunglasses";
  // Glasses chosen by the rules step aside while his eyes are saying something (an approval, dizzy,
  // a heart); glasses chosen in Settings stay on.
  if (pick === "sunglasses" && ctx.eyesBusy) return null;
  return pick;
}
