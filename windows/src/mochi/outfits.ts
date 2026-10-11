// Mochi's accessories: little things drawn on him in code (like everything else about him), put
// on automatically from what is going on — headphones while music plays, a night cap when he
// sleeps, shades while a focus period runs — or chosen in Settings. Mini bots never wear them.

export type Outfit = "headphones" | "nightcap" | "party" | "shades" | "bow";

/** Settings value: "auto" lets the rules below pick, "none" keeps him bare. */
export type OutfitSetting = "auto" | "none" | Outfit;

export const OUTFIT_CHOICES: readonly { value: OutfitSetting; label: string }[] = [
  { value: "auto", label: "Automatic" },
  { value: "none", label: "None" },
  { value: "headphones", label: "Headphones" },
  { value: "nightcap", label: "Night cap" },
  { value: "party", label: "Party hat" },
  { value: "shades", label: "Shades" },
  { value: "bow", label: "Bow" },
];

export interface OutfitContext {
  sleeping: boolean;
  musicPlaying: boolean;
  focusing: boolean;
}

/** What he wears right now. Pure, so it can be tested. */
export function pickOutfit(setting: OutfitSetting | string | undefined, ctx: OutfitContext): Outfit | null {
  if (setting === "none") return null;
  if (setting && setting !== "auto") {
    return OUTFIT_CHOICES.some((c) => c.value === setting) ? (setting as Outfit) : null;
  }
  if (ctx.sleeping) return "nightcap";
  if (ctx.focusing) return "shades";
  if (ctx.musicPlaying) return "headphones";
  return null;
}

const DARK = "#2B2E38";

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
    case "nightcap": {
      const baseY = -ry * 0.74;
      const w = rx * 0.62;
      x.fillStyle = "#4F46E5";
      x.beginPath();
      x.moveTo(-w, baseY);
      x.quadraticCurveTo(-w * 0.5, -ry * 1.55, rx * 0.9, -ry * 1.05 + Math.sin(t * 1.3) * R * 0.03);
      x.quadraticCurveTo(rx * 0.35, -ry * 1.0, w, baseY);
      x.closePath();
      x.fill();
      // White rim and the pompom at the tip.
      x.fillStyle = "#F4F5F9";
      x.beginPath();
      x.roundRect(-w - R * 0.03, baseY - R * 0.09, w * 2 + R * 0.06, R * 0.2, R * 0.1);
      x.fill();
      x.beginPath();
      x.arc(rx * 0.9, -ry * 1.05 + Math.sin(t * 1.3) * R * 0.03 + R * 0.04, R * 0.13, 0, Math.PI * 2);
      x.fill();
      break;
    }
    case "party": {
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
    case "shades": {
      // Over the eyes, which sit a little above the middle.
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
      const cx = -rx * 0.5;
      const cy = -ry * 0.92;
      x.translate(cx, cy);
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
