# Phase 0 benchmark results: go/no-go

> **Provisional: not measured on the target machine.** These numbers come from NVIDIA GeForce GTX 1660 SUPER, not from the target (Lenovo Legion 5 laptop: Ryzen 7, RTX 5050 Laptop GPU 8 GB, 32 GB RAM). The whole Phase 0 suite will be re-run there before the go/no-go is final.

Generated: 2026-10-30 00:28. **8/19 checks passed.**

## Hardware and build (detected at runtime)

- GPU: NVIDIA GeForce GTX 1660 SUPER
- GPU VRAM (MiB): 6144
- CPU: Intel(R) Core(TM) i5-10400 CPU @ 2.90GHz
- RAM (GiB): 15.9
- OS: Windows-11-10.0.26300-SP0
- Python: 3.12.13
- llama.cpp build: version: 0.8.0-dev (build 11982, commit 8a199ad31)
- llama.cpp asset variant: not recorded in models/lock.json (assets: llama-b11982-bin-win-cuda-12.4-x64.zip, cudart-llama-bin-win-cuda-12.4-x64.zip)
- Vision projector (mmproj): LLM bench: not loaded; VRAM bench: not loaded
- LLM results used: bench/results/llm_tuning.json, entry `baseline` (same run as llm_baseline.json)

## Go/no-go table

| Check | Value | Threshold | Result |
|---|---:|---|---|
| LLM total p50 (ms) | 599.0 | <= 700 | PASS |
| LLM total p95 (ms) | 1,184 | <= 1000 | FAIL |
| LLM valid tool call (%) | 100.0 | >= 95 | PASS |
| LLM correct tool (%) | 97.0 | >= 90 | PASS |
| LLM decode (tok/s) | 65.1 | >= 35 | PASS |
| Decider top-1 overall (%) | 85.2 | >= 85 | PASS |
| Decider top-1 Greek (%) | 78.6 | >= 80 | FAIL |
| Decider p95 latency, CPU (ms) | 351.5 | <= 80 | FAIL |
| Decider accepted share at >=97% precision (%) | 74.2 | reachable at score >= 0.7 | PASS |
| T1 p95 latency, `minilm-onnx` (ms) | 7.4 | <= 80 | PASS |
| T1 accepted at >=97% precision, overall, `minilm-onnx` (%) | 38.1 | >= 70 | FAIL |
| T1 accepted at >=97% precision, Greek, `minilm-onnx` (%) | 35.7 | >= 60 | FAIL |
| STT WER English (%) | n/a | <= 10 (not run: no voice clips recorded yet) | FAIL |
| STT WER Greek (%) | n/a | <= 15 (not run: no voice clips recorded yet) | FAIL |
| STT GPU p50, clips <= 4 s (ms) | n/a | <= 150 (not run: no voice clips recorded yet) | FAIL |
| STT CPU p50 (ms, game mode) | n/a | recorded (not run: no voice clips recorded yet) | FAIL |
| VRAM llama-server (no mmproj) + Parakeet (GiB) | 4.3 | <= 4.6 | PASS |
| STT GPU p50 while LLM loaded (ms) | n/a | <= 150 (not run) | FAIL |
| LLM total p50 while STT loaded (ms) | n/a | <= 700 (not run) | FAIL |

## Decisions

- LLM: Qwen3.5-4B Q4_K_M, llama.cpp flags `-fa on --cache-type-k q8_0 --cache-type-v q8_0`, ctx 8192, no vision projector (text only), no speculative decoding. Checks use `llm_tuning.json` entry `baseline` (p50 599 / p95 1184 ms). Other configs measured on NVIDIA GeForce GTX 1660 SUPER (6 GB): `draft` p50 1020 / p95 2173 ms, `ngram` p50 608 / p95 1270 ms, `ngram-n3` p50 970 / p95 2085 ms, `ngram-mapk3` p50 678 / p95 1342 ms, `draft4` p50 536 / p95 928 ms (llama-server crashed after 40 calls), `kv-f16` p50 615 / p95 1218 ms, `draft+ngram` p50 626 / p95 920 ms (llama-server crashed after 26 calls).
- Decider acceptance threshold (GLiNER2.5 on cpu, variant `en_flat`, label mode descriptions): score >= 0.7 gives 97.1% precision and accepts 74.2% of utterances. Top-1 85.2% overall, 78.6% Greek, p95 351 ms.
- T1 bake-off (goal: p95 <= 80 ms and, at 97% precision, >= 70% of utterances accepted, >= 60% of Greek ones; measured on NVIDIA GeForce GTX 1660 SUPER (6 GB)). No candidate met the goal. Best within the 80 ms budget (highest accepted share at 97% precision): `minilm-onnx`, p95 7.4 ms, 38% accepted overall / 36% Greek at score 0.85. Candidates:
  - `e5-onnx`: top-1 71% (Greek 74%), p95 7.4 ms, 6% accepted overall / 7% Greek at score 0.96
  - `e5-torch`: top-1 71% (Greek 74%), p95 16.3 ms, 6% accepted overall / 7% Greek at score 0.96
  - `minilm-onnx`: top-1 82% (Greek 73%), p95 7.4 ms, 38% accepted overall / 36% Greek at score 0.85
  - `minilm-torch`: top-1 82% (Greek 73%), p95 13.4 ms, 38% accepted overall / 36% Greek at score 0.85
  - `gliner-cuda`: fp32 skipped: torch.OutOfMemoryError allocating a 734 MiB embedding matrix on the 6 GB GTX 1660 Super (WDDM, ~5 GB reported free; Windows commit limit also tight). fp16 variant (gliner-cuda-fp16) ran instead.
  - `gliner-cuda-fp16`: top-1 85% (Greek 79%), p95 148.4 ms, 74% accepted overall / 62% Greek at score 0.7
  - `e5-onnx-q8`: top-1 69% (Greek 73%), p95 5.4 ms, 11% accepted overall / 15% Greek at score 0.95
  - `minilm-onnx-q8`: top-1 82% (Greek 73%), p95 5.5 ms, 36% accepted overall / 35% Greek at score 0.85
  - `gliner-cpu-ref`: top-1 85% (Greek 79%), p95 351.5 ms, 74% accepted overall / 62% Greek at score 0.7
- STT engine: Parakeet-TDT 0.6B v3 (ONNX via onnx-asr), CUDA first, CPU for game mode. Verdict pending: the WER and latency checks need voice clips (`bench/record_samples.py`, then `bench/bench_stt.py`).
- VRAM: llama-server (no mmproj) + Parakeet used 4.31 GiB on NVIDIA GeForce GTX 1660 SUPER (6 GB) (total-used minus idle (per-process is N/A under WDDM)); co-resident latency not measured in this run.

### Failed checks and fallbacks

- **LLM total p95 (ms)** (1,184 vs <= 1000): fallback: try Q4_K_S / IQ4_XS, reduce the tools sent per call (route by T1 first), shorten slot encodings, or show the 'acting' state at the tool name (name_ms) and finish the arguments in the background.
- **Decider top-1 Greek (%)** (78.6 vs >= 80): fallback: add Greek label descriptions or move those intents to the T0 grammar.
- **Decider p95 latency, CPU (ms)** (351.5 vs <= 80): fallback: run the decider on GPU (needs VRAM headroom) or use the MiniLM ONNX embedding router (T1 candidate, p95 7.4 ms; it accepts 38% of utterances (36% of Greek ones) at >= 97% precision, the rest go to the LLM).
- **T1 accepted at >=97% precision, overall, `minilm-onnx` (%)** (38.1 vs >= 70): fallback: accept a smaller T1 share (the rest goes to the LLM), add exemplars for the weak intents, or drop T1 in favor of the T0 grammar + LLM.
- **T1 accepted at >=97% precision, Greek, `minilm-onnx` (%)** (35.7 vs >= 60): fallback: accept a smaller T1 share (the rest goes to the LLM), add exemplars for the weak intents, or drop T1 in favor of the T0 grammar + LLM.
- **STT WER English (%)** (n/a vs <= 10 (not run: no voice clips recorded yet)): fallback: tune audio capture (gain, VAD padding) or switch to whisper large-v3-turbo.
- **STT WER Greek (%)** (n/a vs <= 15 (not run: no voice clips recorded yet)): fallback: switch to faster-whisper large-v3-turbo for Greek (plan Task 6 Step 7).
- **STT GPU p50, clips <= 4 s (ms)** (n/a vs <= 150 (not run: no voice clips recorded yet)): fallback: check the CUDA provider really loaded (no CPU fallback: cuDNN / driver / GPU arch); trim VAD tail padding.
- **STT CPU p50 (ms, game mode)** (n/a vs recorded (not run: no voice clips recorded yet)): fallback: record the clips and run bench_stt.py with --devices cuda cpu.
- **STT GPU p50 while LLM loaded (ms)** (n/a vs <= 150 (not run)): fallback: VRAM is likely over-committed (WDDM pages to system RAM): free VRAM (ctx 4096, smaller quant, no mmproj) or run STT on CPU while the LLM is loaded.
- **LLM total p50 while STT loaded (ms)** (n/a vs <= 700 (not run)): fallback: VRAM is likely over-committed (WDDM pages to system RAM): free VRAM (ctx 4096, smaller quant, no mmproj) or run STT on CPU while the LLM is loaded.

### Where the target machine may change a decision

- The target's RTX 5050 Laptop GPU has 8 GB of VRAM; this run used NVIDIA GeForce GTX 1660 SUPER (6 GB).
- GLiNER fp32 on CUDA did not load on NVIDIA GeForce GTX 1660 SUPER (6 GB) (fp32 skipped: torch.OutOfMemoryError allocating a 734 MiB embedding matrix on the 6 GB GTX 1660 Super (WDDM, ~5 GB reported free; Windows commit limit also tight). fp16 variant (gliner-cuda-fp16) ran instead.). With more VRAM it may fit next to the LLM and STT, and a faster GPU may bring its p95 under the T1 budget.
- The LLM checks ran without the vision projector; the spec budgets Qwen3.5-4B with mmproj, so the target run must also measure `bench_llm.py --mmproj` and `bench_vram.py --mmproj`.
- LLM p95 and decode speed are GPU-bound; a faster GPU may close the p95 gap, and extra VRAM headroom would allow an f16 KV cache or a longer context.
- CPU numbers (decider on CPU, STT game mode) were measured on Intel(R) Core(TM) i5-10400 CPU @ 2.90GHz with 15.9 GiB RAM; the target has a Ryzen 7 and 32 GB.
- Every decision above is provisional until the full suite is re-run on the target.
