# Phase 0 — Model Benchmarks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure, on the user's real hardware and voice, whether the chosen local models meet sudo's latency/accuracy/VRAM budgets, and produce a go/no-go report.

**Architecture:** A `uv`-managed Python 3.12 package `sudo_core` (under `core/`) gets the first reusable adapters — perf stats, model fetcher, llama-server process wrapper, LLM tool-call client, GLiNER decider, Parakeet STT. Thin scripts in `bench/` drive those adapters over a shared bilingual fixture set and write JSON results; `bench/report.py` renders `bench/RESULTS.md`.

**Tech Stack:** Python 3.12 (uv), llama.cpp `llama-server` (Windows CUDA 12.4 build), `gliner2[local]`, `onnx-asr` + `onnxruntime-gpu[cuda,cudnn]`, `huggingface_hub`, `httpx`, `jiwer`, `sounddevice`, `soundfile`, `numpy`, `pytest`.

**Spec:** `docs/superpowers/specs/2026-10-29-sudo-design.md` (§2 model stack, §3 latency budgets, §10 phase 0). Note: the WGC→WebView capture measurement moved to Phase 1 Task 5 because it needs the Tauri app.

## Global Constraints

- All models run locally; the only network use in this phase is downloading weights/binaries.
- Python 3.12 via `uv` (system Python 3.14 is not used). All paths are relative to the repo root (wherever it is cloned); Python project at `core/`. Caches and heavy data go under `$SUDO_DATA` (set by `dev.ps1` / `dev.sh`).
- Models and binaries live in `models/` (gitignored); exact sources pinned in `models.toml`:
  - LLM: `unsloth/Qwen3.5-4B-GGUF` → `Qwen3.5-4B-Q4_K_M.gguf` + `mmproj-F16.gguf`
  - Decider: `fastino/GLiNER2.5-multi-Decide` (load via `gliner2.AutoExtractor.from_pretrained`)
  - STT: `istupakov/parakeet-tdt-0.6b-v3-onnx` (load via `onnx_asr.load_model("nemo-parakeet-tdt-0.6b-v3", path)`)
  - llama.cpp: GitHub `ggml-org/llama.cpp` release asset `llama-<tag>-bin-win-cuda-12.4-x64.zip` + `cudart-llama-bin-win-cuda-12.4-x64.zip`; pin the tag used.
- llama-server flags: `--host 127.0.0.1 --port 8765 --jinja -c 8192 -ngl 99 -fa on --cache-type-k q8_0 --cache-type-v q8_0 --chat-template-kwargs {"enable_thinking":false}`.
- Intent ids use dots (`media.next`); LLM tool names are the same id with `.` → `_` (`media_next`).
- Go/no-go thresholds (from spec §3 budgets):
  - LLM tool call: total p50 ≤ 700 ms, p95 ≤ 1000 ms; valid tool call ≥ 95 %; correct tool ≥ 90 %; decode ≥ 35 tok/s.
  - Decider (CPU): top-1 ≥ 85 % overall and ≥ 80 % Greek; p95 ≤ 80 ms; report the score threshold giving ≥ 97 % precision and the share of utterances accepted at it.
  - STT (GPU): WER EN ≤ 10 %, GR ≤ 15 %; p50 ≤ 150 ms for clips ≤ 4 s; CPU latency also recorded (game mode).
  - VRAM: llama-server + Parakeet loaded together ≤ 4.6 GB.
- Model-dependent tests are marked `@pytest.mark.model` and excluded by default (`addopts = "-m 'not model'"`).

## Review Focus

- **Greek with tonos/final sigma and mixed EN/GR** ("άνοιξε το Spotify"): WER must not count accent or case differences as errors → `normalize()` test in Task 6.
- **llama-server fails to start** (missing cudart DLLs, port busy, flash-attn unsupported on Turing): wrapper must raise a clear error with the server's stderr tail, not hang → test in Task 3.
- **Model outputs no tool call or malformed JSON arguments**: counted as invalid, never crashes the bench → test in Task 3.
- **Silent/near-silent recordings** (mic muted, wrong input device): recorder rejects clips with RMS below −50 dBFS and re-prompts → test in Task 6.
- **Decider returns no score for a label set / unknown label**: mapped to `Decision(intent="chat", score=0.0)` → test in Task 4.

---

### Task 1: Python project, perf stats, model manifest & fetcher

**Files:**
- Create: `core/pyproject.toml`, `core/sudo_core/__init__.py`, `core/sudo_core/perf.py`, `core/sudo_core/models/fetch.py`, `models.toml`, `core/tests/test_perf.py`, `core/tests/test_fetch.py`
- Modify: `.gitignore` (add `bench/audio/`, `bench/results/`)

**Interfaces:**
- Produces: `perf.Stopwatch` (context manager; `.ms: float` after exit); `perf.summarize(samples: list[float]) -> Stats` with `Stats(n: int, mean: float, p50: float, p95: float)` (numpy linear percentiles); `fetch.load_manifest(path: Path) -> list[ModelEntry]`; `ModelEntry(name: str, kind: Literal["hf_file","hf_repo","github_release"], repo: str, files: list[str], pin: str | None)`; `fetch.fetch_all(manifest: Path, dest: Path) -> dict[str, Path]` (name → local path), writing `models/lock.json` with sha256 per file.

- [ ] **Step 1:** `uv python install 3.12`; create `core/pyproject.toml` (requires-python `==3.12.*`, deps from Tech Stack, `[tool.pytest.ini_options] addopts = "-m 'not model'"`, markers `model`); `cd core && uv sync`. Expected: `.venv` created, `uv run python -c "import onnxruntime, gliner2, onnx_asr"` succeeds.
- [ ] **Step 2: Write failing tests**

```python
def test_summarize_percentiles():
    s = summarize([float(x) for x in range(1, 11)])
    assert (s.n, s.mean, s.p50, round(s.p95, 2)) == (10, 5.5, 5.5, 9.55)

def test_summarize_empty_raises():
    with pytest.raises(ValueError): summarize([])

def test_load_manifest_reads_all_entries(tmp_path):
    # writes a 2-entry toml (hf_file + github_release) and asserts names, kinds, files, pin
```

- [ ] **Step 3:** Run `cd core && uv run pytest tests/test_perf.py tests/test_fetch.py -v` → FAIL (import errors).
- [ ] **Step 4:** Implement `perf.py` and `fetch.py`. `hf_file` uses `hf_hub_download`; `hf_repo` uses `snapshot_download`; `github_release` downloads the named assets for tag `pin` (or latest, then records the tag in `lock.json`) and unzips both zips into `models/llama.cpp/`. Write `models.toml` with the four entries from Global Constraints.
- [ ] **Step 5:** Run tests → PASS. Then `uv run python -m sudo_core.models.fetch ../models.toml ../models` → all files present; `models/llama.cpp/llama-server.exe --version` prints the build tag.
- [ ] **Step 6:** Commit `feat(core): uv project, perf stats, model manifest + fetcher`.

### Task 2: Bilingual command fixture set

**Files:**
- Create: `bench/fixtures/intents.json`, `bench/fixtures/commands.jsonl`, `core/tests/test_fixtures.py`

**Interfaces:**
- Produces: `intents.json` = `{ "<intent id>": {"en": "<label description>", "el": "<Greek description>", "params": {<JSON-schema properties>}} }` for exactly these 20 intents: `media.play_pause, media.next, media.previous, volume.set, volume.mute, app.open, app.close, window.snap, web.search, web.open, alarm.set, timer.set, reminder.create, schedule.query, calendar.create, email.summary, system.lock, system.screenshot, settings.open, chat`. `commands.jsonl` lines: `{"id": "en-001", "text": str, "lang": "en"|"el"|"mixed", "intent": str, "slots": dict}`.

- [ ] **Step 1: Write failing test** `test_fixture_coverage`: ≥150 lines; ids unique; every intent in `intents.json`; for each intent ≥3 `en` and ≥3 `el`/`mixed`; ≥15 `mixed` overall; every `slots` key exists in that intent's `params`.
- [ ] **Step 2:** Run `uv run pytest tests/test_fixtures.py -v` → FAIL (files missing).
- [ ] **Step 3:** Write the fixtures: natural, varied phrasings (imperative, polite, casual, with fillers like "sudo, um…"), realistic slots (`volume.set {"level": 30}`, `app.open {"app": "Discord"}`, `alarm.set {"time": "07:00"}`), Greek in natural Greek (with tonos), `chat` = open questions/small talk. Ask the user to skim the Greek lines for naturalness before committing.
- [ ] **Step 4:** Run test → PASS.
- [ ] **Step 5:** Commit `test(bench): bilingual intent fixture set`.

### Task 3: llama-server wrapper, tool-call client, LLM bench

**Files:**
- Create: `core/sudo_core/llm/server.py`, `core/sudo_core/llm/client.py`, `core/sudo_core/llm/tools.py`, `bench/bench_llm.py`, `core/tests/test_llm_client.py`, `core/tests/test_llm_server.py`

**Interfaces:**
- Consumes: `perf.summarize`, `intents.json`.
- Produces: `LlamaServer(exe: Path, model: Path, mmproj: Path | None, port: int = 8765, ctx: int = 8192)` with `.start(timeout_s: float = 60) -> None` (polls `GET /health` until 200; raises `LlamaServerError(stderr_tail: str)` if the process exits or times out), `.stop()`, context-manager support. `tools.intents_to_tools(intents: dict) -> list[dict]` (OpenAI function schemas, names `.`→`_`). `LlmClient(base_url: str, http: httpx.Client | None = None)` with `.tool_call(messages: list[dict], tools: list[dict]) -> ToolCallResult` where `ToolCallResult(name: str | None, arguments: dict | None, valid: bool, ttft_ms: float, total_ms: float, completion_tokens: int)`; streams with `tool_choice: "required"`, `temperature: 0`, `stream_options.include_usage: true`.

- [ ] **Step 1: Write failing tests** (httpx `MockTransport` replaying SSE chunks):
  - `test_tool_call_assembles_streamed_arguments` — deltas `{"name":"volume_set"}`, `'{"lev'`, `'el": 30}'` → `name == "volume_set"`, `arguments == {"level": 30}`, `valid is True`, `ttft_ms > 0`.
  - `test_malformed_arguments_are_invalid_not_raised` — arguments `'{"level": }'` → `valid is False`, `arguments is None`.
  - `test_no_tool_call_is_invalid` — content-only stream → `name is None`, `valid is False`.
  - `test_intents_to_tools_names` — `"media.next"` → `"media_next"`, params copied.
  - `test_server_start_raises_with_stderr_when_process_exits` — exe = `sys.executable` with args that print "CUDA error" to stderr and exit 1 → `LlamaServerError` whose `stderr_tail` contains "CUDA error" (inject args via an `_argv()` override).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement. System prompt for the bench (also reused in Phase 3): "You are sudo, a fast desktop assistant. Pick exactly one tool for the user's request. Users speak English or Greek." Prompt-cache by keeping system prompt + tools byte-identical across calls.
- [ ] **Step 4:** Run tests → PASS.
- [ ] **Step 5:** `bench/bench_llm.py`: start server, 3 warm-up calls, then every fixture line once → `bench/results/llm.json` with `{stats_total, stats_ttft, decode_tok_s, valid_rate, correct_rate, per_lang:{…}, failures:[…]}`. Decode tok/s from a separate 256-token free-text generation using server `timings.predicted_per_second`. Run `uv run python ../bench/bench_llm.py` (marked as a model run). Expected: JSON written; numbers printed against thresholds.
- [ ] **Step 6:** If `-fa on` fails on the 1660 Super, rerun with `-fa off --cache-type-k f16 --cache-type-v f16` and record which config was used in `llm.json`.
- [ ] **Step 7:** Commit `feat(core): llama-server wrapper + streaming tool-call client; LLM bench`.

### Task 4: GLiNER2.5-multi-Decide adapter & router bench

**Files:**
- Create: `core/sudo_core/brain/decider.py`, `bench/bench_decider.py`, `core/tests/test_decider.py`

**Interfaces:**
- Produces: `Decider(model_id: str = "fastino/GLiNER2.5-multi-Decide", device: str = "cpu", extractor=None)` (inject a fake `extractor` in tests); `.decide(text: str, labels: dict[str, str]) -> Decision` where `labels` maps intent id → description and `Decision(intent: str, score: float, latency_ms: float)`. Calls `extractor.classify_text(text, {"intent": {"labels": {id: desc…}}}, include_confidence=True)` — confirm the exact confidence kwarg / return shape against the model card's `SKILL.md` and the `gliner2` source before implementing, and pin it in a docstring.

- [ ] **Step 1: Write failing tests** with a fake extractor:
  - `test_decide_maps_label_and_score` — fake returns `{"intent": {"label": "media.next", "confidence": 0.91}}` → `Decision("media.next", 0.91, …)`.
  - `test_unknown_or_missing_label_falls_back_to_chat` — fake returns `{"intent": None}` or a label not in `labels` → `Decision("chat", 0.0, …)`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [ ] **Step 5:** `bench/bench_decider.py`: load on CPU, warm up 5 calls, run all fixtures with `labels = {id: intents[id]["en"]}` and again with `"en" + " / " + "el"` descriptions → `bench/results/decider.json`: accuracy overall / per lang, latency stats, confusion pairs, and a precision-vs-threshold sweep (0.3…0.95 step 0.05) reporting the lowest threshold with ≥ 97 % precision and accepted share. Also time `extract_entities(text, ["app", "number", "time", "query", "url"])` on the same model as a slot-extraction probe and record latency + slot hit rate.
- [ ] **Step 6:** Commit `feat(core): GLiNER2.5-multi-Decide adapter; router bench`.

### Task 5: Parakeet STT adapter

**Files:**
- Create: `core/sudo_core/stt/parakeet.py`, `core/tests/test_parakeet.py`

**Interfaces:**
- Produces: `ParakeetStt(model_dir: Path, device: Literal["cuda","cpu"] = "cuda")` with `.transcribe(audio: np.ndarray, sample_rate: int = 16000) -> Transcript` where `Transcript(text: str, latency_ms: float)`. Resamples non-16 kHz input; converts stereo to mono; providers `["CUDAExecutionProvider","CPUExecutionProvider"]` for cuda (call `onnxruntime.preload_dlls()` first), `["CPUExecutionProvider"]` for cpu. `.active_provider -> str`.

- [ ] **Step 1: Write failing tests:** `test_stereo_48k_is_converted_to_mono_16k` (unit, monkeypatch the recognizer to capture the waveform; asserts shape and rate); `@pytest.mark.model test_transcribes_tts_sample` using a short WAV generated once with Windows SAPI (`System.Speech`) saying "open discord" → text contains "discord" (case-insensitive).
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run unit test → PASS; run `uv run pytest -m model tests/test_parakeet.py` → PASS and `active_provider == "CUDAExecutionProvider"`.
- [ ] **Step 5:** Commit `feat(core): Parakeet-TDT v3 STT adapter`.

### Task 6: Voice sample recorder, WER metric & STT bench

**Files:**
- Create: `bench/metrics.py`, `bench/record_samples.py`, `bench/bench_stt.py`, `core/tests/test_metrics.py`

**Interfaces:**
- Produces: `metrics.normalize(text: str) -> str` (NFD, drop combining marks, casefold, map final `ς`→`σ`, strip punctuation, collapse spaces); `metrics.wer(ref: str, hyp: str) -> float` (jiwer on normalized strings); `record_samples.is_silent(audio: np.ndarray, threshold_dbfs: float = -50.0) -> bool`. Recordings: `bench/audio/<fixture id>.wav` (16 kHz mono) + `bench/audio/refs.jsonl`.

- [ ] **Step 1: Write failing tests:** `normalize("Άνοιξε το Spotify!") == "ανοιξε το spotify"`; `normalize("Πρόσεχε, ΣΟΥΔΑΣ") == "προσεχε σουδασ"`; `wer("άνοιξε το spotify", "Ανοιξε το Spotify") == 0.0`; `is_silent(np.zeros(16000)) is True`; `is_silent(0.1*np.sin(np.linspace(0, 2000, 16000))) is False`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [ ] **Step 5:** `record_samples.py`: lists input devices and asks which to use; picks 40 fixtures (20 `en`, 20 `el`/`mixed`, stratified by intent, fixed seed 7); for each shows the text, records on Enter until Enter (max 6 s), rejects silent clips and re-prompts, allows `r` to redo. **Human step: the user records the 40 clips** (≈5 min).
- [ ] **Step 6:** `bench_stt.py`: for `device in ["cuda","cpu"]`, 3 warm-ups, transcribe every clip → `bench/results/stt.json` with WER per lang, latency stats per device and clip-length bucket (≤2 s, 2–4 s, >4 s), and worst 10 transcripts.
- [ ] **Step 7:** If Greek WER > 15 %: add `faster-whisper` to deps and a `--engine whisper` option (`large-v3-turbo`, `compute_type="int8_float16"`, `language=None`) to `bench_stt.py`; record both engines side by side. Otherwise skip.
- [ ] **Step 8:** Commit `feat(bench): voice recorder, WER metric, STT bench`.

### Task 7: Concurrent VRAM check & go/no-go report

**Files:**
- Create: `bench/bench_vram.py`, `bench/report.py`, `core/tests/test_report.py`, `bench/RESULTS.md` (generated, committed)

**Interfaces:**
- Consumes: `LlamaServer`, `ParakeetStt`, all `bench/results/*.json`.
- Produces: `report.evaluate(results: dict) -> list[Check]` with `Check(name: str, value: float, threshold: str, passed: bool)` using exactly the Global Constraints thresholds; `report.render(checks: list[Check], meta: dict) -> str` (markdown table + hardware/build info + "Decisions" section).

- [ ] **Step 1: Write failing tests:** `test_evaluate_flags_llm_p50_over_budget` (p50 = 820 → that check `passed is False`); `test_evaluate_all_pass_on_good_results`; `test_render_contains_every_check_name`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [ ] **Step 5:** `bench_vram.py`: start llama-server, load Parakeet on CUDA, run one tool call + one transcription, then read `nvidia-smi --query-compute-apps=pid,used_memory --format=csv` and sum sudo's PIDs → `bench/results/vram.json`.
- [ ] **Step 6:** Run `uv run python ../bench/report.py` → writes `bench/RESULTS.md`. In its "Decisions" section record: chosen quant/flags, decider acceptance threshold, STT engine, and for each failed check the fallback to take (e.g. LLM over budget → try `Q4_K_S`/`IQ4_XS` or reduce tools per call; decider < 80 % Greek → add Greek label descriptions or move those intents to T0 grammar; STT GR fails → whisper turbo).
- [ ] **Step 7:** Commit `feat(bench): VRAM check, go/no-go report` and show the user `bench/RESULTS.md`.

### Task 8: LLM latency tuning (added after Task 3 measured p95 1435 ms)

Runs right after Task 3. Goal: tool-call total **p95 ≤ 1000 ms** while keeping correct tool ≥ 90 % and valid ≥ 95 %, within the VRAM budget (llama-server ≤ 3.6 GB on this GPU). Task 3 found latency ≈ 200 ms prefill + ~15 ms per output token; the tail is slot-heavy calls (40–75 tokens) and runaway `chat` replies.

**Files:**
- Modify: `core/sudo_core/llm/server.py`, `core/sudo_core/llm/client.py`, `core/sudo_core/llm/tools.py`, `bench/bench_llm.py`, `models.toml`, `core/tests/test_llm_server.py`, `core/tests/test_llm_client.py`
- Create: `bench/results/llm_tuning.json` (gitignored, summarized into the report)

**Interfaces:**
- Consumes: Task 3's `LlamaServer`, `LlmClient`, `intents_to_tools`, `bench_llm.py`.
- Produces: `LlamaServer(..., draft_model: Path | None = None, draft_max: int = 16, draft_min: int = 1, spec_ngram: bool = False)` adding the speculative-decoding flags the pinned build supports (verify names with `llama-server.exe --help`; e.g. `-md`, `-ngld 99`, `--draft-max`, `--draft-min`); before `Popen`, raise `LlamaServerError` if 127.0.0.1:port already accepts connections. `ToolCallResult` gains `name_ms: float | None` (time until the function name is complete in the stream — the island can switch to "acting" then) and `finish_reason: str | None`; a call with `finish_reason == "length"` is `valid=False`. `intents_to_tools`: `chat` tool has **no parameters** (the reply is generated by a separate call later, never inside the tool call); `SYSTEM_PROMPT` gains one line: "Use chat for questions you can answer yourself or small talk; use web_search only when the user asks to search or look something up."

- [ ] **Step 1: Write failing tests:** `_argv()` contains the draft flags and `-ngld 99` only when `draft_model` is set; `start()` raises `LlamaServerError` mentioning the port when a socket is already listening on it; streamed name delta then args → `name_ms` ≤ `total_ms` and set; `finish_reason: "length"` → `valid is False`; `chat` tool schema has empty `properties`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [ ] **Step 5:** Add `llm-draft` to `models.toml` (`unsloth/Qwen3.5-0.8B-GGUF`, the `Q4_K_M` file — check the exact filename via the HF API) and fetch it.
- [ ] **Step 6:** `bench_llm.py --config <name>` + per-call error handling (an `httpx.HTTPError` becomes an invalid sample; results flushed after each call) + records `max_tokens`, `finish_reason` counts, `name_ms` stats, and correctness by tool name alone (validity reported separately). Run these configs over all fixtures: `baseline` (new chat schema + prompt line, no speculation), `draft` (0.8B draft model), `ngram` (n-gram/prompt-lookup speculation, only if the build supports it), `draft+ngram` if both exist. For each record p50/p95 total, p50/p95 `name_ms`, valid, correct (overall + per lang), and llama-server VRAM from `nvidia-smi --query-compute-apps`.
- [ ] **Step 7:** Make the best config that meets the goal (or the fastest within the VRAM budget if none does) the `LlamaServer` default; write a comparison table + the decision into the report file.
- [ ] **Step 8:** Commit `perf(core): speculative decoding + compact chat tool; LLM tuning bench`.

### Task 9: Fast intent tier (T1) bake-off (added after Task 4 measured GLiNER at p95 351–611 ms on CPU)

Runs right after Task 4. Goal: a T1 intent classifier with **p95 ≤ 80 ms** that, at its ≥ 97 %-precision threshold, accepts **≥ 70 %** of utterances (overall, and ≥ 60 % of `el`). Same `Decision` contract as Task 4 so Phase 3 can use either.

**Files:**
- Create: `core/sudo_core/brain/embed_router.py`, `core/tests/test_embed_router.py`, `bench/bench_t1.py`
- Modify: `core/pyproject.toml`, `models.toml`, `.gitignore` (nothing new expected)

**Interfaces:**
- Consumes: `Decision(intent, score, latency_ms)` and `Decider` from Task 4; `perf.summarize`; fixtures.
- Produces: `EmbedRouter(model_dir: Path, exemplars: dict[str, list[str]], device: Literal["cpu","cuda"] = "cpu", encoder=None)` with `.decide(text: str) -> Decision` — embeds the utterance (normalized, query prefix if the model needs one), cosine-scores against per-intent exemplar embeddings precomputed at construction, intent = best intent by max exemplar similarity, `score` = that similarity; `.add_exemplar(intent: str, text: str) -> None` (Phase 3 will learn from corrections). `encoder` injection for unit tests (`encode(list[str]) -> np.ndarray`).

- [ ] **Step 1: Write failing tests** with a fake encoder (deterministic vectors): nearest exemplar's intent wins; `score` is the cosine similarity in [−1, 1]; empty exemplar set for an intent is ignored; `add_exemplar` changes a later decision; exemplar embeddings are computed once at construction (count encoder calls).
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [ ] **Step 5:** Add to `models.toml` and fetch: `intfloat/multilingual-e5-small` and `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2` (hf_repo). Prefer ONNX/`onnxruntime` CPU or `sentence-transformers` — whichever gives lower latency; record which.
- [ ] **Step 6:** `bench/bench_t1.py` evaluates with **leave-one-out** (each utterance is classified with itself removed from the exemplars; exemplars = all other fixture texts of each intent), for each candidate: (a) e5-small CPU, (b) MiniLM-L12 CPU, (c) GLiNER2.5-multi-Decide on **CUDA** (install a CUDA build of torch into the venv only for this probe if feasible; record extra VRAM via `nvidia-smi` total-used delta and disk size; skip with a note if it can't be installed cleanly), (d) Task 4's best CPU GLiNER result for reference. Report per candidate: top-1 overall/en/el/mixed, latency p50/p95 (after 10 warm-ups, single-threaded call path as Phase 3 will use), threshold sweep 0.30…0.95 → lowest threshold with ≥ 97 % precision and accepted share overall/el, and VRAM/RAM.
- [ ] **Step 7:** Write `bench/results/t1.json` and a comparison table + recommendation in the report file: the winner is the fastest candidate meeting the goal; if none meets it, the one with the highest accepted share at ≥ 97 % precision with p95 ≤ 80 ms; if none is ≤ 80 ms, say so and recommend dropping T1 in favor of T0 grammar + LLM (Task 8 numbers).
- [ ] **Step 8:** Commit `feat(core): embedding intent router; T1 bake-off bench`.
