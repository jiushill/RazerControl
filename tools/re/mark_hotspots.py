# -*- coding: utf-8 -*-
"""Draw a crop rectangle and button hotspots on a product image for calibration."""
import sys

from PIL import Image, ImageDraw

CROP = (0.355, 0.110, 0.650, 0.918)
HOTSPOTS = [
    ('L', 0.447, 0.226),
    ('R', 0.560, 0.214),
    ('M', 0.495, 0.238),
    ('B', 0.408, 0.387),
    ('F', 0.411, 0.452),
]


def main():
    src = sys.argv[1]
    dst = sys.argv[2]
    im = Image.open(src).convert('RGBA')
    w, h = im.size
    d = ImageDraw.Draw(im)
    d.rectangle(
        (CROP[0] * w, CROP[1] * h, CROP[2] * w, CROP[3] * h),
        outline=(0, 255, 255, 255), width=2)
    for name, x, y in HOTSPOTS:
        cx, cy = x * w, y * h
        d.ellipse((cx - 12, cy - 12, cx + 12, cy + 12),
                  outline=(255, 0, 0, 255), width=3)
        d.text((cx + 14, cy - 6), name, fill=(255, 0, 0, 255))
    im.resize((1500, 840)).save(dst)


if __name__ == '__main__':
    main()
