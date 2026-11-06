"""Unit tests for bench/report.py (go/no-go evaluation, data-driven decisions, markdown rendering)."""

import math
import sys
from pathlib import Path

BENCH = Path(__file__).resolve().parents[2] / "bench"
sys.path.insert(0, str(BENCH))

from report import (  # noqa: E402
    Check,
    build_decisions,
    evaluate,
    fallback_texts,
    pick_llm_entry,
    pick_t1,
    render,
    stt_inputs,
    vram_inputs,
)

GATES = {"p95_ms": 80.0, "precision": 0.97, "min_accepted_share": 0.70, "min_accepted_share_el": 0.60}


def good_results() -> dict:
    return {
        "llm": {"stats_total": {"p50": 600.0, "p95": 900.0}, "valid_rate": 0.99, "correct_rate": 0.95, "decode_tok_s": 60.0},
        "decider": {"overall": 0.9, "el": 0.85, "p95_ms": 50.0,
                    "at_97": {"threshold": 0.7, "accepted_share": 0.74, "precision": 0.98}},
        "t1": {"candidate": "minilm-onnx", "p95_ms": 7.0, "accepted_share": 0.75, "accepted_share_el": 0.65,
               "gates": GATES},
        "stt": {"wer_en": 0.05, "wer_el": 0.10, "p50_ms_le4s": 100.0, "cpu_p50_ms": 800.0, "gpu_invalid": None},
        "vram": {"total_gib": 4.0, "mmproj": None, "invalid": None, "stt_p50_ms": 90.0, "llm_p50_ms": 650.0},
    }


def by_name(checks: list[Check]) -> dict[str, Check]:
    return {c.name: c for c in checks}


def cand(p95: float, share: float | None, share_el: float | None = None, overall: float = 0.8) -> dict:
    at = None if share is None else {"threshold": 0.8, "accepted_share": share,
                                     "accepted_share_el": share if share_el is None else share_el, "precision": 0.98}
    return {"accuracy": {"overall": overall, "el": overall}, "latency_ms": {"p95": p95},
            "sweep": {"at_97_precision": at, "at_97_precision_fine": at}}


# ------------------------------------------------------------------ evaluate


def test_evaluate_flags_llm_p50_over_budget():
    r = good_results()
    r["llm"]["stats_total"]["p50"] = 820.0
    checks = by_name(evaluate(r))
    bad = [c for c in checks.values() if "p50" in c.name and c.name.startswith("LLM total p50 (")]
    assert len(bad) == 1 and bad[0].passed is False and bad[0].value == 820.0
    assert checks["LLM total p95 (ms)"].passed is True


def test_evaluate_all_pass_on_good_results():
    checks = evaluate(good_results())
    assert checks and all(c.passed for c in checks), [c for c in checks if not c.passed]


def test_evaluate_missing_results_not_run():
    r = good_results()
    r["stt"] = None
    checks = evaluate(r)
    stt = [c for c in checks if c.name.startswith("STT") and "while" not in c.name]
    assert stt and all(not c.passed and math.isnan(c.value) and "not run" in c.threshold for c in stt)
    assert all(c.passed for c in checks if c not in stt)


def test_evaluate_t1_rows_use_best_candidate_and_gates():
    r = good_results()
    r["t1"].update(p95_ms=90.0, accepted_share=0.38, accepted_share_el=0.36)
    t1 = [c for c in evaluate(r) if c.name.startswith("T1")]
    assert len(t1) == 3 and all("minilm-onnx" in c.name for c in t1)
    vals = {c.threshold: (c.value, c.passed) for c in t1}
    assert vals["<= 80"] == (90.0, False)
    assert vals[">= 70"] == (38.0, False) and vals[">= 60"] == (36.0, False)


def test_evaluate_t1_not_run():
    r = good_results()
    r["t1"] = None
    t1 = [c for c in evaluate(r) if c.name.startswith("T1")]
    assert len(t1) == 3 and all(not c.passed and "not run" in c.threshold for c in t1)


def test_evaluate_stt_cpu_fallback_fails_gpu_check():
    r = good_results()
    r["stt"]["gpu_invalid"] = "ran on CPU: onnxruntime loaded CPUExecutionProvider"
    gpu = by_name(evaluate(r))["STT GPU p50, clips <= 4 s (ms)"]
    assert not gpu.passed and math.isnan(gpu.value) and "ran on CPU" in gpu.threshold


def test_evaluate_coresident_checks():
    r = good_results()
    r["vram"].update(stt_p50_ms=2039.0, llm_p50_ms=738.0)
    checks = by_name(evaluate(r))
    stt, llm = checks["STT GPU p50 while LLM loaded (ms)"], checks["LLM total p50 while STT loaded (ms)"]
    assert (stt.value, stt.threshold, stt.passed) == (2039.0, "<= 150", False)
    assert (llm.value, llm.threshold, llm.passed) == (738.0, "<= 700", False)


def test_evaluate_coresident_not_run_for_old_vram_results():
    r = good_results()
    r["vram"].update(stt_p50_ms=None, llm_p50_ms=None)
    checks = by_name(evaluate(r))
    for n in ("STT GPU p50 while LLM loaded (ms)", "LLM total p50 while STT loaded (ms)"):
        assert not checks[n].passed and "not run" in checks[n].threshold


def test_evaluate_vram_label_shows_mmproj_and_cpu_fallback_fails():
    r = good_results()
    assert any(c.name.startswith("VRAM") and "no mmproj" in c.name for c in evaluate(r))
    r["vram"]["mmproj"] = "mmproj-F16.gguf"
    assert any(c.name.startswith("VRAM") and "+ mmproj" in c.name for c in evaluate(r))
    r["vram"]["invalid"] = "ran on CPU: onnxruntime loaded CPUExecutionProvider"
    vram = [c for c in evaluate(r) if c.name.startswith("VRAM") or "while" in c.name]
    assert len(vram) == 3 and all(not c.passed and "ran on CPU" in c.threshold for c in vram)


# ------------------------------------------------------------------ normalizing raw results


def test_stt_inputs_marks_cpu_fallback_invalid():
    clip = {"seconds": 2.0, "latency_ms": 100.0}
    raw = {"devices": {"cuda": {"provider": "CPUExecutionProvider", "wer": {"en": 0.05, "el_mixed": 0.1},
                                "transcripts": [clip]}}}
    s = stt_inputs(raw)
    assert s["wer_en"] == 0.05 and "ran on CPU" in s["gpu_invalid"]
    raw["devices"]["cuda"]["provider"] = "CUDAExecutionProvider"
    assert stt_inputs(raw)["gpu_invalid"] is None and stt_inputs(raw)["p50_ms_le4s"] == 100.0


def test_vram_inputs_reads_old_and_new_files():
    old = {"total_gib": 4.3, "stt_provider": "CUDAExecutionProvider"}
    v = vram_inputs(old)
    assert v["total_gib"] == 4.3 and v["invalid"] is None and v["stt_p50_ms"] is None and v["mmproj"] is None
    new = {"total_gib": 4.3, "provider": "CPUExecutionProvider", "mmproj": "mmproj-F16.gguf",
           "coresident": {"stt_p50_ms": 80.0, "llm_total_p50_ms": 600.0}}
    v = vram_inputs(new)
    assert "ran on CPU" in v["invalid"] and v["mmproj"] == "mmproj-F16.gguf"
    assert (v["stt_p50_ms"], v["llm_p50_ms"]) == (80.0, 600.0)


def test_pick_t1_fastest_candidate_meeting_goal():
    doc = {"gates": GATES, "candidates": {"a": cand(9.0, 0.75, 0.65), "b": cand(5.0, 0.72, 0.61),
                                          "c": cand(3.0, 0.30), "broken": {"error": "OOM"}}}
    best = pick_t1(doc)
    assert best["candidate"] == "b" and best["verdict"] == "met" and best["met"] == ["a", "b"]


def test_pick_t1_highest_share_within_latency_when_none_meets():
    doc = {"gates": GATES, "candidates": {"fast-low": cand(5.0, 0.36), "fast-mid": cand(7.0, 0.38),
                                          "slow-high": cand(150.0, 0.74, 0.62), "fast-none": cand(4.0, None)}}
    best = pick_t1(doc)
    assert best["candidate"] == "fast-mid" and best["verdict"] == "fast-only" and best["met"] == []
    assert best["accepted_share"] == 0.38


def test_pick_t1_none_within_latency():
    doc = {"gates": GATES, "candidates": {"slow": cand(150.0, 0.74, 0.62), "slower": cand(300.0, 0.5)}}
    best = pick_t1(doc)
    assert best["verdict"] == "none-fast" and best["candidate"] == "slow"


def test_pick_llm_entry_prefers_complete_mmproj_run():
    base = {"decode_tok_s": 60.0}
    assert pick_llm_entry({"baseline": base}) == "baseline"
    assert pick_llm_entry({"baseline": base, "baseline+mmproj": {"decode_tok_s": 58.0}}) == "baseline+mmproj"
    assert pick_llm_entry({"baseline": base, "baseline+mmproj": {"start_error": "OOM"}}) == "baseline"
    assert pick_llm_entry({}) is None


# ------------------------------------------------------------------ decisions prose


def facts_for(t1_doc: dict) -> dict:
    return {
        "llm_key": "baseline",
        "llm": {"flags": "-fa on --cache-type-k q8_0 --cache-type-v q8_0", "mmproj": None,
                "stats_total": {"p50": 599.0, "p95": 1184.0}},
        "tuning": {"baseline": {"stats_total": {"p50": 599.0, "p95": 1184.0}},
                   "draft": {"stats_total": {"p50": 3600.0, "p95": 5000.0}}},
        "t1": t1_doc,
        "t1_best": pick_t1(t1_doc),
    }


META = {"gpu": "Test GPU 6G", "gpu_vram_mib": 6144, "cpu": "Test CPU", "ram_gib": 15.9}


def test_decisions_derive_t1_verdict_from_data():
    none_met = {"gates": GATES, "candidates": {"minilm-onnx": cand(7.0, 0.38, 0.36)}}
    text = "\n".join(sum(build_decisions(facts_for(none_met), META), []))
    assert "No candidate met the goal" in text and "minilm-onnx" in text
    met = {"gates": GATES, "candidates": {"minilm-onnx": cand(7.0, 0.75, 0.65)}}
    text = "\n".join(sum(build_decisions(facts_for(met), META), []))
    assert "No candidate met" not in text and "`minilm-onnx` meets the goal" in text


def test_decisions_have_no_hardcoded_hardware_or_llm_facts():
    t1 = {"gates": GATES, "candidates": {"minilm-onnx": cand(7.0, 0.38, 0.36),
                                         "gliner-cuda": {"error": "torch.OutOfMemoryError: no memory"}}}
    decisions, notes = build_decisions(facts_for(t1), META)
    text = "\n".join(decisions + notes)
    for literal in ("6 GB card", " here", "1660"):
        assert literal not in text, literal
    assert "Test GPU 6G" in text and "ctx 8192" in text and "no vision projector" in text
    assert "no speculative decoding" in text and "`draft`" in text


def test_minilm_fallback_states_its_accepted_share():
    t1 = {"gates": GATES, "candidates": {"minilm-onnx": cand(7.4, 0.38, 0.36)}}
    text = fallback_texts(facts_for(t1))["Decider p95"]
    assert "38%" in text and "36%" in text


def test_render_contains_every_check_name_and_target_banner():
    checks = evaluate(good_results())
    md = render(checks, {"gpu": "Test GPU", "ram_gib": 16})
    for c in checks:
        assert c.name in md
    assert "Decisions" in md and "Test GPU" in md and "Provisional" in md
    md = render(checks, {"gpu": "NVIDIA GeForce RTX 5050 Laptop GPU"})
    assert "Provisional" not in md and "target machine" in md
