"""Converts the record.mjs video into the README GIF with FFmpeg's two-pass palette.

Usage: python scripts/readme-demo/to-gif.py <outDir> <out.gif> [--fps 10] [--width 600] [--crop w:h:x:y]
FFmpeg is taken from $FFMPEG or PATH.
"""
import argparse, json, os, shutil, subprocess

ap = argparse.ArgumentParser()
ap.add_argument("dir"); ap.add_argument("out")
ap.add_argument("--fps", type=int, default=10); ap.add_argument("--width", type=int, default=600)
ap.add_argument("--crop", default="520:236:100:0", help="FFmpeg crop w:h:x:y in recorded pixels")
a = ap.parse_args()
t = json.load(open(os.path.join(a.dir, "timing.json")))
ffmpeg = os.environ.get("FFMPEG") or shutil.which("ffmpeg")
if not ffmpeg:
    raise SystemExit("FFmpeg not found: set FFMPEG or add it to PATH.")
flt = f"crop={a.crop},fps={a.fps},scale={a.width}:-1:flags=lanczos"
subprocess.run([ffmpeg, "-y", "-loglevel", "error", "-ss", f"{t['tStart']:.2f}", "-to", f"{t['tEnd']:.2f}", "-i", t["video"],
                "-vf", f"{flt},split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
                "-loop", "0", a.out], check=True)
print(f"{a.out}: {os.path.getsize(a.out) / 2**20:.2f} MB")
