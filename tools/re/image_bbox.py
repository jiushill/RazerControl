# -*- coding: utf-8 -*-
"""Compute the non-white bounding box and alpha stats of a product image."""
import sys

from PIL import Image


def main():
    path = sys.argv[1]
    im = Image.open(path).convert('RGBA')
    w, h = im.size
    px = im.load()
    xs, ys = [], []
    alphas = set()
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            alphas.add(a)
            mx, mn = max(r, g, b), min(r, g, b)
            if not (mx >= 248 and mx - mn <= 6):
                xs.append(x)
                ys.append(y)
    x0, y0, x1, y1 = min(xs), min(ys), max(xs), max(ys)
    print('%s size=%dx%d alpha=%s' % (path, w, h, sorted(alphas)[:8]))
    print('bbox px: %d,%d - %d,%d' % (x0, y0, x1, y1))
    print('bbox normalized: %.4f, %.4f, %.4f, %.4f' % (
        x0 / w, y0 / h, (x1 + 1) / w, (y1 + 1) / h))


if __name__ == '__main__':
    main()
