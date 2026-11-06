"""Benchmark GLiNER2.5-multi-Decide as Sudo's fast command router (dev only).

usage: python -I router_bench.py <model_dir> <eval.jsonl>
Prints per-utterance results, intent accuracy (English / Greek / Greeklish), argument accuracy,
and warm CPU latency.
"""
import json, sys, time, os
os.environ.setdefault("HF_HUB_OFFLINE", "1")
os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "src-tauri", "resources"))
import gliner_router  # noqa: E402

sys.stdout.reconfigure(encoding="utf-8")
model_dir, eval_path = sys.argv[1], sys.argv[2]
started = time.time()
router = gliner_router.Router(model_dir)
print(f"load {time.time() - started:.1f}s")
router.route("warm up")

rows = [json.loads(line) for line in open(eval_path, encoding="utf-8") if line.strip()]
stats = {}
latencies = []
for row in rows:
    t = time.perf_counter()
    result = router.route(row["text"])
    ms = (time.perf_counter() - t) * 1000
    latencies.append(ms)
    greek = any("Ͱ" <= c <= "Ͽ" for c in row["text"])
    group = "el" if greek else ("greeklish" if row["text"].split()[0] in {"pare", "anoikse", "epomeno"} else "en")
    want = row["intent"]
    got = result["intent"] if result["confidence"] >= gliner_router.CONFIDENT else "chat"
    ok = got == want
    arg_ok = True
    if ok and want == "open_app":
        arg_ok = (result.get("app") or "").lower().strip() == row["app"]
    if ok and want == "type_text":
        arg_ok = (result.get("text") or "").lower().strip(" .") == row["text_arg"]
    s = stats.setdefault(group, [0, 0, 0])
    s[0] += 1; s[1] += ok; s[2] += ok and arg_ok
    flag = "OK " if ok and arg_ok else ("ARG" if ok else "BAD")
    print(f"{flag} {ms:6.0f}ms  {want:>14} -> {result['intent']:<14} {result['confidence']:.2f}  app={result.get('app')!r} text={result.get('text')!r}  | {row['text']}")

for group, (n, intent_ok, full_ok) in stats.items():
    print(f"{group:10} intent {intent_ok}/{n}  with details {full_ok}/{n}")
latencies.sort()
print(f"latency p50 {latencies[len(latencies) // 2]:.0f} ms  p95 {latencies[int(len(latencies) * 0.95)]:.0f} ms  max {latencies[-1]:.0f} ms")
