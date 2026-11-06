"""Resamples record-gif.mjs screencast frames onto a fixed 20 fps timeline and writes a GIF.

Usage: python scripts/frames-to-gif.py <framesDir> <out.gif> [--fps 20] [--lead 0.4] [--max-mb 6]
Each output tick shows the latest screencast frame at or before it (the compositor only emits a
frame when something changed), all quantized to one shared palette so the static backdrop does
not shimmer and Pillow's inter-frame diffing keeps the file small.
"""
import argparse
import json
import os

from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("frames")
ap.add_argument("out")
ap.add_argument("--fps", type=float, default=20)
ap.add_argument("--lead", type=float, default=0.4)
ap.add_argument("--max-mb", type=float, default=6)
a = ap.parse_args()

meta = json.load(open(os.path.join(a.frames, "frames.json")))
frames = meta["frames"]
t0 = meta["tKey"] - a.lead
t1 = meta["tEnd"]
step = 1 / a.fps
ticks = [t0 + i * step for i in range(int((t1 - t0) / step) + 1)]

timeline, j = [], 0
for t in ticks:
    while j + 1 < len(frames) and frames[j + 1]["t"] <= t:
        j += 1
    timeline.append(frames[j]["file"] if frames and frames[j]["t"] <= t else frames[0]["file"])
timeline[-1] = meta["last"]

cache = {}


def load(f):
    if f not in cache:
        cache[f] = Image.open(os.path.join(a.frames, f)).convert("RGB")
    return cache[f]


# Shared palette from a mosaic of frames spread over the run (backdrop + every state's look).
sample = [load(timeline[int(k * (len(timeline) - 1) / 7)]) for k in range(8)]
w, h = sample[0].size
mosaic = Image.new("RGB", (w * 4, h * 2))
for k, im in enumerate(sample):
    mosaic.paste(im, ((k % 4) * w, (k // 4) * h))
palette = mosaic.quantize(colors=255, method=Image.Quantize.MEDIANCUT)

out = []
for f in timeline:
    out.append(load(f).quantize(palette=palette, dither=Image.Dither.NONE))

out[0].save(a.out, save_all=True, append_images=out[1:], duration=int(1000 / a.fps), loop=0, optimize=False, disposal=1)
mb = os.path.getsize(a.out) / 2**20
uniq = len(set(timeline))
print(f"{a.out}: {len(out)} frames @ {a.fps:g} fps ({len(out) / a.fps:.1f} s), {uniq} distinct source frames, {mb:.2f} MB")
if mb > a.max_mb:
    print(f"WARNING: over {a.max_mb} MB")
