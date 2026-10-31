#!/usr/bin/env python3
"""Cut a car out of a photo for the Coucou Linux wallpaper (the car goes in front of the clock).

    python3 cut_car.py original.jpg car_cut.png --rect 0.16,0.45,0.92,0.78 [--width 2400]

--rect is x0,y0,x1,y1 as fractions of the photo, loosely around the car. GrabCut separates it from the background; the
result is a cropped RGBA PNG with a feathered edge. Needs opencv-python (cv2) and numpy. The photo itself is not part of
the repo (it has its own licence): keep it, and its credit, outside it.
"""
import argparse

import cv2
import numpy as np


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--rect", default="0.16,0.45,0.92,0.78")
    ap.add_argument("--width", type=int, default=2400)
    ap.add_argument("--iters", type=int, default=8)
    ap.add_argument("--fg", action="append", default=[], help="x0,y0,x1,y1 (fractions): surely the car")
    ap.add_argument("--bg", action="append", default=[], help="x0,y0,x1,y1 (fractions): surely not the car")
    a = ap.parse_args()

    img = cv2.imread(a.src)
    h0, w0 = img.shape[:2]
    k = a.width / w0
    img = cv2.resize(img, (a.width, int(h0 * k)), interpolation=cv2.INTER_AREA)
    h, w = img.shape[:2]
    x0, y0, x1, y1 = [float(v) for v in a.rect.split(",")]
    box = lambda r: (int(r[0] * w), int(r[1] * h), int(r[2] * w), int(r[3] * h))

    mask = np.full((h, w), cv2.GC_BGD, np.uint8)
    bx0, by0, bx1, by1 = box((x0, y0, x1, y1))
    mask[by0:by1, bx0:bx1] = cv2.GC_PR_FGD
    for r in a.fg:
        fx0, fy0, fx1, fy1 = box([float(v) for v in r.split(",")])
        mask[fy0:fy1, fx0:fx1] = cv2.GC_FGD
    for r in a.bg:
        gx0, gy0, gx1, gy1 = box([float(v) for v in r.split(",")])
        mask[gy0:gy1, gx0:gx1] = cv2.GC_BGD

    bgd, fgd = np.zeros((1, 65), np.float64), np.zeros((1, 65), np.float64)
    cv2.grabCut(img, mask, None, bgd, fgd, a.iters, cv2.GC_INIT_WITH_MASK)
    fg = np.where((mask == cv2.GC_FGD) | (mask == cv2.GC_PR_FGD), 255, 0).astype(np.uint8)

    # keep the biggest piece, fill holes, smooth the edge
    n, lab, stats, _ = cv2.connectedComponentsWithStats(fg, 8)
    if n > 1:
        fg = np.where(lab == 1 + np.argmax(stats[1:, cv2.CC_STAT_AREA]), 255, 0).astype(np.uint8)
    cnts, _ = cv2.findContours(fg, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    filled = np.zeros_like(fg)
    cv2.drawContours(filled, cnts, -1, 255, cv2.FILLED)
    filled = cv2.morphologyEx(filled, cv2.MORPH_OPEN, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (5, 5)))
    alpha = cv2.GaussianBlur(filled, (0, 0), 1.6)

    rgba = cv2.cvtColor(img, cv2.COLOR_BGR2BGRA)
    rgba[:, :, 3] = alpha
    ys, xs = np.where(alpha > 8)
    pad = 6
    rgba = rgba[max(ys.min() - pad, 0):ys.max() + pad, max(xs.min() - pad, 0):xs.max() + pad]
    cv2.imwrite(a.dst, rgba)
    print("cut-out", rgba.shape[1], "x", rgba.shape[0])


if __name__ == "__main__":
    main()
