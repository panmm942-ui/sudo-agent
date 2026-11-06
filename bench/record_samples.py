"""Record the 40 voice samples for the STT bench (interactive; run on the target laptop).

Run from core/:  uv run python ../bench/record_samples.py [--audio-dir DIR] [--device N]
Writes <audio-dir>/<fixture id>.wav (16 kHz mono) and <audio-dir>/refs.jsonl (default dir: bench/audio).
Re-running resumes: clips already in refs.jsonl are skipped.
"""

from __future__ import annotations

import argparse
import json
import random
import re
from collections import defaultdict
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "bench" / "fixtures" / "commands.jsonl"
DEFAULT_AUDIO_DIR = ROOT / "bench" / "audio"
SAMPLE_RATE = 16000
MAX_SECONDS = 6.0
SEED = 7
PER_GROUP = 20


def is_silent(audio: np.ndarray, threshold_dbfs: float = -50.0) -> bool:
    """True if the clip's RMS level is below threshold_dbfs (full scale = 1.0) or the clip is empty."""
    x = np.asarray(audio, dtype=np.float64)
    if x.size == 0:
        return True
    rms = float(np.sqrt(np.mean(x**2)))
    if rms <= 0.0:
        return True
    return bool(20.0 * np.log10(rms) < threshold_dbfs)


def _stratified(rows: list[dict], n: int, rng: random.Random) -> list[dict]:
    by_intent: dict[str, list[dict]] = defaultdict(list)
    for r in sorted(rows, key=lambda r: r["id"]):
        by_intent[r["intent"]].append(r)
    for v in by_intent.values():
        rng.shuffle(v)
    intents = sorted(by_intent)
    rng.shuffle(intents)
    picked: list[dict] = []
    while len(picked) < n and any(by_intent.values()):  # round-robin across intents
        for intent in intents:
            if by_intent[intent] and len(picked) < n:
                picked.append(by_intent[intent].pop())
    return picked


_NOT_SPOKEN = [
    re.compile(r"\d"),  # digits: "30" may come back as "thirty", "6:30" as "six thirty"
    re.compile(r"https?://|www\.|\b[a-z0-9-]+\.[a-z]{2,}\b", re.IGNORECASE),  # URLs / domains: "youtube dot com"
    re.compile(r"\b[AaPp]\.[Mm]\b|\b(?:AM|PM|pm)\b"),  # AM/PM, case-sensitive so "am" the verb is kept
]


def is_spoken_form(text: str) -> bool:
    """True if the text reads one way only. Number/URL/time formatting is a Phase 3 text-normalization concern,
    not an acoustic one, so the STT references avoid it rather than scoring "10 AM" against "ten a.m."."""
    return not any(p.search(text) for p in _NOT_SPOKEN)


def select_fixtures(rows: list[dict]) -> list[dict]:
    """40 clips: 20 `en` + 20 `el`/`mixed`, spoken-form only (see is_spoken_form), stratified by intent
    (intents without eligible fixtures are refilled from the rest), deterministic (seed 7, input-order independent)."""
    rng = random.Random(SEED)
    eligible = [r for r in rows if is_spoken_form(r["text"])]
    en = _stratified([r for r in eligible if r["lang"] == "en"], PER_GROUP, rng)
    other = _stratified([r for r in eligible if r["lang"] in ("el", "mixed")], PER_GROUP, rng)
    return en + other


def pick_device(sd) -> int | None:
    inputs = [(i, d) for i, d in enumerate(sd.query_devices()) if d["max_input_channels"] > 0]
    default = sd.default.device[0]
    for i, d in inputs:
        print(f"  [{i}] {d['name']}{'  (default)' if i == default else ''}")
    ans = input("Input device number (Enter = default): ").strip()
    return int(ans) if ans else None


def capture(read_block, enter_pressed, now, max_seconds: float = MAX_SECONDS) -> np.ndarray:
    """Collect blocks from read_block() until enter_pressed() is true or max_seconds elapse on now().

    Single-threaded: no helper thread outlives the clip, so a capped clip leaves stdin untouched.
    """
    blocks: list[np.ndarray] = []
    start = now()
    while not enter_pressed() and now() - start < max_seconds:
        blocks.append(np.asarray(read_block(), dtype=np.float32).reshape(-1))
    return np.concatenate(blocks) if blocks else np.zeros(0, dtype=np.float32)


def _enter_pressed() -> bool:
    """Non-blocking check for an Enter key press (Windows console); drains any other pending keys."""
    import msvcrt

    pressed = False
    while msvcrt.kbhit():
        if msvcrt.getwch() in ("\r", "\n"):
            pressed = True
    return pressed


def record_clip(sd, device: int | None) -> np.ndarray:
    """Record on Enter until Enter (max MAX_SECONDS). Returns float32 mono at 16 kHz."""
    import time

    input("  Press Enter to start recording... ")
    print("  Recording - press Enter to stop (max 6 s)... ", flush=True)
    block = 1600
    with sd.InputStream(samplerate=SAMPLE_RATE, channels=1, dtype="float32", device=device) as stream:
        _enter_pressed()  # discard keys typed before recording started
        return capture(lambda: stream.read(block)[0], _enter_pressed, time.monotonic)


def main() -> None:
    import sounddevice as sd
    import soundfile as sf

    ap = argparse.ArgumentParser()
    ap.add_argument("--audio-dir", type=Path, default=DEFAULT_AUDIO_DIR)
    ap.add_argument("--device", type=int, default=None, help="input device index (skips the prompt)")
    args = ap.parse_args()

    rows = [json.loads(l) for l in FIXTURES.read_text(encoding="utf-8").splitlines() if l.strip()]
    chosen = select_fixtures(rows)
    args.audio_dir.mkdir(parents=True, exist_ok=True)
    refs_path = args.audio_dir / "refs.jsonl"
    done = {json.loads(l)["id"] for l in refs_path.read_text(encoding="utf-8").splitlines() if l.strip()} \
        if refs_path.exists() else set()

    device = args.device if args.device is not None else pick_device(sd)
    print(f"\n{len(chosen) - len(done)} clips to record. Speak naturally, read the text as you would say it.")
    print("Type r + Enter at the 'Keep?' prompt to redo a clip.\n")

    for k, r in enumerate(chosen, 1):
        if r["id"] in done:
            continue
        while True:
            print(f"[{k}/{len(chosen)}] ({r['lang']}) {r['text']}")
            audio = record_clip(sd, device)
            if is_silent(audio):
                print("  Clip is silent - try again.\n")
                continue
            print(f"  Recorded {len(audio) / SAMPLE_RATE:.1f} s.")
            if input("  Keep? (Enter = yes, r = redo): ").strip().lower() == "r":
                continue
            break
        sf.write(args.audio_dir / f"{r['id']}.wav", audio, SAMPLE_RATE, subtype="PCM_16")
        with refs_path.open("a", encoding="utf-8") as f:
            f.write(json.dumps({"id": r["id"], "lang": r["lang"], "intent": r["intent"], "text": r["text"]},
                               ensure_ascii=False) + "\n")
    print("\nDone. Next: uv run python ../bench/bench_stt.py")


if __name__ == "__main__":
    main()
