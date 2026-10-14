// Entry of the VGA panel's wardrobe bundle: the SAME drawing code as the PC's Mochi (windows/src/mochi/
// wardrobe.ts, a port of the Mac's MochiOutfitDrawing.swift), exposed on window for the panel's page.
import { drawOutfitBehind, drawOutfitFront, mochiH, outfitFollowsRoll } from "../../windows/src/mochi/wardrobe";
import { seasonal } from "../../windows/src/mochi/outfits";

(window as unknown as Record<string, unknown>).MochiWardrobe = {
  drawOutfitBehind,
  drawOutfitFront,
  mochiH,
  outfitFollowsRoll,
  seasonal,
};
