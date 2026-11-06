"""Co-resident check: llama-server (default config) + Parakeet on CUDA loaded together (model run).

Run from core/:  uv run python ../bench/bench_vram.py [--mmproj] [--calls 5]
Writes bench/results/vram.json:
  - VRAM of both models together. Under WDDM nvidia-smi usually reports per-process memory as [N/A]; the
    per-process sum is used only when *every* sudo PID reports a number, otherwise (total GPU used - idle GPU
    used). The method is recorded.
  - Co-resident latency: after warm-up, --calls rounds of (transcribe the committed SAPI clip, one tool call)
    while both models stay loaded; p50 of each. The real assistant always runs both models together.
  - `provider`: the ONNX Runtime provider Parakeet actually loaded. Anything but CUDAExecutionProvider marks the
    run invalid (a silent CPU fallback would shrink the VRAM figure and is not a GPU number).
--mmproj also loads the vision projector (spec section 2 budgets Qwen3.5-4B *with* mmproj; it OOMs on 6 GB).
"""

from __future__ import annotations

import argparse
import gc
import json
import os
import subprocess
import sys
from pathlib import Path

import httpx
import soundfile as sf

sys.path.insert(0, str(Path(__file__).resolve().parent))

from benchenv import configure_cuda_cache, provider_problem  # noqa: E402
from sudo_core.llm.client import LlmClient  # noqa: E402
from sudo_core.llm.server import LlamaServer  # noqa: E402
from sudo_core.llm.tools import SYSTEM_PROMPT, intents_to_tools  # noqa: E402
from sudo_core.perf import summarize  # noqa: E402
from sudo_core.stt.parakeet import ParakeetStt  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
MODELS = ROOT / "models"
MMPROJ = MODELS / "llm" / "mmproj-F16.gguf"
CLIP = ROOT / "core" / "tests" / "data" / "open_discord.wav"
OUT = ROOT / "bench" / "results" / "vram.json"
LIMIT_GIB = 4.6
MIB_PER_GIB = 1024
STT_WARMUPS = 3  # the first CUDA call is a ~12 s cold init
LLM_WARMUPS = 2
LLM_TEXTS = ["set the volume to 40", "open spotify", "what's on my calendar today", "βάλε παύση στη μουσική"]


def smi(*args: str) -> str:
    try:
        return subprocess.run(["nvidia-smi", *args], capture_output=True, text=True, timeout=15).stdout
    except (OSError, subprocess.SubprocessError):
        return ""


def gpu_used_mib() -> int | None:
    try:
        return int(smi("--query-gpu=memory.used", "--format=csv,noheader,nounits").strip().splitlines()[0])
    except (ValueError, IndexError):
        return None


def pid_sum_mib(rows: list[str], pids: set[int]) -> int | None:
    """Sum of nvidia-smi per-process memory over our PIDs, or None unless *every* PID reports a number
    (a partial sum would silently drop e.g. Parakeet when only llama-server reports)."""
    found: dict[int, int] = {}
    for row in rows:
        parts = [p.strip() for p in row.split(",")]
        if len(parts) == 2 and parts[0].isdigit() and int(parts[0]) in pids and parts[1].isdigit():
            found[int(parts[0])] = int(parts[1])
    return sum(found.values()) if pids and set(found) == pids else None


def vram_figure(pid_sum: int | None, delta: int | None) -> tuple[int | None, str]:
    if pid_sum is not None:
        return pid_sum, "per-process sum"
    if delta is not None:
        return delta, "total-used minus idle (per-process memory incomplete or N/A, e.g. under WDDM)"
    return None, "unavailable"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--mmproj", action="store_true", help=f"also load the vision projector ({MMPROJ.name})")
    ap.add_argument("--calls", type=int, default=5, help="co-resident calls of each workload after warm-up (>= 3)")
    args = ap.parse_args()
    if args.calls < 3:
        sys.exit("--calls must be >= 3")

    configure_cuda_cache()
    intents = json.loads((ROOT / "bench" / "fixtures" / "intents.json").read_text(encoding="utf-8"))
    tools = intents_to_tools(intents)
    audio, sr = sf.read(CLIP, dtype="float32")
    idle = gpu_used_mib()
    srv = LlamaServer(exe=MODELS / "llama.cpp" / "llama-server.exe", model=MODELS / "llm" / "Qwen3.5-4B-Q4_K_M.gguf",
                      mmproj=MMPROJ if args.mmproj else None)
    stt = None
    out: dict = {"idle_mib": idle, "server_flags": srv.flags_description(),
                 "mmproj": MMPROJ.name if args.mmproj else None}
    try:
        # STT first: its ONNX session init needs a large transient host-memory commit, which fails
        # ("bad allocation") on a tight-commit box if llama-server is already resident.
        stt = ParakeetStt(MODELS / "stt", device="cuda")
        srv.start(timeout_s=240)
        out["provider"] = stt.active_provider
        problem = provider_problem("cuda", out["provider"])
        out["valid"] = problem is None
        if problem:
            out["invalid_reason"] = problem
            print(f"INVALID RUN: Parakeet {problem}", file=sys.stderr)
        for _ in range(STT_WARMUPS):
            stt.transcribe(audio, sr)
        stt_lats: list[float] = []
        llm_lats: list[float] = []
        llm_valid, llm_errors = 0, []
        with httpx.Client(timeout=httpx.Timeout(300.0)) as http:
            client = LlmClient(srv.base_url, http=http)

            def msgs(text: str) -> list[dict]:
                return [{"role": "system", "content": SYSTEM_PROMPT}, {"role": "user", "content": text}]

            for i in range(LLM_WARMUPS):
                client.tool_call(msgs(LLM_TEXTS[i % len(LLM_TEXTS)]), tools)
            for i in range(args.calls):  # alternate, as the assistant would: hear, then act
                stt_lats.append(stt.transcribe(audio, sr).latency_ms)
                try:
                    call = client.tool_call(msgs(LLM_TEXTS[i % len(LLM_TEXTS)]), tools)
                except httpx.HTTPError as e:  # errored calls never enter the latency stats
                    llm_errors.append(f"{type(e).__name__}: {e}")
                    continue
                llm_lats.append(call.total_ms)
                llm_valid += int(call.valid)
        out["coresident"] = {
            "calls": args.calls,
            "clip": CLIP.relative_to(ROOT).as_posix(),
            "clip_seconds": round(len(audio) / sr, 2),
            "stt_latency_ms": stt_lats,
            "stt_p50_ms": summarize(stt_lats).p50,
            "llm_total_ms": llm_lats,
            "llm_total_p50_ms": summarize(llm_lats).p50 if llm_lats else None,
            "llm_valid": llm_valid,
            "llm_errors": llm_errors,
        }
        used = gpu_used_mib()
        pids = {srv._proc.pid, os.getpid()} if srv._proc else {os.getpid()}
        rows = [l.strip() for l in smi("--query-compute-apps=pid,used_memory",
                                       "--format=csv,noheader,nounits").splitlines()]
        pid_mib = pid_sum_mib(rows, pids)
        delta = used - idle if used is not None and idle is not None else None
        mib, method = vram_figure(pid_mib, delta)
        out.update({"total_used_mib": used, "pid_sum_mib": pid_mib, "delta_mib": delta, "method": method,
                    "nvidia_smi_compute_apps": rows})
        if mib is None:
            sys.exit("could not read VRAM from nvidia-smi")
        out["total_mib"] = mib
        out["total_gib"] = mib / MIB_PER_GIB
        out["limit_gib"] = LIMIT_GIB
        out["passed"] = out["valid"] and out["total_gib"] <= LIMIT_GIB
    finally:
        del stt
        gc.collect()
        srv.stop()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, indent=2), encoding="utf-8")
    print(json.dumps({k: v for k, v in out.items() if k != "nvidia_smi_compute_apps"}, indent=2))
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
