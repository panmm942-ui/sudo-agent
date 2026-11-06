"""STT benchmark: Parakeet WER and latency on the user's own recordings (model run, not a pytest test).

Run from core/:  uv run python ../bench/bench_stt.py [--audio-dir DIR] [--devices cuda cpu] [--out FILE]
Needs bench/audio/refs.jsonl + wavs from bench/record_samples.py. Writes bench/results/stt.json.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path

import soundfile as sf

sys.path.insert(0, str(Path(__file__).resolve().parent))

from benchenv import provider_problem  # noqa: E402
from metrics import normalize, wer  # noqa: E402
from sudo_core.perf import Stopwatch, summarize  # noqa: E402
from sudo_core.stt.parakeet import ParakeetStt  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_AUDIO_DIR = ROOT / "bench" / "audio"
DEFAULT_OUT = ROOT / "bench" / "results" / "stt.json"
MODEL_DIR = ROOT / "models" / "stt"
WARMUPS = 3  # the first CUDA transcribe is a ~12 s cold init; warm-ups never enter the stats
GREEK_WER_GATE = 0.15


def bucket(seconds: float) -> str:
    return "<=2s" if seconds <= 2.0 else "2-4s" if seconds <= 4.0 else ">4s"


def lat_stats(lats: list[float]) -> dict:
    st = summarize(lats)
    return {"n": st.n, "mean": st.mean, "p50": st.p50, "p95": st.p95, "max": max(lats)}


def corpus_wer(pairs: list[tuple[str, str]]) -> float | None:
    """Corpus-level WER: total word errors / total reference words (not a mean of clip WERs)."""
    import jiwer

    pairs = [(normalize(r), normalize(h)) for r, h in pairs]
    pairs = [(r, h) for r, h in pairs if r]
    if not pairs:
        return None
    out = jiwer.process_words([r for r, _ in pairs], [h if h else "<empty>" for _, h in pairs])
    return (out.substitutions + out.deletions + out.insertions) / sum(len(r.split()) for r, _ in pairs)


def load_clips(audio_dir: Path) -> list[dict]:
    refs_path = audio_dir / "refs.jsonl"
    if not refs_path.exists():
        sys.exit("no recordings - run bench/record_samples.py first")
    refs = [json.loads(l) for l in refs_path.read_text(encoding="utf-8").splitlines() if l.strip()]
    if not refs:
        sys.exit("no recordings - run bench/record_samples.py first")
    clips = []
    for r in refs:
        audio, sr = sf.read(audio_dir / f"{r['id']}.wav", dtype="float32")
        clips.append({**r, "audio": audio, "sr": sr, "seconds": len(audio) / sr})
    return clips


def run_device(device: str, clips: list[dict]) -> dict:
    with Stopwatch() as load:
        stt = ParakeetStt(MODEL_DIR, device=device)
    provider = stt.active_provider
    problem = provider_problem(device, provider)  # onnxruntime silently falls back to CPU if CUDA fails to load
    if problem:
        print(f"[{device}] INVALID RUN: {problem}", file=sys.stderr)
    for i in range(WARMUPS):
        c = clips[i % len(clips)]
        stt.transcribe(c["audio"], c["sr"])
    rows = []
    for c in clips:
        t = stt.transcribe(c["audio"], c["sr"])
        rows.append({"id": c["id"], "lang": c["lang"], "seconds": c["seconds"], "ref": c["text"], "hyp": t.text,
                     "wer": wer(c["text"], t.text), "latency_ms": t.latency_ms})
    by_lang: dict[str, list[dict]] = defaultdict(list)
    by_bucket: dict[str, list[float]] = defaultdict(list)
    for r in rows:
        by_lang[r["lang"]].append(r)
        by_bucket[bucket(r["seconds"])].append(r["latency_ms"])
    greek = [r for r in rows if r["lang"] in ("el", "mixed")]
    pair = lambda rs: [(r["ref"], r["hyp"]) for r in rs]  # noqa: E731
    return {
        "device": device,
        "provider": provider,
        "valid": problem is None,  # False: latency is not a number for this device (report fails the check)
        **({"invalid_reason": problem} if problem else {}),
        "load_ms": load.ms,
        "warmups": WARMUPS,
        "wer": {
            "overall": corpus_wer(pair(rows)),
            "el_mixed": corpus_wer(pair(greek)),
            **{lang: corpus_wer(pair(rs)) for lang, rs in sorted(by_lang.items())},
        },
        "latency_ms": {"all": lat_stats([r["latency_ms"] for r in rows]),
                       **{b: lat_stats(v) for b, v in sorted(by_bucket.items())}},
        "worst_10": sorted(rows, key=lambda r: r["wer"], reverse=True)[:10],
        "transcripts": rows,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--audio-dir", type=Path, default=DEFAULT_AUDIO_DIR)
    ap.add_argument("--devices", nargs="+", default=["cuda", "cpu"], choices=["cuda", "cpu"])
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()

    clips = load_clips(args.audio_dir)
    result = {"engine": "parakeet-tdt-0.6b-v3", "n_clips": len(clips), "devices": {}}
    for device in args.devices:
        res = result["devices"][device] = run_device(device, clips)
        lat = res["latency_ms"]["all"]
        print(f"[{device}] provider={res['provider']} load={res['load_ms']:.0f}ms "
              f"p50={lat['p50']:.0f}ms p95={lat['p95']:.0f}ms wer={res['wer']}")
    el = next(iter(result["devices"].values()))["wer"]["el_mixed"]
    result["greek_wer_gate_exceeded"] = el is not None and el > GREEK_WER_GATE
    if result["greek_wer_gate_exceeded"]:
        print(f"Greek/mixed WER {el:.1%} > {GREEK_WER_GATE:.0%}: consider the faster-whisper fallback (plan Task 6 Step 7).")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
