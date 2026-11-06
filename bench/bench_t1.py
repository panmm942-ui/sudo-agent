"""T1 (fast intent tier) bake-off (model run, not a pytest test).

Run from core/:
    uv run python ../bench/bench_t1.py                       # embedding candidates, CPU
GLiNER-on-CUDA needs a CUDA torch, which lives in a throwaway venv (the project venv's torch is CPU-only):
    uv run python ../bench/setup_gliner_cuda.py                 # once: creates $SUDO_DATA/venv-gliner-cuda
    uv run python ../bench/bench_t1.py --candidates gliner-cuda --gliner-half   # fp16; drop --gliner-half for fp32
CUDA candidates (gliner-cuda, e5-torch-cuda) are handed to that venv's python automatically when the current
interpreter has no CUDA torch; the venv is located through $SUDO_DATA (bench/benchenv.py).

Embedding candidates are evaluated leave-one-out: each fixture utterance is classified against the exemplars
made of every *other* fixture text (identical texts are excluded too, so there is no leakage). GLiNER needs no
exemplars and is evaluated on all utterances with Task 4's best setup (flat `en` description labels).
Latency = `.decide()` wall time per call, sequential calls (the Phase 3 call path), after 10 warm-ups.

Results are merged per candidate into bench/results/t1.json, so the two venvs can write into the same file.
`gliner-cpu-ref` is copied from bench/results/decider.json (Task 4's best CPU variant) rather than re-run.
"""

from __future__ import annotations

import argparse
import json
import platform
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "bench"))
sys.path.insert(0, str(ROOT / "core"))

from bench_decider import GATES, load_fixtures  # noqa: E402
from benchenv import gliner_venv_dir, venv_python  # noqa: E402
from sudo_core.perf import Stopwatch, summarize  # noqa: E402

RESULTS = ROOT / "bench" / "results"
MODELS = ROOT / "models"
WARMUPS = 10
COARSE = [round(0.30 + 0.05 * i, 2) for i in range(14)]  # 0.30 ... 0.95 (the brief's sweep)
FINE = [round(0.30 + 0.01 * i, 2) for i in range(70)]  # 0.30 ... 0.99 (cosine scores of e5 cluster near 0.8-0.95)
WARMUP_TEXTS = ["pause the music", "set the volume to 40", "what is the weather like", "άνοιξε το chrome", "next",
                "lock the screen", "βάλε ξυπνητήρι", "open settings", "mute", "take a screenshot"]

# name -> (model dir, backend, onnx file, query prefix, device)
EMBED = {
    "e5-onnx": ("embed-e5", "onnx", "onnx/model.onnx", "query: ", "cpu"),
    "e5-onnx-q8": ("embed-e5", "onnx", "onnx/model_qint8_avx512_vnni.onnx", "query: ", "cpu"),
    "e5-torch": ("embed-e5", "torch", None, "query: ", "cpu"),
    "e5-torch-cuda": ("embed-e5", "torch", None, "query: ", "cuda"),
    "minilm-onnx": ("embed-minilm", "onnx", "onnx/model.onnx", "", "cpu"),
    "minilm-onnx-q8": ("embed-minilm", "onnx", "onnx/model_quint8_avx2.onnx", "", "cpu"),
    "minilm-torch": ("embed-minilm", "torch", None, "", "cpu"),
}
DEFAULT_CANDIDATES = [k for k in EMBED if not k.endswith("cuda")]
CUDA_CANDIDATES = {"gliner-cuda", *(k for k in EMBED if k.endswith("cuda"))}


def cuda_torch_available() -> bool:
    try:
        import torch
    except ImportError:
        return False
    return bool(torch.cuda.is_available())


def plan_cuda_run(candidates: list[str], cuda_torch: bool) -> tuple[list[str], list[str]]:
    """-> (candidates to run in this interpreter, CUDA candidates to hand to the CUDA-torch venv)."""
    if cuda_torch:
        return list(candidates), []
    return [c for c in candidates if c not in CUDA_CANDIDATES], [c for c in candidates if c in CUDA_CANDIDATES]


def nvidia_used_mib() -> int | None:
    try:
        out = subprocess.run(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
                             capture_output=True, text=True, timeout=20).stdout
        return int(out.strip().splitlines()[0])
    except Exception:
        return None


def rss_mib() -> float | None:
    try:
        import psutil

        return psutil.Process().memory_info().rss / 2**20
    except Exception:
        return None


def dir_mib(paths: list[Path]) -> float:
    return sum(p.stat().st_size for p in paths if p.exists()) / 2**20


def sweep(preds: list[dict]) -> dict:
    def rows_for(grid: list[float]) -> list[dict]:
        out = []
        langs = Counter(p["lang"] for p in preds)
        for t in grid:
            acc = [p for p in preds if p["score"] >= t]
            correct = sum(p["pred"] == p["gold"] for p in acc)
            el_acc = [p for p in acc if p["lang"] == "el"]
            out.append({
                "threshold": t,
                "accepted": len(acc),
                "accepted_share": len(acc) / len(preds),
                "accepted_share_el": len(el_acc) / langs["el"] if langs["el"] else None,
                "precision": correct / len(acc) if acc else None,
                "precision_el": (sum(p["pred"] == p["gold"] for p in el_acc) / len(el_acc)) if el_acc else None,
            })
        return out

    def first_ok(rows: list[dict]) -> dict | None:
        ok = [r for r in rows if r["precision"] is not None and r["precision"] >= GATES["precision"]]
        return ok[0] if ok else None

    coarse, fine = rows_for(COARSE), rows_for(FINE)
    return {"rows": coarse, "at_97_precision": first_ok(coarse), "at_97_precision_fine": first_ok(fine)}


def summarize_run(preds: list[dict], lats: list[float]) -> dict:
    per_lang: dict[str, list[bool]] = defaultdict(list)
    for p in preds:
        per_lang[p["lang"]].append(p["pred"] == p["gold"])
    st = summarize(lats)
    confusions = Counter((p["gold"], p["pred"]) for p in preds if p["gold"] != p["pred"])
    return {
        "accuracy": {"overall": sum(p["pred"] == p["gold"] for p in preds) / len(preds),
                     **{lang: sum(v) / len(v) for lang, v in sorted(per_lang.items())}},
        "latency_ms": {"n": st.n, "mean": st.mean, "p50": st.p50, "p95": st.p95, "max": max(lats)},
        "top_confusions": [{"gold": g, "pred": p, "count": c} for (g, p), c in confusions.most_common(10)],
        "sweep": sweep(preds),
        "predictions": preds,
    }


def run_embed(name: str, rows: list[dict], threads: int) -> dict:
    from sudo_core.brain.embed_router import EmbedRouter, load_encoder

    mdir, backend, onnx_file, prefix, device = EMBED[name]
    model_dir = MODELS / mdir
    exemplars: dict[str, list[str]] = defaultdict(list)
    for r in rows:
        exemplars[r["intent"]].append(r["text"])

    vram0, rss0 = nvidia_used_mib(), rss_mib()
    with Stopwatch() as load:
        kw = {"onnx_file": onnx_file} if onnx_file else {}
        encoder = load_encoder(model_dir, device=device, num_threads=threads, backend=backend, **kw)
    with Stopwatch() as build:
        router = EmbedRouter(model_dir, dict(exemplars), device=device, encoder=encoder, query_prefix=prefix)
    for i in range(WARMUPS):
        router.decide(WARMUP_TEXTS[i % len(WARMUP_TEXTS)])

    preds, lats = [], []
    for r in rows:
        d = router.decide(r["text"], exclude_text=r["text"])
        preds.append({"id": r["id"], "lang": r["lang"], "gold": r["intent"], "pred": d.intent, "score": d.score})
        lats.append(d.latency_ms)
    vram1, rss1 = nvidia_used_mib(), rss_mib()
    files = [model_dir / (onnx_file or "model.safetensors")]
    res = summarize_run(preds, lats)
    res.update({
        "kind": "embedding-knn", "model": mdir, "backend": backend, "onnx_file": onnx_file, "device": device,
        "threads": threads, "query_prefix": prefix, "eval": "leave-one-out", "n_exemplars": len(rows),
        "load_ms": load.ms, "exemplar_embed_ms": build.ms,
        "disk_mib": dir_mib(files),
        "vram_delta_mib": (vram1 - vram0) if device == "cuda" and vram0 is not None and vram1 is not None else None,
        "rss_delta_mib": (rss1 - rss0) if rss0 is not None and rss1 is not None else None,
    })
    return res


_ST_DTYPES = {"F32": "float32", "F16": "float16", "BF16": "bfloat16", "I64": "int64", "I32": "int32",
              "I16": "int16", "I8": "int8", "U8": "uint8", "BOOL": "bool", "F64": "float64"}


def _stream_safetensors_to_cuda(path: str, chunk: int = 32 << 20, half: bool = False) -> dict:
    """Read a .safetensors file straight into CUDA tensors in small chunks (no mmap, ~chunk bytes of host RAM)."""
    import struct

    import torch

    out = {}
    with open(path, "rb") as f:
        (n,) = struct.unpack("<Q", f.read(8))
        header = json.loads(f.read(n))
        base = 8 + n
        buf = bytearray(chunk)
        for name, meta in header.items():
            if name == "__metadata__":
                continue
            start, end = meta["data_offsets"]
            src = getattr(torch, _ST_DTYPES[meta["dtype"]])
            dst = torch.float16 if half and src.is_floating_point else src
            numel = (end - start) // torch.empty(0, dtype=src).element_size()
            t = torch.empty(numel, dtype=dst, device="cuda")
            f.seek(base + start)
            step = chunk - chunk % torch.empty(0, dtype=src).element_size()
            done = 0  # elements
            while done < numel:
                nbytes = min(step, (numel - done) * torch.empty(0, dtype=src).element_size())
                k = f.readinto(memoryview(buf)[:nbytes])
                part = torch.frombuffer(buf, dtype=torch.uint8, count=k).to("cuda").view(src)
                t[done : done + part.numel()].copy_(part)  # copy_ converts dtype (fp32 -> fp16) chunk by chunk
                done += part.numel()
            out[name] = t.reshape(meta["shape"])
    return out


def load_gliner_cuda_low_commit(half: bool = False):
    """GLiNER on CUDA when the Windows commit limit is nearly exhausted (os error 1455 'paging file too small').

    The stock loader builds the fp32 model on the host, then mmaps and copies the 1.1 GB checkpoint, which needs
    ~2.5 GB of commit on top of CUDA torch's ~1.5 GB. Here the model is constructed directly on the GPU and the
    checkpoint is streamed into GPU tensors, so host commit stays tiny. Same weights, same fp32 numerics.
    """
    import torch
    from gliner2 import AutoExtractor
    from gliner2.models.boundary import model as boundary_model

    from gliner2.models import base as gliner_base

    original = boundary_model.load_checkpoint_state_dict
    orig_from_config = gliner_base.AutoModel.from_config
    # the encoder is serialised as bf16 and gliner2 then calls .float(), briefly holding both copies (~1.1 GB)
    dt = torch.float16 if half else torch.float32

    def from_config(config, **kw):
        enc = orig_from_config(config, dtype=dt, **kw)
        if half:
            enc.float = lambda *a, **k: enc  # keep the encoder fp16 (gliner2 would copy it back to fp32)
        return enc

    gliner_base.AutoModel.from_config = staticmethod(from_config)
    boundary_model.load_checkpoint_state_dict = lambda repo_or_dir, hub_kwargs=None: _stream_safetensors_to_cuda(
        str(Path(repo_or_dir) / "model.safetensors"), half=half)
    try:
        with torch.device("cuda"):
            extractor = AutoExtractor.from_pretrained(str(MODELS / "decider"))
    finally:
        boundary_model.load_checkpoint_state_dict = original
        gliner_base.AutoModel.from_config = orig_from_config
    if half:
        extractor.half()
    extractor.to("cuda")
    extractor.eval()
    return extractor


def run_gliner_cuda(rows: list[dict], intents: dict, loader: str, half: bool) -> dict:
    import torch
    from sudo_core.brain.decider import Decider

    labels = {k: v["en"] for k, v in intents.items()}
    vram0, rss0 = nvidia_used_mib(), rss_mib()
    torch.cuda.reset_peak_memory_stats()
    with Stopwatch() as load:
        if loader == "stock":
            decider = Decider(str(MODELS / "decider"), device="cuda", label_mode="descriptions")
        else:  # the stock loader segfaults / raises os error 1455 when the Windows commit limit is nearly used up
            decider = Decider(extractor=load_gliner_cuda_low_commit(half), label_mode="descriptions")
    vram_loaded = nvidia_used_mib()
    for i in range(WARMUPS):
        decider.decide(WARMUP_TEXTS[i % len(WARMUP_TEXTS)], labels)
    preds, lats = [], []
    for r in rows:
        d = decider.decide(r["text"], labels)
        preds.append({"id": r["id"], "lang": r["lang"], "gold": r["intent"], "pred": d.intent, "score": d.score})
        lats.append(d.latency_ms)
    vram1, rss1 = nvidia_used_mib(), rss_mib()
    res = summarize_run(preds, lats)
    res.update({
        "kind": "gliner", "model": "fastino/GLiNER2.5-multi-Decide", "device": "cuda", "label_mode": "descriptions",
        "labels": "en (flat)", "eval": "all utterances (no exemplars)", "loader": loader, "dtype": "float16" if half else "float32", "torch": torch.__version__,
        "gpu": torch.cuda.get_device_name(0), "load_ms": load.ms,
        "disk_mib": sum(p.stat().st_size for p in (MODELS / "decider").rglob("*")
                        if p.is_file() and ".cache" not in p.parts) / 2**20,
        "vram_delta_loaded_mib": (vram_loaded - vram0) if None not in (vram0, vram_loaded) else None,
        "vram_delta_after_run_mib": (vram1 - vram0) if None not in (vram0, vram1) else None,
        "torch_max_allocated_mib": torch.cuda.max_memory_allocated() / 2**20,
        "torch_max_reserved_mib": torch.cuda.max_memory_reserved() / 2**20,
        "rss_delta_mib": (rss1 - rss0) if rss0 is not None and rss1 is not None else None,
    })
    return res


def gliner_cpu_reference() -> dict | None:
    path = RESULTS / "decider.json"
    if not path.exists():
        return None
    v = json.loads(path.read_text(encoding="utf-8"))["variants"]["en_flat"]
    preds = v["predictions"]
    res = summarize_run(preds, [v["latency_ms"]["mean"]] * 2)  # recompute accuracy/sweep; latency copied below
    res["latency_ms"] = v["latency_ms"]
    res.update({"kind": "gliner", "model": "fastino/GLiNER2.5-multi-Decide", "device": "cpu", "label_mode": "descriptions",
                "labels": "en (flat)", "eval": "all utterances (no exemplars)",
                "note": "copied from bench/results/decider.json (Task 4, 4 torch threads); not re-run"})
    return res


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--candidates", default=",".join(DEFAULT_CANDIDATES) + ",gliner-cpu-ref")
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--gliner-half", action="store_true", help="run GLiNER in fp16 on CUDA (halves VRAM / commit)")
    ap.add_argument("--gliner-loader", choices=["low-commit", "stock"], default="low-commit")
    ap.add_argument("--out", default=str(RESULTS / "t1.json"))
    args = ap.parse_args()

    intents, rows = load_fixtures()
    out_path = Path(args.out)
    doc = json.loads(out_path.read_text(encoding="utf-8")) if out_path.exists() else {"candidates": {}}
    doc["gates"] = {**GATES, "min_accepted_share": 0.70, "min_accepted_share_el": 0.60}
    doc["n_commands"] = len(rows)
    doc["platform"] = platform.platform()

    def save(name: str, res: dict) -> None:
        doc["candidates"][name + ("-fp16" if name == "gliner-cuda" and args.gliner_half else "")] = res
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(doc, indent=2, ensure_ascii=False), encoding="utf-8")

    candidates = [c.strip() for c in args.candidates.split(",") if c.strip()]
    wants_cuda = any(c in CUDA_CANDIDATES for c in candidates)
    here, in_venv = plan_cuda_run(candidates, cuda_torch=cuda_torch_available() if wants_cuda else True)
    if in_venv:
        venv_py = venv_python(gliner_venv_dir())
        if venv_py.exists() and Path(sys.executable).resolve() != venv_py.resolve():
            print(f"no CUDA torch in this interpreter; running {in_venv} with {venv_py}", flush=True)
        else:
            why = (f"torch.cuda.is_available() is False in {sys.executable} (driver too old for this torch build?)"
                   if venv_py.exists() else
                   f"no CUDA torch here and no venv at {venv_py}: run `uv run python ../bench/setup_gliner_cuda.py`")
            print(f"skipping {in_venv}: {why}", file=sys.stderr, flush=True)
            for name in in_venv:
                save(name, {"error": f"skipped: {why}"})
            in_venv = []

    for name in here:
        print(f"== {name}", flush=True)
        try:
            if name in EMBED:
                res = run_embed(name, rows, args.threads)
            elif name == "gliner-cuda":
                res = run_gliner_cuda(rows, intents, args.gliner_loader, args.gliner_half)
            elif name == "gliner-cpu-ref":
                res = gliner_cpu_reference()
                if res is None:
                    print("  no decider.json; skipped")
                    continue
            else:
                raise ValueError(f"unknown candidate {name}")
        except Exception as e:  # record the failure, keep going
            import traceback

            traceback.print_exc()
            res = {"error": f"{type(e).__name__}: {e}"}
        else:
            a, lat = res["accuracy"], res["latency_ms"]
            s = res["sweep"]
            print(f"  acc={a}\n  p50={lat['p50']:.1f} p95={lat['p95']:.1f} ms\n"
                  f"  @97%prec coarse={s['at_97_precision']}\n  @97%prec fine={s['at_97_precision_fine']}", flush=True)
        save(name, res)
    if here:
        print("wrote", out_path)
    if in_venv:  # the venv run re-reads and merges into the same t1.json
        argv = [str(venv_python(gliner_venv_dir())), str(Path(__file__).resolve()), "--candidates", ",".join(in_venv),
                "--threads", str(args.threads), "--gliner-loader", args.gliner_loader, "--out", str(out_path)]
        sys.exit(subprocess.run(argv + (["--gliner-half"] if args.gliner_half else [])).returncode)


if __name__ == "__main__":
    main()
