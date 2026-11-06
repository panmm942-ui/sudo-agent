"""LLM tool-call benchmark (model run, not a pytest test).

Run from core/:  uv run python ../bench/bench_llm.py --config baseline [--mmproj]
Configs: baseline | draft | draft4 | draft8 | ngram | draft+ngram (see CONFIGS). Each run writes
bench/results/llm_<config>.json (flushed after every call) and merges a summary into
bench/results/llm_tuning.json.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path

import httpx

sys.path.insert(0, str(Path(__file__).resolve().parent))

from benchenv import configure_cuda_cache  # noqa: E402
from sudo_core.llm.client import LlmClient, ToolCallResult
from sudo_core.llm.server import LlamaServer, LlamaServerError
from sudo_core.llm.tools import SYSTEM_PROMPT, intents_to_tools, tool_name
from sudo_core.perf import summarize

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "bench" / "fixtures"
RESULTS = ROOT / "bench" / "results"
MODELS = ROOT / "models"
DRAFT = MODELS / "llm-draft" / "Qwen3.5-0.8B-Q4_K_M.gguf"
WARMUPS = ["pause the music", "set the volume to 40", "what is the weather like"]
MAX_TOKENS = 160

THRESHOLDS = {"total_p50": 700.0, "total_p95": 1000.0, "valid": 0.95, "correct": 0.90, "decode": 35.0}
VRAM_BUDGET_MIB = 3600

MMPROJ = MODELS / "llm" / "mmproj-F16.gguf"

# Keyword args for LlamaServer. The vision projector is opt-in via --mmproj (spec section 2 budgets it, but it
# OOMs on the 6 GB dev box); an mmproj run is stored under "<config>+mmproj".
CONFIGS: dict[str, dict] = {
    "baseline": {},
    "kv-f16": {"kv_type": "f16"},
    "draft": {"draft_model": DRAFT},
    "draft4": {"draft_model": DRAFT, "draft_max": 4},
    "draft8": {"draft_model": DRAFT, "draft_max": 8},
    "ngram": {"spec_ngram": True},
    "ngram-n3": {"spec_ngram": True, "ngram_args": ["--spec-ngram-simple-size-n", "3", "--spec-ngram-simple-size-m", "16"]},
    "ngram-mapk3": {
        "spec_ngram": True, "ngram_type": "ngram-map-k",
        "ngram_args": ["--spec-ngram-map-k-size-n", "3", "--spec-ngram-map-k-size-m", "16"],
    },
    "draft+ngram": {"draft_model": DRAFT, "spec_ngram": True},
}


def result_key(config: str, mmproj: bool) -> str:
    return f"{config}+mmproj" if mmproj else config


def gpu_used_mib() -> int | None:
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=10,
        ).stdout
        return int(out.strip().splitlines()[0])
    except (OSError, ValueError, IndexError, subprocess.SubprocessError):
        return None


def process_vram_mib(pid: int) -> int | None:
    """Per-process VRAM from nvidia-smi; under WDDM this is usually [N/A] (caller falls back to a delta)."""
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=10,
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return None
    for line in out.splitlines():
        parts = [p.strip() for p in line.split(",")]
        if len(parts) == 2 and parts[0] == str(pid) and parts[1].isdigit():
            return int(parts[1])
    return None


def messages_for(text: str) -> list[dict]:
    return [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": text}]


def slots_match(expected: dict, got: dict | None) -> bool:
    """Loose slot check: every expected key present, values equal after str-normalising."""
    if got is None:
        return False
    return all(k in got and str(got[k]).strip().lower() == str(v).strip().lower() for k, v in expected.items())


def stats_dict(samples: list[float]) -> dict | None:
    return summarize(samples).__dict__ if samples else None


def build_summary(config: str, srv: LlamaServer, samples: list[dict], per_lang: dict, extra: dict) -> dict:
    n = len(samples)
    ok = [s for s in samples if not s["error"]]  # errored calls have no latency: never count them as 0 ms
    slot_hits = [s["slots_ok"] for s in samples if s["slots_ok"] is not None]
    return {
        "config": config,
        "flags": srv.flags_description(),
        "mmproj": srv.mmproj.name if srv.mmproj else None,
        "max_tokens": MAX_TOKENS,
        "n": n,
        "stats_total": stats_dict([s["total_ms"] for s in ok]),
        "stats_ttft": stats_dict([s["ttft_ms"] for s in ok]),
        "stats_name_ms": stats_dict([s["name_ms"] for s in ok if s["name_ms"] is not None]),
        # errored calls count as invalid (and wrong) in the rates below
        "valid_rate": sum(s["valid"] for s in samples) / n if n else None,
        "invalid_calls": sum(not s["valid"] for s in samples),
        # correctness = tool name only; validity is reported separately
        "correct_rate": sum(s["correct"] for s in samples) / n if n else None,
        "slot_match_rate_of_correct": (sum(slot_hits) / len(slot_hits)) if slot_hits else None,
        "finish_reasons": dict(Counter(str(s["finish_reason"]) for s in samples)),
        "http_errors": sum(1 for s in samples if s["error"]),
        "per_lang": {
            lang: {"n": v["n"], "valid_rate": v["valid"] / v["n"], "correct_rate": v["correct"] / v["n"]}
            for lang, v in per_lang.items()
        },
        **extra,
    }


def merge_tuning(summary: dict) -> None:
    path = RESULTS / "llm_tuning.json"
    data = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    data[summary["config"]] = {k: v for k, v in summary.items() if k not in ("failures", "samples")}
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")


def run(config: str, mmproj: bool = False) -> None:
    intents = json.loads((FIXTURES / "intents.json").read_text(encoding="utf-8"))
    commands = [
        json.loads(line)
        for line in (FIXTURES / "commands.jsonl").read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    tools = intents_to_tools(intents)
    RESULTS.mkdir(parents=True, exist_ok=True)
    key = result_key(config, mmproj)
    out_path = RESULTS / f"llm_{key.replace('+', '_')}.json"

    configure_cuda_cache()
    idle_mib = gpu_used_mib()
    srv = LlamaServer(
        exe=MODELS / "llama.cpp" / "llama-server.exe",
        model=MODELS / "llm" / "Qwen3.5-4B-Q4_K_M.gguf",
        mmproj=MMPROJ if mmproj else None,
        **CONFIGS[config],
    )
    samples: list[dict] = []
    failures: list[dict] = []
    per_lang: dict[str, dict[str, int]] = defaultdict(lambda: {"n": 0, "valid": 0, "correct": 0})
    extra: dict = {"vram_idle_mib": idle_mib}

    def flush(done: bool) -> None:
        out = build_summary(key, srv, samples, per_lang, {**extra, "complete": done})
        out["failures"] = failures
        out["samples"] = samples
        out_path.write_text(json.dumps(out, indent=2, ensure_ascii=False), encoding="utf-8")
        if done:
            merge_tuning(out)

    try:
        t0 = time.perf_counter()
        srv.start(timeout_s=240)
        extra["startup_s"] = round(time.perf_counter() - t0, 1)
        with httpx.Client(timeout=httpx.Timeout(300.0)) as http:
            client = LlmClient(srv.base_url, http=http, max_tokens=MAX_TOKENS)
            try:
                for text in WARMUPS:
                    client.tool_call(messages_for(text), tools)
            except httpx.HTTPError as e:
                extra["server_crashed"] = f"{type(e).__name__} during warmup\n{srv.stderr_tail()[-3000:]}"
                print(f"[{config}] {extra['server_crashed']}", file=sys.stderr)

            for cmd in ([] if "server_crashed" in extra else commands):
                expected = tool_name(cmd["intent"])
                error = None
                try:
                    r = client.tool_call(messages_for(cmd["text"]), tools)
                except httpx.HTTPError as e:
                    error = f"{type(e).__name__}: {e}"
                    r = ToolCallResult(None, None, False, 0.0, 0.0, 0)  # latency fields not used (see below)
                correct = r.name == expected
                pl = per_lang[cmd["lang"]]
                pl["n"] += 1
                pl["valid"] += int(r.valid)
                pl["correct"] += int(correct)
                slots_ok = slots_match(cmd["slots"], r.arguments) if (cmd["slots"] and correct and r.valid) else None
                samples.append(
                    {
                        "id": cmd["id"],
                        "total_ms": None if error else round(r.total_ms, 1),
                        "ttft_ms": None if error else round(r.ttft_ms, 1),
                        "name_ms": round(r.name_ms, 1) if r.name_ms is not None and not error else None,
                        "tokens": r.completion_tokens,
                        "finish_reason": r.finish_reason,
                        "valid": r.valid,
                        "correct": correct,
                        "slots_ok": slots_ok,
                        "error": error,
                    }
                )
                if not correct or not r.valid:
                    failures.append(
                        {"id": cmd["id"], "text": cmd["text"], "lang": cmd["lang"], "expected": expected,
                         "got": r.name, "arguments": r.arguments, "valid": r.valid,
                         "finish_reason": r.finish_reason, "error": error}
                    )
                flush(False)
                if error and not srv.is_alive():
                    extra["server_crashed"] = srv.stderr_tail()[-3000:]
                    print(f"[{config}] llama-server died after {len(samples)} calls:\n{extra['server_crashed']}", file=sys.stderr)
                    break

            # VRAM while the server is warm: per-process if nvidia-smi reports it, else delta of total used.
            used = gpu_used_mib()
            extra["vram_pid_mib"] = process_vram_mib(srv._proc.pid) if srv._proc else None
            extra["vram_total_used_mib"] = used
            extra["vram_server_mib"] = (
                None if "server_crashed" in extra
                else extra["vram_pid_mib"] if extra["vram_pid_mib"] is not None
                else (used - idle_mib if used is not None and idle_mib is not None else None)
            )
            try:
                if "server_crashed" in extra:
                    raise KeyError("server crashed")
                gen = client.generate_text("Write a short story about a robot who learns to cook.", max_tokens=256)
                extra["decode_tok_s"] = float(gen["timings"]["predicted_per_second"])
                extra["decode_tokens"] = int(gen["timings"].get("predicted_n", 0))
                draft_n = gen["timings"].get("draft_n")
                if draft_n:
                    extra["decode_draft_accept"] = gen["timings"].get("draft_n_accepted", 0) / draft_n
            except (httpx.HTTPError, KeyError) as e:
                extra["decode_error"] = f"{type(e).__name__}: {e}"
    except LlamaServerError as e:
        extra["start_error"] = str(e)[:3000]
        print(f"[{config}] llama-server failed:\n{e.stderr_tail[-2000:]}", file=sys.stderr)
        flush(True)
        raise SystemExit(f"{config}: server failed to start/run") from e
    finally:
        srv.stop()
    flush(True)
    print_report(json.loads(out_path.read_text(encoding="utf-8")))


def print_report(out: dict) -> None:
    def verdict(ok: bool) -> str:
        return "PASS" if ok else "FAIL"

    t, st, sn = THRESHOLDS, out["stats_total"], out["stats_name_ms"]
    print(f"\n[{out['config']}] {out['flags']}   (n={out['n']})")
    if "server_crashed" in out:
        print("llama-server crashed during the run (see server_crashed in the json)")
    if st is None:
        return
    print(f"total p50 {st['p50']:7.1f} ms (<= {t['total_p50']:.0f}) {verdict(st['p50'] <= t['total_p50'])}   "
          f"p95 {st['p95']:7.1f} ms (<= {t['total_p95']:.0f}) {verdict(st['p95'] <= t['total_p95'])}")
    if sn:
        print(f"name_ms p50 {sn['p50']:7.1f}   p95 {sn['p95']:7.1f}")
    print(f"valid {out['valid_rate']:.1%} (>= {t['valid']:.0%})  correct {out['correct_rate']:.1%} (>= {t['correct']:.0%})")
    for lang, v in sorted(out["per_lang"].items()):
        print(f"  {lang:6s} n={v['n']:3d} valid {v['valid_rate']:.1%} correct {v['correct_rate']:.1%}")
    print(f"finish_reasons {out['finish_reasons']}  http_errors {out['http_errors']}  max_tokens {out['max_tokens']}")
    print(f"VRAM {out.get('vram_server_mib')} MiB (budget {VRAM_BUDGET_MIB})   decode {out.get('decode_tok_s', 0):.1f} tok/s")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", default="baseline", choices=sorted(CONFIGS))
    ap.add_argument("--mmproj", action="store_true",
                    help=f"also load the vision projector ({MMPROJ.name}); results go to <config>+mmproj")
    args = ap.parse_args()
    run(args.config, mmproj=args.mmproj)


if __name__ == "__main__":
    main()
