// Mochi's accessories, drawn in code like the rest of him. The Mac app has a wardrobe of its own
// (NotchBuddy/Sources/CoucouKit/MochiWardrobe.swift, MochiOutfitDrawing.swift: 3-D shapes that turn
// with his head). That drawing is not ported; this file keeps its contract so a port can replace
// the shapes without touching anything else:
//   - the same ids (partyHat, santaHat, witchHat, bunnyEars, sunglasses, bow), the same "auto" =
//     the seasons, the same dates (seasonal() below is MochiWardrobe.swift's rule);
//   - the Mac ids that are not drawn here (beanie, crown, roundGlasses, scarf, pumpkin) read as
//     "none" until they are.
// Added on the PC only, in "auto" when no season applies: headphones while a player plays, a night
// cap when he sleeps, sunglasses during a Pomodoro focus period. Mini bots never wear anything.

export type Outfit =
  | "partyHat" | "santaHat" | "witchHat" | "bunnyEars" | "sunglasses" | "bow"
  | "headphones" | "nightcap";

/** Settings value: "auto" lets the rules below pick, "none" keeps him bare. */
export type OutfitSetting = "auto" | "none" | Outfit;

export const OUTFIT_CHOICES: readonly { value: OutfitSetting; label: string }[] = [
  { value: "auto", label: "Automatic (seasons, music, focus)" },
  { value: "none", label: "None" },
  { value: "partyHat", label: "Party hat" },
  { value: "santaHat", label: "Santa hat" },
  { value: "witchHat", label: "Witch hat" },
  { value: "bunnyEars", label: "Bunny ears" },
  { value: "sunglasses", label: "Sunglasses" },
  { value: "bow", label: "Bow" },
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

const DARK = "#2B2E38";

/** A cap that droops to the right: the night cap and Santa's hat differ only in colour. */
function softCap(x: CanvasRenderingContext2D, R: number, rx: number, ry: number, t: number, body: string, rim: string) {
  const baseY = -ry * 0.74;
  const w = rx * 0.62;
  const sway = Math.sin(t * 1.3) * R * 0.03;
  const tipX = rx * 0.9;
  const tipY = -ry * 1.05 + sway;
  x.fillStyle = body;
  x.beginPath();
  x.moveTo(-w, baseY);
  x.quadraticCurveTo(-w * 0.5, -ry * 1.55, tipX, tipY);
  x.quadraticCurveTo(rx * 0.35, -ry * 1.0, w, baseY);
  x.closePath();
  x.fill();
  x.fillStyle = rim;
  x.beginPath();
  x.roundRect(-w - R * 0.03, baseY - R * 0.09, w * 2 + R * 0.06, R * 0.2, R * 0.1);
  x.fill();
  x.beginPath();
  x.arc(tipX, tipY + R * 0.04, R * 0.13, 0, Math.PI * 2);
  x.fill();
}

/**
 * Draws `outfit` in the body's own space: origin at the centre of the body, `rx`/`ry` its radii and
 * `R` the unit everything is measured in (the caller has already applied tilt and squash).
 */
export function drawOutfit(x: CanvasRenderingContext2D, outfit: Outfit, R: number, rx: number, ry: number, t: number) {
  x.save();
  x.lineCap = "round";
  x.lineJoin = "round";
  switch (outfit) {
    case "headphones": {
      // The band over the top, two cups on the sides; the cups pulse a hair with time (music).
      x.strokeStyle = DARK;
      x.lineWidth = R * 0.15;
      x.beginPath();
      x.ellipse(0, -ry * 0.04, rx * 1.01, ry * 1.0, 0, Math.PI * 1.04, Math.PI * 1.96);
      x.stroke();
      const pulse = 1 + 0.04 * Math.sin(t * 7);
      for (const sd of [-1, 1]) {
        x.save();
        x.translate(sd * rx * 1.0, ry * 0.02);
        x.scale(pulse, pulse);
        x.fillStyle = DARK;
        x.beginPath();
        x.roundRect(-R * 0.17, -R * 0.33, R * 0.34, R * 0.66, R * 0.15);
        x.fill();
        x.fillStyle = "#FF5C7A";
        x.beginPath();
        x.roundRect(-R * 0.07 - sd * R * 0.04, -R * 0.2, R * 0.14, R * 0.4, R * 0.07);
        x.fill();
        x.restore();
      }
      break;
    }
    case "nightcap":
      softCap(x, R, rx, ry, t, "#4F46E5", "#F4F5F9");
      break;
    case "santaHat":
      softCap(x, R, rx, ry, t, "#DC2626", "#FFFFFF");
      break;
    case "partyHat": {
      const baseY = -ry * 0.82;
      const w = rx * 0.4;
      const tipY = -ry * 1.75;
      x.fillStyle = "#F59E0B";
      x.beginPath();
      x.moveTo(-w, baseY);
      x.lineTo(0, tipY);
      x.lineTo(w, baseY);
      x.closePath();
      x.fill();
      // Stripes, clipped to the cone.
      x.save();
      x.clip();
      x.strokeStyle = "#FFF6D6";
      x.lineWidth = R * 0.1;
      for (let i = 0; i < 3; i++) {
        const y = baseY - (i + 0.6) * (baseY - tipY) * 0.26;
        x.beginPath();
        x.moveTo(-w, y + R * 0.18);
        x.lineTo(w, y - R * 0.18);
        x.stroke();
      }
      x.restore();
      x.fillStyle = "#F43F5E";
      x.beginPath();
      x.arc(0, tipY, R * 0.1, 0, Math.PI * 2);
      x.fill();
      break;
    }
    case "witchHat": {
      const baseY = -ry * 0.78;
      const tipY = -ry * 1.7;
      // Brim first, then the cone with its tip bent over, then the band.
      x.fillStyle = "#1F1B2E";
      x.beginPath();
      x.ellipse(0, baseY, rx * 0.95, R * 0.15, 0, 0, Math.PI * 2);
      x.fill();
      x.beginPath();
      x.moveTo(-rx * 0.5, baseY);
      x.quadraticCurveTo(-rx * 0.2, tipY * 0.8, rx * 0.34, tipY);
      x.quadraticCurveTo(rx * 0.2, tipY * 0.85, rx * 0.5, baseY);
      x.closePath();
      x.fill();
      x.fillStyle = "#7C3AED";
      x.beginPath();
      x.roundRect(-rx * 0.52, baseY - R * 0.2, rx * 1.04, R * 0.2, R * 0.05);
      x.fill();
      x.fillStyle = "#FBBF24";
      x.beginPath();
      x.roundRect(-R * 0.1, baseY - R * 0.19, R * 0.2, R * 0.18, R * 0.03);
      x.fill();
      break;
    }
    case "bunnyEars": {
      for (const sd of [-1, 1]) {
        x.save();
        x.translate(sd * rx * 0.4, -ry * 1.2);
        x.rotate(sd * 0.14);
        x.fillStyle = "#EDEDEF";
        x.beginPath();
        x.ellipse(0, 0, R * 0.2, R * 0.55, 0, 0, Math.PI * 2);
        x.fill();
        x.fillStyle = "#F9A8D4";
        x.beginPath();
        x.ellipse(0, R * 0.04, R * 0.1, R * 0.4, 0, 0, Math.PI * 2);
        x.fill();
        x.restore();
      }
      break;
    }
    case "sunglasses": {
      // Over the eyes, which sit a little below the middle.
      const y = ry * 0.1;
      const w = rx * 0.42;
      const hgt = R * 0.4;
      x.fillStyle = "rgba(10,12,20,0.92)";
      for (const sd of [-1, 1]) {
        x.beginPath();
        x.roundRect(sd * rx * 0.42 - w * 0.5, y - hgt * 0.5, w, hgt, R * 0.12);
        x.fill();
      }
      x.strokeStyle = "rgba(10,12,20,0.92)";
      x.lineWidth = R * 0.07;
      x.beginPath();
      x.moveTo(-rx * 0.42 + w * 0.5, y - hgt * 0.2);
      x.lineTo(rx * 0.42 - w * 0.5, y - hgt * 0.2);
      x.stroke();
      // A glint.
      x.strokeStyle = "rgba(255,255,255,0.35)";
      x.lineWidth = R * 0.04;
      for (const sd of [-1, 1]) {
        x.beginPath();
        x.moveTo(sd * rx * 0.42 - w * 0.25, y - hgt * 0.3);
        x.lineTo(sd * rx * 0.42 - w * 0.05, y - hgt * 0.05);
        x.stroke();
      }
      break;
    }
    case "bow": {
      // At the top left, a little off the body's edge.
      x.translate(-rx * 0.5, -ry * 0.92);
      x.rotate(-0.35);
      x.fillStyle = "#F43F5E";
      for (const sd of [-1, 1]) {
        x.beginPath();
        x.moveTo(0, 0);
        x.quadraticCurveTo(sd * R * 0.3, -R * 0.32, sd * R * 0.42, 0);
        x.quadraticCurveTo(sd * R * 0.3, R * 0.32, 0, 0);
        x.fill();
      }
      x.fillStyle = "#BE123C";
      x.beginPath();
      x.arc(0, 0, R * 0.1, 0, Math.PI * 2);
      x.fill();
      break;
    }
  }
  x.restore();
}
