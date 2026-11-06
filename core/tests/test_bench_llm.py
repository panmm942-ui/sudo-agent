"""Unit tests for bench/bench_llm.py summary statistics (no server needed)."""

import sys
from pathlib import Path

BENCH = Path(__file__).resolve().parents[2] / "bench"
sys.path.insert(0, str(BENCH))

from bench_llm import build_summary, result_key  # noqa: E402
from sudo_core.llm.server import LlamaServer  # noqa: E402


def sample(total, error=None, valid=True):
    return {"id": "x", "total_ms": total, "ttft_ms": None if error else total / 2,
            "name_ms": None if error else total / 2, "tokens": 0, "finish_reason": None if error else "tool_calls",
            "valid": valid and error is None, "correct": error is None, "slots_ok": None, "error": error}


def test_errored_calls_are_excluded_from_latency_and_counted_invalid():
    srv = LlamaServer(exe=Path("llama-server.exe"), model=Path("m.gguf"))
    samples = [sample(600.0), sample(800.0), sample(None, error="ReadTimeout: boom")]
    out = build_summary("baseline", srv, samples, {}, {})
    assert out["stats_total"]["n"] == 2 and out["stats_total"]["p50"] == 700.0
    assert out["stats_ttft"]["n"] == 2 and out["stats_name_ms"]["n"] == 2
    assert out["http_errors"] == 1 and out["invalid_calls"] == 1
    assert out["valid_rate"] == 2 / 3


def test_summary_records_mmproj():
    srv = LlamaServer(exe=Path("llama-server.exe"), model=Path("m.gguf"), mmproj=Path("mmproj-F16.gguf"))
    out = build_summary("baseline", srv, [sample(600.0)], {}, {})
    assert out["mmproj"] == "mmproj-F16.gguf" and "--mmproj" in out["flags"]


def test_result_key_separates_mmproj_runs():
    assert result_key("baseline", mmproj=False) == "baseline"
    assert result_key("baseline", mmproj=True) == "baseline+mmproj"
