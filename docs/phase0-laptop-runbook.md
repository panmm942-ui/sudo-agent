# Phase 0 on the target laptop: runbook

Re-runs every Phase 0 bench on the Lenovo Legion 5 (RTX 5050 Laptop GPU 8 GB, Blackwell sm_120) and regenerates
`bench/RESULTS.md`. Commands are for **PowerShell**; Git Bash works too (`source ./dev.sh`, forward slashes).
Every command after step 2 runs from `core/` in a shell where `dev.ps1` was dot-sourced.

## Unverified until run on the laptop

None of these has been run on Blackwell hardware yet; the dev box is a GTX 1660 Super. Tick each off on the laptop:

- [ ] **Minimum driver.** 580 is a guess. Look up the minimum driver for the CUDA 13.x toolkit in NVIDIA's CUDA 13.x
  release notes, compare it with `nvidia-smi` ("Driver Version"), and correct this runbook and `models.toml`.
- [ ] **onnxruntime on Blackwell.** Check that `ParakeetStt.active_provider` reports `CUDAExecutionProvider` (not
  `CPUExecutionProvider`) with the CUDA 12 onnxruntime wheels (step 4).
- [ ] **llama.cpp cuda-13.4 build.** Check that `llama-server` starts on the RTX 5050 and offloads layers to the GPU
  (the log should show the layers offloaded and the sm_120 device); the "native sm_120" claim is unconfirmed.
- [ ] **GLiNER cu128 venv.** The step 7 recipe was never run. Check that `setup_gliner_cuda.py` completes and that
  `torch.cuda.get_arch_list()` includes `sm_120`; otherwise try the cu130 index.

## 1. Prerequisites

- **NVIDIA driver:** the required minimum is **to verify** (see the checklist above; 580 or newer is the working
  assumption for the CUDA 13.x llama.cpp build and for the onnxruntime CUDA 12 and cu128 torch wheels, none of it
  confirmed). Check yours with `nvidia-smi` (top right: "Driver Version").
- **git** and **uv** (`powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"`).
- About **15 GB free**: ~11 GB of models, the Python env and caches; +3 GB for the optional GLiNER CUDA venv (step 7).
- A microphone for step 8, and a quiet room.

## 2. Clone and environment

The repo has no remote yet. On the dev box, make a bundle: `git bundle create sudo.bundle phase-0-1`, and copy
`sudo.bundle` to the laptop. Then on the laptop:

```powershell
git clone -b phase-0-1 sudo.bundle sudo
cd sudo
. .\dev.ps1                       # sets SUDO_DATA (<repo>\.data unless D:\sudo-data exists) and all caches
cd core
uv sync --frozen
```

If PowerShell refuses to run `dev.ps1`, allow local scripts once: `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`.
To put the data elsewhere, set `$env:SUDO_DATA = 'E:\sudo-data'` before dot-sourcing `dev.ps1`.

## 3. Fetch models with the Blackwell llama.cpp build

```powershell
uv run python -m sudo_core.models.fetch ../models.toml ../models --llama-variant cuda-13.4
```

`cuda-13.4` is the llama.cpp b11982 Windows CUDA 13 build. Whether it runs natively on sm_120 is **to verify on the
laptop**: check that `llama-server` starts and offloads layers to the GPU. The default `cuda-12.4` build is for
the dev box only. Every Hugging Face file is pinned to the commit the dev box measured. `models/lock.json`
records the variant and hashes.

## 4. Test suite

```powershell
uv run pytest -q              # default suite, no models needed
uv run pytest -q -m model     # Parakeet on CUDA: must report CUDAExecutionProvider
```

## 5. LLM bench (about 5 min per config)

```powershell
uv run python ../bench/bench_llm.py --config baseline
uv run python ../bench/bench_llm.py --config baseline --mmproj
```

The first llama-server start may JIT-compile CUDA kernels (~80 s on a GPU without native kernels; the cache in
`$SUDO_DATA\cuda-cache` makes later starts fast). The `--mmproj` run measures the spec'd configuration with the
vision projector. If it fails to start (out of memory), keep the text-only numbers and say so in step 11.
Optional, if time allows: `--config kv-f16` and `--config draft4`.

## 6. Decider and T1 bake-off (CPU)

```powershell
uv run python ../bench/bench_decider.py
uv run python ../bench/bench_t1.py
```

## 7. GLiNER on CUDA (T1 probe)

```powershell
uv run python ../bench/setup_gliner_cuda.py      # once: ~3 GB venv in $SUDO_DATA\venv-gliner-cuda, cu128 torch
uv run python ../bench/bench_t1.py --candidates gliner-cuda
uv run python ../bench/bench_t1.py --candidates gliner-cuda --gliner-half
```

**To verify on the laptop:** this recipe has never been run. Check that it completes and that
`torch.cuda.get_arch_list()` lists `sm_120`. `setup_gliner_cuda.py` stops if CUDA is unavailable or the arch is
missing; for a missing arch, re-run it with `--torch-index https://download.pytorch.org/whl/cu130`. `bench_t1.py` hands the CUDA candidates to that venv on
its own.

## 8. Record the voice clips (~5 min, 40 clips)

Before recording, the Greek fixture review must be done: the clips are read from `bench/fixtures/commands.jsonl`.

```powershell
uv run python ../bench/record_samples.py
```

Pick the laptop microphone. For each line, press Enter, read the text naturally, then press Enter again. Type
`r` + Enter at "Keep?" to redo a clip. A re-run resumes where it stopped. The 40 lines are 20 English and 20
Greek/mixed. They contain no digits, URLs or AM/PM on purpose: how numbers, URLs and times are written is a
Phase 3 text-normalization concern, not an acoustic one, so WER here measures recognition only. Fillers
("um", "εε") are ignored by the WER on both sides.

## 9. STT bench

```powershell
uv run python ../bench/bench_stt.py
```

The first CUDA transcription takes about 12 s (warm-up, not counted). If the output says `INVALID RUN: ... ran on
CPU`, the CUDA provider did not load. The report then fails the GPU check instead of counting CPU numbers.
Report it in step 11.

## 10. Co-resident VRAM and latency

```powershell
uv run python ../bench/bench_vram.py --mmproj
```

This loads llama-server (with the vision projector) and Parakeet together, then times transcriptions and tool
calls while both stay loaded. If it fails to start with `--mmproj`, run `uv run python ../bench/bench_vram.py`
instead. Each run overwrites `bench/results/vram.json`.

## 11. Report, and what to send back

```powershell
uv run python ../bench/report.py
git add ../bench/RESULTS.md
git commit -m "bench: Phase 0 results on the target laptop"
git bundle create ../../results.bundle phase-0-1
```

Send back:
- `results.bundle` (next to the repo folder), which carries the committed `bench/RESULTS.md`. That file records the
  GPU, the llama.cpp build and variant, and whether mmproj was loaded;
- a zip of `bench/results/` (gitignored: the raw JSON behind every number) and `models/lock.json`;
- anything that failed or looked odd: an out-of-memory error with `--mmproj`, an `INVALID RUN` line, or a
  `setup_gliner_cuda.py` arch error.

Do not commit `bench/audio/` (your voice). It is gitignored.
