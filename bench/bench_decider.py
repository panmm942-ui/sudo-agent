"""GLiNER2.5-multi-Decide router benchmark (model run, not a pytest test).

Run from core/:  uv run python ../bench/bench_decider.py [--threads 4] [--model ../models/decider]
Writes bench/results/decider.json: per label variant (en | en_el) accuracy overall / per lang, latency stats,
top confusion pairs and a precision-vs-threshold sweep; plus an extract_entities slot-extraction probe.
"""

from __future__ import annotations

import argparse
import json
import platform
from collections import Counter, defaultdict
from pathlib import Path

from sudo_core.brain.decider import Decider
from sudo_core.perf import Stopwatch, summarize

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "bench" / "fixtures"
RESULTS = ROOT / "bench" / "results"
DEFAULT_MODEL = ROOT / "models" / "decider"
WARMUP_TEXTS = ["pause the music", "set the volume to 40", "what is the weather like", "άνοιξε το chrome", "next"]
SLOT_ENTITY_TYPES = ["app", "number", "time", "query", "url"]
THRESHOLDS = [round(0.30 + 0.05 * i, 2) for i in range(14)]  # 0.30 ... 0.95

GATES = {"overall": 0.85, "el": 0.80, "p95_ms": 80.0, "precision": 0.97}


def load_fixtures() -> tuple[dict, list[dict]]:
    intents = json.loads((FIXTURES / "intents.json").read_text(encoding="utf-8"))
    rows = [json.loads(line) for line in (FIXTURES / "commands.jsonl").read_text(encoding="utf-8").splitlines() if line.strip()]
    return intents, rows


def label_variants(intents: dict) -> dict[str, tuple[str, dict[str, str]]]:
    """name -> (Decider label_mode, {intent id: description}). en / en_el use the brief's dict form;
    the *_flat variants send the same descriptions as a flat label list."""
    en = {k: v["en"] for k, v in intents.items()}
    en_el = {k: f'{v["en"]} / {v["el"]}' for k, v in intents.items()}
    return {
        "en": ("dict", en),
        "en_el": ("dict", en_el),
        "en_flat": ("descriptions", en),
        "en_el_flat": ("descriptions", en_el),
    }


def sweep(preds: list[dict]) -> dict:
    rows = []
    for t in THRESHOLDS:
        acc = [p for p in preds if p["score"] >= t]
        correct = sum(p["pred"] == p["gold"] for p in acc)
        rows.append({
            "threshold": t,
            "accepted": len(acc),
            "accepted_share": len(acc) / len(preds),
            "precision": correct / len(acc) if acc else None,
        })
    ok = [r for r in rows if r["precision"] is not None and r["precision"] >= GATES["precision"]]
    return {"rows": rows, "at_97_precision": ok[0] if ok else None}


def run_variant(decider: Decider, rows: list[dict], labels: dict[str, str]) -> dict:
    preds, lats = [], []
    for r in rows:
        d = decider.decide(r["text"], labels)
        preds.append({"id": r["id"], "lang": r["lang"], "gold": r["intent"], "pred": d.intent, "score": d.score})
        lats.append(d.latency_ms)
    per_lang: dict[str, list[bool]] = defaultdict(list)
    for p in preds:
        per_lang[p["lang"]].append(p["pred"] == p["gold"])
    st = summarize(lats)
    confusions = Counter((p["gold"], p["pred"]) for p in preds if p["gold"] != p["pred"])
    return {
        "n_labels": len(labels),
        "accuracy": {
            "overall": sum(p["pred"] == p["gold"] for p in preds) / len(preds),
            **{lang: sum(v) / len(v) for lang, v in sorted(per_lang.items())},
        },
        "latency_ms": {"n": st.n, "mean": st.mean, "p50": st.p50, "p95": st.p95, "max": max(lats)},
        "top_confusions": [{"gold": g, "pred": p, "count": c} for (g, p), c in confusions.most_common(10)],
        "sweep": sweep(preds),
        "predictions": preds,
    }


def slot_probe(extractor, rows: list[dict], threshold: float) -> dict:
    lats, hits, total = [], 0, 0
    for r in rows:
        with Stopwatch() as sw:
            ents = extractor.extract_entities(r["text"], SLOT_ENTITY_TYPES, threshold=threshold)
        lats.append(sw.ms)
        found = [str(s).lower() for vals in ents.get("entities", {}).values() for s in vals]
        for value in r["slots"].values():
            total += 1
            v = str(value).lower()
            hits += any(v in f or f in v for f in found if f)
    st = summarize(lats)
    return {
        "threshold": threshold,
        "latency_ms": {"n": st.n, "mean": st.mean, "p50": st.p50, "p95": st.p95},
        "slots_total": total,
        "slots_hit": hits,
        "hit_rate": hits / total if total else None,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--model", default=str(DEFAULT_MODEL))
    ap.add_argument("--out", default=str(RESULTS / "decider.json"))
    ap.add_argument("--skip-slots", action="store_true")
    args = ap.parse_args()

    import torch

    intents, rows = load_fixtures()
    with Stopwatch() as load:
        decider = Decider(args.model, device="cpu", num_threads=args.threads)
    print(f"loaded in {load.ms:.0f} ms (threads={torch.get_num_threads()})")
    variants = label_variants(intents)
    deciders = {
        mode: Decider(extractor=decider._extractor, label_mode=mode) for mode in ("dict", "descriptions")
    }

    out: dict = {
        "model": args.model,
        "device": "cpu",
        "torch_threads": torch.get_num_threads(),
        "torch": torch.__version__,
        "platform": platform.platform(),
        "load_ms": load.ms,
        "n_commands": len(rows),
        "gates": GATES,
        "variants": {},
    }
    for name, (mode, labels) in variants.items():
        for t in WARMUP_TEXTS:
            deciders[mode].decide(t, labels)
        res = run_variant(deciders[mode], rows, labels)
        res["label_mode"] = mode
        out["variants"][name] = res
        a, lat, sw = res["accuracy"], res["latency_ms"], res["sweep"]["at_97_precision"]
        print(f"[{name}] acc={a} p50={lat['p50']:.1f} p95={lat['p95']:.1f} ms  @97%prec={sw}")

    if not args.skip_slots:
        out["slot_probe"] = {f"threshold_{t}": slot_probe(decider._extractor, rows, t) for t in (0.5, 0.3)}
        print("slot probe:", json.dumps(out["slot_probe"]))

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(out, indent=2, ensure_ascii=False), encoding="utf-8")
    print("wrote", out_path)


if __name__ == "__main__":
    main()
