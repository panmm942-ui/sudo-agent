"""Go/no-go report: turns bench/results/*.json into bench/RESULTS.md.

Run from core/:  uv run python ../bench/report.py
`evaluate` takes normalized inputs (see `collect`); a missing section yields failed "not run" checks.
Every verdict in the Decisions section is computed from the results and the detected hardware.
"""

from __future__ import annotations

import ctypes
import datetime
import inspect
import json
import math
import platform
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from statistics import median

sys.path.insert(0, str(Path(__file__).resolve().parent))

from benchenv import provider_problem  # noqa: E402
from sudo_core.llm.server import LlamaServer  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "bench" / "results"
OUT = ROOT / "bench" / "RESULTS.md"
LLAMA_EXE = ROOT / "models" / "llama.cpp" / "llama-server.exe"
LOCK = ROOT / "models" / "lock.json"
LLM_CONFIG = "baseline"  # Task 8's winner == the LlamaServer default (q8_0 KV, no speculation)
VRAM_GIB_LIMIT = 4.6
T1_DEFAULT_GATES = {"p95_ms": 80.0, "precision": 0.97, "min_accepted_share": 0.70, "min_accepted_share_el": 0.60}
TARGET = {"desc": "Lenovo Legion 5 laptop: Ryzen 7, RTX 5050 Laptop GPU 8 GB, 32 GB RAM",
          "gpu_tag": "5050", "gpu": "RTX 5050 Laptop GPU", "gpu_gb": 8, "cpu": "Ryzen 7", "ram_gb": 32}


@dataclass(frozen=True)
class Check:
    name: str
    value: float
    threshold: str
    passed: bool


def _not_run(items: list[tuple[str, str]], why: str = "not run") -> list[Check]:
    return [Check(n, math.nan, f"{t} ({why})", False) for n, t in items]


def _le(name: str, value: float, limit: float) -> Check:
    return Check(name, float(value), f"<= {limit:g}", bool(value <= limit))


def _ge(name: str, value: float, limit: float) -> Check:
    return Check(name, float(value), f">= {limit:g}", bool(value >= limit))


def _pct(x: float) -> float:
    return round(x * 100, 6)


def evaluate(results: dict) -> list[Check]:
    """Apply the plan's Global Constraints thresholds. Missing/None sections become failed "not run" checks."""
    checks: list[Check] = []

    llm = results.get("llm")
    if llm:
        checks += [
            _le("LLM total p50 (ms)", llm["stats_total"]["p50"], 700),
            _le("LLM total p95 (ms)", llm["stats_total"]["p95"], 1000),
            _ge("LLM valid tool call (%)", llm["valid_rate"] * 100, 95),
            _ge("LLM correct tool (%)", llm["correct_rate"] * 100, 90),
            _ge("LLM decode (tok/s)", llm["decode_tok_s"], 35),
        ]
    else:
        checks += _not_run([("LLM total p50 (ms)", "<= 700"), ("LLM total p95 (ms)", "<= 1000"),
                            ("LLM valid tool call (%)", ">= 95"), ("LLM correct tool (%)", ">= 90"),
                            ("LLM decode (tok/s)", ">= 35")])

    dec = results.get("decider")
    if dec:
        checks += [
            _ge("Decider top-1 overall (%)", dec["overall"] * 100, 85),
            _ge("Decider top-1 Greek (%)", dec["el"] * 100, 80),
            _le("Decider p95 latency, CPU (ms)", dec["p95_ms"], 80),
        ]
        a = dec.get("at_97")
        if a:
            checks.append(Check("Decider accepted share at >=97% precision (%)", a["accepted_share"] * 100,
                                f"reachable at score >= {a['threshold']:g}", True))
        else:
            checks.append(Check("Decider accepted share at >=97% precision (%)", 0.0,
                                "97% precision not reachable at any threshold", False))
    else:
        checks += _not_run([("Decider top-1 overall (%)", ">= 85"), ("Decider top-1 Greek (%)", ">= 80"),
                            ("Decider p95 latency, CPU (ms)", "<= 80"),
                            ("Decider accepted share at >=97% precision (%)", "reachable")])

    t1 = results.get("t1")
    g = {**T1_DEFAULT_GATES, **((t1 or {}).get("gates") or {})}
    lim_p95, lim_all, lim_el = g["p95_ms"], _pct(g["min_accepted_share"]), _pct(g["min_accepted_share_el"])
    if t1:
        c = f"`{t1['candidate']}`"
        checks += [
            _le(f"T1 p95 latency, {c} (ms)", t1["p95_ms"], lim_p95),
            _ge(f"T1 accepted at >=97% precision, overall, {c} (%)", _pct(t1["accepted_share"] or 0.0), lim_all),
            _ge(f"T1 accepted at >=97% precision, Greek, {c} (%)", _pct(t1["accepted_share_el"] or 0.0), lim_el),
        ]
    else:
        checks += _not_run([("T1 p95 latency, best candidate (ms)", f"<= {lim_p95:g}"),
                            ("T1 accepted at >=97% precision, overall, best candidate (%)", f">= {lim_all:g}"),
                            ("T1 accepted at >=97% precision, Greek, best candidate (%)", f">= {lim_el:g}")])

    stt = results.get("stt")
    if stt:
        checks += [
            _le("STT WER English (%)", stt["wer_en"] * 100, 10),
            _le("STT WER Greek (%)", stt["wer_el"] * 100, 15),
        ]
        if stt.get("gpu_invalid"):
            checks += _not_run([("STT GPU p50, clips <= 4 s (ms)", "<= 150")], f"invalid: {stt['gpu_invalid']}")
        else:
            checks.append(_le("STT GPU p50, clips <= 4 s (ms)", stt["p50_ms_le4s"], 150))
        cpu = stt.get("cpu_p50_ms")
        checks.append(Check("STT CPU p50 (ms, game mode)", cpu if cpu is not None else math.nan,
                            "recorded" if cpu is not None else "recorded (not run)", cpu is not None))
    else:
        checks += _not_run([("STT WER English (%)", "<= 10"), ("STT WER Greek (%)", "<= 15"),
                            ("STT GPU p50, clips <= 4 s (ms)", "<= 150"), ("STT CPU p50 (ms, game mode)", "recorded")],
                           "not run: no voice clips recorded yet")

    vram = results.get("vram")
    co = [("STT GPU p50 while LLM loaded (ms)", "stt_p50_ms", 150), ("LLM total p50 while STT loaded (ms)", "llm_p50_ms", 700)]
    if vram:
        label = ("VRAM llama-server + mmproj + Parakeet (GiB)" if vram.get("mmproj")
                 else "VRAM llama-server (no mmproj) + Parakeet (GiB)")
        if vram.get("invalid"):
            why = f"invalid: {vram['invalid']}"
            checks += _not_run([(label, f"<= {VRAM_GIB_LIMIT:g}")] + [(n, f"<= {lim}") for n, _, lim in co], why)
        else:
            checks.append(_le(label, vram["total_gib"], VRAM_GIB_LIMIT))
            for name, key, lim in co:
                v = vram.get(key)
                checks += [_le(name, v, lim)] if v is not None else _not_run([(name, f"<= {lim}")])
    else:
        checks += _not_run([("VRAM llama-server + Parakeet (GiB)", f"<= {VRAM_GIB_LIMIT:g}")]
                           + [(n, f"<= {lim}") for n, _, lim in co])
    return checks


# More specific keys first: render() uses the first key that prefixes the check name.
FALLBACKS = {
    "LLM total p50 while": "VRAM is likely over-committed (WDDM pages to system RAM): free VRAM (ctx 4096, smaller "
                           "quant, no mmproj) or run STT on CPU while the LLM is loaded",
    "LLM total": "try Q4_K_S / IQ4_XS, reduce the tools sent per call (route by T1 first), shorten slot encodings, "
                 "or show the 'acting' state at the tool name (name_ms) and finish the arguments in the background",
    "LLM valid": "tighten the system prompt / lower max_tokens; fall back to grammar-constrained decoding",
    "LLM correct": "add few-shot examples for confusable intents or move those intents to the T0 grammar",
    "LLM decode": "use a lower quant or a faster GPU; check the llama-server log for CPU fallback",
    "Decider top-1 overall": "keep the decider as a hint only and route the rest to the LLM (T2)",
    "Decider top-1 Greek": "add Greek label descriptions or move those intents to the T0 grammar",
    "Decider p95": "run the decider on GPU (needs VRAM headroom) or use an embedding router (T1 candidates)",
    "Decider accepted": "expect a smaller T1 share and send more utterances to the LLM",
    "T1 p95": "run GLiNER on the GPU (bench/setup_gliner_cuda.py, then bench_t1.py --candidates gliner-cuda) "
              "or drop T1 in favor of the T0 grammar + LLM",
    "T1 accepted": "accept a smaller T1 share (the rest goes to the LLM), add exemplars for the weak intents, "
                   "or drop T1 in favor of the T0 grammar + LLM",
    "STT WER English": "tune audio capture (gain, VAD padding) or switch to whisper large-v3-turbo",
    "STT WER Greek": "switch to faster-whisper large-v3-turbo for Greek (plan Task 6 Step 7)",
    "STT GPU p50 while": "VRAM is likely over-committed (WDDM pages to system RAM): free VRAM (ctx 4096, smaller "
                         "quant, no mmproj) or run STT on CPU while the LLM is loaded",
    "STT GPU": "check the CUDA provider really loaded (no CPU fallback: cuDNN / driver / GPU arch); trim VAD tail padding",
    "STT CPU": "record the clips and run bench_stt.py with --devices cuda cpu",
    "STT": "record voice clips with bench/record_samples.py, then run bench/bench_stt.py",
    "VRAM": "reduce ctx to 4096, use a smaller quant, or run STT on CPU while the LLM is loaded",
}


def _fmt(v: float) -> str:
    return "n/a" if math.isnan(v) else f"{v:.1f}" if abs(v) < 1000 else f"{v:,.0f}"


def on_target(meta: dict) -> bool:
    return TARGET["gpu_tag"] in str(meta.get("gpu", ""))


def render(checks: list[Check], meta: dict) -> str:
    passed = sum(c.passed for c in checks)
    gpu = meta.get("gpu", "an unknown GPU")
    banner = (f"> **Measured on the target machine** ({TARGET['desc']}; detected GPU: {gpu})."
              if on_target(meta) else
              f"> **Provisional: not measured on the target machine.** These numbers come from {gpu}, not from the "
              f"target ({TARGET['desc']}). The whole Phase 0 suite will be re-run there before the go/no-go is final.")
    lines = [
        "# Phase 0 benchmark results: go/no-go",
        "",
        banner,
        "",
        f"Generated: {meta.get('generated', 'n/a')}. **{passed}/{len(checks)} checks passed.**",
        "",
        "## Hardware and build (detected at runtime)",
        "",
    ]
    for key, label in [("gpu", "GPU"), ("gpu_vram_mib", "GPU VRAM (MiB)"), ("cpu", "CPU"), ("ram_gib", "RAM (GiB)"),
                       ("os", "OS"), ("python", "Python"), ("llama_build", "llama.cpp build"),
                       ("llama_variant", "llama.cpp asset variant"), ("mmproj", "Vision projector (mmproj)"),
                       ("llm_results", "LLM results used")]:
        if key in meta:
            lines.append(f"- {label}: {meta[key]}")
    lines += ["", "## Go/no-go table", "", "| Check | Value | Threshold | Result |", "|---|---:|---|---|"]
    for c in checks:
        lines.append(f"| {c.name} | {_fmt(c.value)} | {c.threshold} | {'PASS' if c.passed else 'FAIL'} |")
    lines += ["", "## Decisions", ""]
    lines += [f"  {n.strip()}" if n.startswith("  -") else f"- {n}" for n in meta.get("decisions", [])]
    lines += ["", "### Failed checks and fallbacks", ""]
    failed = [c for c in checks if not c.passed]
    if not failed:
        lines.append("- None.")
    fallbacks = {**FALLBACKS, **meta.get("fallbacks", {})}
    for c in failed:
        fb = next((fallbacks[k] for k in FALLBACKS if c.name.startswith(k)), "investigate")
        lines.append(f"- **{c.name}** ({_fmt(c.value)} vs {c.threshold}): fallback: {fb}.")
    lines += ["", "### Where the target machine may change a decision", ""]
    lines += [f"- {n}" for n in meta.get("laptop_notes", [])]
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------- normalizing real results


def _at97(c: dict) -> dict | None:
    s = c.get("sweep", {})
    return s.get("at_97_precision") or s.get("at_97_precision_fine")


def pick_t1(doc: dict) -> dict | None:
    """Task 9 Step 7 rule: the fastest candidate meeting the goal; if none does, the highest accepted share at
    97% precision among those within the p95 budget; if none is within it, the highest share overall
    (verdict "none-fast": drop T1 in favor of T0 grammar + LLM)."""
    g = {**T1_DEFAULT_GATES, **(doc.get("gates") or {})}
    cands = {n: c for n, c in doc.get("candidates", {}).items() if "accuracy" in c}
    if not cands:
        return None

    def share(n: str) -> float:
        a = _at97(cands[n])
        return a["accepted_share"] if a else 0.0

    def share_el(n: str) -> float:
        a = _at97(cands[n])
        return (a.get("accepted_share_el") or 0.0) if a else 0.0

    def p95(n: str) -> float:
        return cands[n]["latency_ms"]["p95"]

    fast = [n for n in cands if p95(n) <= g["p95_ms"]]
    met = [n for n in fast if share(n) >= g["min_accepted_share"] and share_el(n) >= g["min_accepted_share_el"]]
    if met:
        best, verdict = min(met, key=p95), "met"
    elif fast:
        best, verdict = max(fast, key=lambda n: (share(n), -p95(n))), "fast-only"
    else:
        best, verdict = max(cands, key=lambda n: (share(n), -p95(n))), "none-fast"
    a = _at97(cands[best])
    return {"candidate": best, "verdict": verdict, "met": met, "gates": g, "p95_ms": p95(best),
            "accepted_share": share(best), "accepted_share_el": share_el(best),
            "threshold": a["threshold"] if a else None}


def stt_inputs(raw: dict) -> dict | None:
    gpu = raw.get("devices", {}).get("cuda")
    if not gpu:
        return None
    cpu = raw["devices"].get("cpu")
    provider = gpu.get("provider") or gpu.get("active_provider") or "unknown"
    lat = [r["latency_ms"] for r in gpu["transcripts"] if r["seconds"] <= 4.0]
    wer_en, wer_el = gpu["wer"].get("en"), gpu["wer"].get("el_mixed")
    if wer_en is None or wer_el is None or not lat:
        return None
    return {"wer_en": wer_en, "wer_el": wer_el, "p50_ms_le4s": median(lat), "provider": provider,
            "gpu_invalid": provider_problem("cuda", provider),
            "cpu_p50_ms": cpu["latency_ms"]["all"]["p50"] if cpu else None}


def vram_inputs(raw: dict) -> dict:
    provider = raw.get("provider") or raw.get("stt_provider") or "unknown"
    co = raw.get("coresident") or {}
    return {"total_gib": raw["total_gib"], "mmproj": raw.get("mmproj"), "provider": provider,
            "invalid": provider_problem("cuda", provider), "method": raw.get("method"),
            "stt_p50_ms": co.get("stt_p50_ms"), "llm_p50_ms": co.get("llm_total_p50_ms")}


def pick_llm_entry(tuning: dict) -> str | None:
    """The spec'd configuration (with the vision projector) when it ran to completion, else the text-only one."""
    for key in (f"{LLM_CONFIG}+mmproj", LLM_CONFIG):
        if (tuning.get(key) or {}).get("decode_tok_s") is not None:
            return key
    return None


def _load(name: str):
    p = RESULTS / name
    return json.loads(p.read_text(encoding="utf-8")) if p.exists() else None


def collect() -> tuple[dict, dict]:
    """Read bench/results/*.json -> (evaluate() inputs, extra facts for the Decisions section)."""
    res: dict = {"llm": None, "decider": None, "t1": None, "stt": None, "vram": None}
    facts: dict = {}
    tuning = _load("llm_tuning.json") or {}
    key = pick_llm_entry(tuning)
    facts["tuning"] = tuning
    if key:
        res["llm"] = facts["llm"] = tuning[key]
        facts["llm_key"] = key
    dec = _load("decider.json")
    if dec:
        vname = max(dec["variants"], key=lambda k: dec["variants"][k]["accuracy"]["overall"])
        v = dec["variants"][vname]
        res["decider"] = {"overall": v["accuracy"]["overall"], "el": v["accuracy"]["el"],
                          "p95_ms": v["latency_ms"]["p95"], "at_97": v["sweep"].get("at_97_precision")}
        facts["decider"] = {"variant": vname, "label_mode": v.get("label_mode"), "device": dec.get("device"),
                            **res["decider"]}
    facts["t1"] = _load("t1.json")
    res["t1"] = facts["t1_best"] = pick_t1(facts["t1"]) if facts["t1"] else None
    stt = _load("stt.json")
    res["stt"] = facts["stt"] = stt_inputs(stt) if stt else None
    vram = _load("vram.json")
    res["vram"] = facts["vram"] = vram_inputs(vram) if vram else None
    return res, facts


def hardware_meta() -> dict:
    meta: dict = {"os": platform.platform(), "python": platform.python_version()}
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"],
                             capture_output=True, text=True, timeout=10).stdout.strip().splitlines()[0]
        name, mib = [p.strip() for p in out.split(",")]
        meta["gpu"], meta["gpu_vram_mib"] = name, int(mib)
    except (OSError, ValueError, IndexError, subprocess.SubprocessError):
        meta["gpu"] = "unknown (nvidia-smi unavailable)"
    meta["cpu"] = platform.processor() or "unknown"
    try:
        import winreg

        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as k:
            meta["cpu"] = winreg.QueryValueEx(k, "ProcessorNameString")[0].strip()
    except (ImportError, OSError):
        pass
    try:
        class MemStatus(ctypes.Structure):
            _fields_ = [("length", ctypes.c_ulong), ("load", ctypes.c_ulong), ("total", ctypes.c_ulonglong),
                        ("a", ctypes.c_ulonglong), ("b", ctypes.c_ulonglong), ("c", ctypes.c_ulonglong),
                        ("d", ctypes.c_ulonglong), ("e", ctypes.c_ulonglong),
                        ("f", ctypes.c_ulonglong)]

        m = MemStatus()
        m.length = ctypes.sizeof(m)
        ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(m))
        meta["ram_gib"] = round(m.total / 2**30, 1)
    except (AttributeError, OSError):
        pass
    if LLAMA_EXE.exists():
        try:
            p = subprocess.run([str(LLAMA_EXE), "--version"], capture_output=True, text=True, timeout=20,
                               encoding="utf-8", errors="replace")
            text = p.stderr + p.stdout
            meta["llama_build"] = next((l.strip() for l in text.splitlines() if l.lower().startswith("version")),
                                       "unknown")
        except (OSError, subprocess.SubprocessError):
            meta["llama_build"] = "unknown"
    try:
        lock = json.loads(LOCK.read_text(encoding="utf-8")).get("llama.cpp", {})
        assets = ", ".join(lock.get("files", {}))
        meta["llama_variant"] = lock.get("variant") or f"not recorded in models/lock.json (assets: {assets or 'n/a'})"
    except (OSError, ValueError):
        pass
    return meta


# ---------------------------------------------------------------- decisions prose (all derived from data)


def _gpu_phrase(meta: dict) -> str:
    gpu = meta.get("gpu", "an unknown GPU")
    mib = meta.get("gpu_vram_mib")
    return f"{gpu} ({mib / 1024:.0f} GB)" if mib else gpu


def _ctx(flags: str) -> int:
    m = re.search(r"(?:^|\s)-c (\d+)", flags)
    return int(m.group(1)) if m else inspect.signature(LlamaServer.__init__).parameters["ctx"].default


def _share_text(best: dict) -> str:
    if best["threshold"] is None:
        return "97% precision not reachable at any threshold"
    return (f"{best['accepted_share']:.0%} accepted overall / {best['accepted_share_el']:.0%} Greek at score "
            f"{best['threshold']:g}")


def fallback_texts(facts: dict) -> dict[str, str]:
    """Fallback sentences that quote measured numbers (merged over FALLBACKS by render)."""
    out: dict[str, str] = {}
    t1 = facts.get("t1") or {}
    minilm = (t1.get("candidates") or {}).get("minilm-onnx")
    if minilm and "accuracy" in minilm:
        a = _at97(minilm)
        acc = (f"it accepts {a['accepted_share']:.0%} of utterances ({a.get('accepted_share_el') or 0:.0%} of Greek ones) "
               f"at >= 97% precision, the rest go to the LLM") if a else "it never reaches 97% precision"
        out["Decider p95"] = (f"run the decider on GPU (needs VRAM headroom) or use the MiniLM ONNX embedding router "
                              f"(T1 candidate, p95 {minilm['latency_ms']['p95']:.1f} ms; {acc})")
    return out


def build_decisions(facts: dict, meta: dict) -> tuple[list[str], list[str]]:
    d: list[str] = []
    gpu = _gpu_phrase(meta)
    llm = facts.get("llm")
    if llm:
        flags = llm.get("flags", "")
        vision = f"vision projector `{llm['mmproj']}` loaded" if llm.get("mmproj") else "no vision projector (text only)"
        spec = "speculative decoding on" if "--spec-type" in flags else "no speculative decoding"
        key = facts.get("llm_key", LLM_CONFIG)
        others = [f"`{k}` p50 {v['stats_total']['p50']:.0f} / p95 {v['stats_total']['p95']:.0f} ms"
                  + (f" (llama-server crashed after {v.get('n')} calls)" if "server_crashed" in v else "")
                  for k, v in (facts.get("tuning") or {}).items() if k != key and (v.get("stats_total") or {}).get("p50")]
        d.append(f"LLM: Qwen3.5-4B Q4_K_M, llama.cpp flags `{flags}`, ctx {_ctx(flags)}, {vision}, {spec}. Checks use "
                 f"`llm_tuning.json` entry `{key}` (p50 {llm['stats_total']['p50']:.0f} / p95 "
                 f"{llm['stats_total']['p95']:.0f} ms)."
                 + (f" Other configs measured on {gpu}: {', '.join(others)}." if others else ""))
    dec = facts.get("decider")
    if dec:
        a = dec.get("at_97")
        thr = (f"score >= {a['threshold']:g} gives {a['precision']:.1%} precision and accepts "
               f"{a['accepted_share']:.1%} of utterances") if a else "no threshold reaches 97% precision"
        d.append(f"Decider acceptance threshold (GLiNER2.5 on {dec['device']}, variant `{dec['variant']}`, label mode "
                 f"{dec['label_mode']}): {thr}. Top-1 {dec['overall']:.1%} overall, {dec['el']:.1%} Greek, "
                 f"p95 {dec['p95_ms']:.0f} ms.")
    t1, best = facts.get("t1"), facts.get("t1_best")
    if t1 and best:
        g = best["gates"]
        goal = (f"T1 bake-off (goal: p95 <= {g['p95_ms']:g} ms and, at 97% precision, >= "
                f"{g['min_accepted_share']:.0%} of utterances accepted, >= {g['min_accepted_share_el']:.0%} of Greek "
                f"ones; measured on {gpu}).")
        w = f"`{best['candidate']}`"
        if best["verdict"] == "met":
            verdict = (f"{w} meets the goal (the fastest of {len(best['met'])} that do): p95 {best['p95_ms']:.1f} ms, "
                       f"{_share_text(best)}.")
        elif best["verdict"] == "fast-only":
            verdict = (f"No candidate met the goal. Best within the {g['p95_ms']:g} ms budget (highest accepted share "
                       f"at 97% precision): {w}, p95 {best['p95_ms']:.1f} ms, {_share_text(best)}.")
        else:
            verdict = (f"No candidate met the goal and none has p95 <= {g['p95_ms']:g} ms: drop T1 in favor of the "
                       f"T0 grammar + LLM. Highest accepted share: {w} ({_share_text(best)}).")
        d.append(f"{goal} {verdict} Candidates:")
        for name, c in t1["candidates"].items():
            if "accuracy" not in c:
                d.append(f"  - `{name}`: {c.get('error', 'skipped')}")
                continue
            a = _at97(c)
            acc = (f"{a['accepted_share']:.0%} accepted overall / {(a.get('accepted_share_el') or 0):.0%} Greek at "
                   f"score {a['threshold']:g}" if a else "no 97%-precision threshold")
            d.append(f"  - `{name}`: top-1 {c['accuracy']['overall']:.0%} (Greek {c['accuracy'].get('el', 0):.0%}), "
                     f"p95 {c['latency_ms']['p95']:.1f} ms, {acc}")
    stt = facts.get("stt")
    if stt:
        gpu_lat = (f"GPU latency not counted ({stt['gpu_invalid']})" if stt.get("gpu_invalid")
                   else f"GPU p50 {stt['p50_ms_le4s']:.0f} ms on clips <= 4 s (provider {stt['provider']})")
        d.append(f"STT engine: Parakeet-TDT 0.6B v3 (ONNX via onnx-asr): WER {stt['wer_en']:.1%} English, "
                 f"{stt['wer_el']:.1%} Greek/mixed, {gpu_lat}.")
    else:
        d.append("STT engine: Parakeet-TDT 0.6B v3 (ONNX via onnx-asr), CUDA first, CPU for game mode. Verdict pending: "
                 "the WER and latency checks need voice clips (`bench/record_samples.py`, then `bench/bench_stt.py`).")
    vram = facts.get("vram")
    if vram:
        what = f"llama-server + `{vram['mmproj']}`" if vram.get("mmproj") else "llama-server (no mmproj)"
        co = (f"; co-resident p50: STT {vram['stt_p50_ms']:.0f} ms, LLM {vram['llm_p50_ms']:.0f} ms"
              if vram.get("stt_p50_ms") is not None and vram.get("llm_p50_ms") is not None
              else "; co-resident latency not measured in this run")
        bad = f" Run invalid: {vram['invalid']}." if vram.get("invalid") else ""
        d.append(f"VRAM: {what} + Parakeet used {vram['total_gib']:.2f} GiB on {gpu} ({vram.get('method')}){co}.{bad}")

    notes: list[str] = []
    if on_target(meta):
        notes.append(f"These numbers were measured on the target ({gpu}); the decisions above stand as measured.")
        return d, notes
    notes.append(f"The target's {TARGET['gpu']} has {TARGET['gpu_gb']} GB of VRAM; this run used {gpu}.")
    gl = ((t1 or {}).get("candidates") or {})
    if "error" in gl.get("gliner-cuda", {}):
        notes.append(f"GLiNER fp32 on CUDA did not load on {gpu} ({gl['gliner-cuda']['error']}). With more VRAM "
                     "it may fit next to the LLM and STT, and a faster GPU may bring its p95 under the T1 budget.")
    if llm and not llm.get("mmproj"):
        notes.append("The LLM checks ran without the vision projector; the spec budgets Qwen3.5-4B with mmproj, so the "
                     "target run must also measure `bench_llm.py --mmproj` and `bench_vram.py --mmproj`.")
    notes.append("LLM p95 and decode speed are GPU-bound; a faster GPU may close the p95 gap, and extra VRAM headroom "
                 "would allow an f16 KV cache or a longer context.")
    notes.append(f"CPU numbers (decider on CPU, STT game mode) were measured on {meta.get('cpu', 'an unknown CPU')} "
                 f"with {meta.get('ram_gib', '?')} GiB RAM; the target has a {TARGET['cpu']} and {TARGET['ram_gb']} GB.")
    notes.append("Every decision above is provisional until the full suite is re-run on the target.")
    return d, notes


def main() -> None:
    res, facts = collect()
    checks = evaluate(res)
    meta = hardware_meta()
    meta["generated"] = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    key = facts.get("llm_key")
    meta["llm_results"] = (f"bench/results/llm_tuning.json, entry `{key}` (same run as llm_{key.replace('+', '_')}.json)"
                           if key else "none (bench_llm.py not run)")
    llm_mm = (facts.get("llm") or {}).get("mmproj")
    vram_mm = (facts.get("vram") or {}).get("mmproj")
    meta["mmproj"] = (f"LLM bench: {'`' + llm_mm + '` loaded' if llm_mm else 'not loaded'}; "
                      f"VRAM bench: {'`' + vram_mm + '` loaded' if vram_mm else 'not loaded' if facts.get('vram') else 'not run'}")
    meta["decisions"], meta["laptop_notes"] = build_decisions(facts, meta)
    meta["fallbacks"] = fallback_texts(facts)
    OUT.write_text(render(checks, meta), encoding="utf-8")
    print(f"wrote {OUT}: {sum(c.passed for c in checks)}/{len(checks)} checks passed")
    for c in checks:
        print(f"  {'PASS' if c.passed else 'FAIL'}  {c.name}: {_fmt(c.value)}  ({c.threshold})")


if __name__ == "__main__":
    main()
