#!/usr/bin/env python3
"""
Reads the frames of a screen recording of the island opening and closing and says whether it moved the way
it should: no frame where it vanished, and no sideways excursion past where it rests.

Usage: island_motion_check.py FRAMES_DIR [--tolerance PX]

FRAMES_DIR holds f0001.png, f0002.png... cropped to the top of the screen (the script that records them is
island-motion-check.sh). The island is the black shape on the wallpaper: for every frame the extent of the
dark pixels in the top band is measured, and the sequence is judged.

What it protects against (both happened, see COUCOU.md "Window"):
  * the pill flashing at the screen edge for a frame or two when the window changed size;
  * the pill blinking out while it did.
"""
import glob
import sys

from PIL import Image

DARK = 45        # luminance below which a pixel is "island"
MIN_RUN = 20     # a run narrower than this is not the pill (a notch of clock text, an icon)


def open_fraction(img):
    """How much of the crop is island: a card taking the top of the screen reads as large."""
    px = img.load()
    w, h = img.size
    n = sum(1 for y in range(0, h, 4) for x in range(0, w, 4) if px[x, y] < DARK)
    return n / float((w // 4 + 1) * (h // 4 + 1))


def runs_of(img):
    px = img.load()
    w, h = img.size
    cols = [x for x in range(w) if any(px[x, y] < DARK for y in range(int(h * 0.18), int(h * 0.62), 3))]
    out, start, prev = [], None, None
    for x in cols:
        if start is None:
            start = prev = x
        elif x - prev <= 6:
            prev = x
        else:
            out.append((start, prev))
            start = prev = x
    if start is not None:
        out.append((start, prev))
    return [r for r in out if r[1] - r[0] >= MIN_RUN]


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    folder = argv[1]
    tol = int(argv[argv.index('--tolerance') + 1]) if '--tolerance' in argv else 48
    frames = sorted(glob.glob(folder + '/f*.png'))
    if not frames:
        print('no frames in', folder)
        return 2
    seq = []
    OPEN = 'open'
    for f in frames:
        img = Image.open(f).convert('L')
        if open_fraction(img) > 0.30:
            seq.append(OPEN)  # the card is out: not a pill to measure
            continue
        r = runs_of(img)
        seq.append((min(a for a, _ in r), max(b for _, b in r)) if r else None)

    present = [s for s in seq if s and s != OPEN]
    if not present:
        print('FAIL: the island is in no frame')
        return 1
    rest_left = present[0][0]
    rest_right = present[0][1]
    end_left, end_right = present[-1]
    lo = min(rest_left, end_left) - tol
    hi = max(rest_right, end_right) + tol

    problems = []
    first = next(i for i, s in enumerate(seq) if s and s != OPEN)
    last = max(i for i, s in enumerate(seq) if s and s != OPEN)
    for i, s in enumerate(seq[first:last + 1], start=first):
        if s == OPEN:
            continue
        if s is None:
            problems.append('frame %d: the island is gone' % (i + 1))
        elif s[0] < lo or s[1] > hi:
            problems.append('frame %d: the island is at %d..%d, past %d..%d' % (i + 1, s[0], s[1], lo, hi))

    # While open the card is wider than the pill (that is not an excursion); what is judged is the
    # collapsed island before and after: it must come back to where it was.
    drift = abs(end_left - rest_left)
    if drift > tol:
        problems.append('it rests %d px from where it started' % drift)

    print('frames: %d  island: first %s  last %s' % (len(seq), present[0], present[-1]))
    if problems:
        for p in problems[:12]:
            print('FAIL:', p)
        return 1
    print('OK: no frame without the island, no excursion, back where it started')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
