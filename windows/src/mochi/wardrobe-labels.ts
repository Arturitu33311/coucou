// The wardrobe's names and texts, in the current language (upstream's wardrobe API, see
// outfits.ts). Kept apart from outfits.ts so what the VGA panel bundles carries no translations.

import { N_, labels, t } from "../i18n/i18n";
import { seasonalOutfit, type OutfitSelection } from "./outfits";

/** The English names, the keys of their translations (src/i18n). */
export const OUTFIT_KEYS: Record<OutfitSelection, string> = {
  auto: N_("Automatic (seasons, music, focus)"),
  none: N_("None"),
  partyHat: N_("Party hat"),
  beanie: N_("Beanie"),
  crown: N_("Crown"),
  sunglasses: N_("Sunglasses"),
  roundGlasses: N_("Round glasses"),
  bow: N_("Bow"),
  scarf: N_("Scarf"),
  witchHat: N_("Witch hat"),
  pumpkin: N_("Pumpkin"),
  santaHat: N_("Santa hat"),
  bunnyEars: N_("Bunny ears"),
  headphones: N_("Headphones"),
  nightcap: N_("Night cap"),
  necktie: N_("Necktie"),
  monocle: N_("Monocle"),
  headband: N_("Headband"),
  earmuffs: N_("Earmuffs"),
  bandana: N_("Bandana"),
  wizardHat: N_("Wizard hat"),
};

/** In the current language: every read goes through `t()`. */
export const OUTFIT_LABELS: Record<OutfitSelection, string> = labels(OUTFIT_KEYS);

export const WARDROBE_STRINGS = {
  get title() { return t("Wardrobe"); },
  get autoBadge() { return t("AUTO"); },
  autoNow: (current: string) => t("Auto · {outfit}", { outfit: current }),
  autoHover: (current: string) => t("Auto · follows the seasons (now: {outfit})", { outfit: current }),
};

/** The grey text at the right of the wardrobe header: the hovered outfit, otherwise the choice. */
export function wardrobeHeader(hovered: OutfitSelection | null, selection: OutfitSelection, date: Date): string {
  const season = OUTFIT_LABELS[seasonalOutfit(date)];
  if (hovered) return hovered === "auto" ? WARDROBE_STRINGS.autoHover(season) : OUTFIT_LABELS[hovered];
  return selection === "auto" ? WARDROBE_STRINGS.autoNow(season) : OUTFIT_LABELS[selection];
}
