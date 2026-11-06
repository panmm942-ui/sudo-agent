"""Sudo AI fast-command router: GLiNER2.5-multi-Decide on the CPU.

Reads one JSON object per line on stdin ({"id": 1, "text": "..."}) and answers on stdout with
{"id": 1, "intent": "screenshot", "confidence": 0.97, "ms": 160}. GLiNER only decides what kind
of command it is; Sudo (Rust) works out the details (which app, what to type) and checks them,
and anything unclear goes to the language model. Everything stays on this PC: the model loads
from a local folder with networking disabled.
"""
import contextlib
import json
import os
import sys
import time

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

# Below this, Sudo hands the request to the language model instead.
CONFIDENT = 0.6

# Short, plain wording: every label is encoded with each request, so long descriptions made a
# command three times slower (about 500 ms instead of 160 ms on a 6-core CPU) and no more
# accurate. This wording was the most accurate of the sets tried on Greek and English commands.
LABELS = {
    "take a screenshot": "screenshot",
    "open an app": "open_app",
    "type text": "type_text",
    "pause or play music": "play_pause",
    "next song": "next_track",
    "previous song": "previous_track",
    "volume up": "volume_up",
    "volume down": "volume_down",
    "mute sound": "mute",
    "question or conversation": "chat",
}


@contextlib.contextmanager
def skip_random_init():
    """Building the network fills every weight with random numbers that the checkpoint then
    overwrites; that alone took 20 of the 25 seconds of loading. Skip it while loading."""
    import torch
    keep = lambda tensor, *args, **kwargs: tensor  # noqa: E731
    patched = []
    for owner, name in [(torch.nn.init, n) for n in ("normal_", "uniform_", "trunc_normal_", "xavier_uniform_",
                                                     "xavier_normal_", "kaiming_uniform_", "kaiming_normal_", "zeros_", "ones_")] + \
                       [(torch.Tensor, "normal_"), (torch.Tensor, "uniform_")]:
        if hasattr(owner, name):
            patched.append((owner, name, getattr(owner, name)))
            setattr(owner, name, keep)
    try:
        import transformers.initialization as hf_init
        for name in ("normal_", "uniform_", "trunc_normal_", "zeros_", "ones_"):
            if hasattr(hf_init, name):
                patched.append((hf_init, name, getattr(hf_init, name)))
                setattr(hf_init, name, keep)
    except ImportError:
        pass
    try:
        yield
    finally:
        for owner, name, original in reversed(patched):
            setattr(owner, name, original)


class Router:
    def __init__(self, model_dir, fast_load=True):
        import torch
        from gliner2 import AutoExtractor
        torch.set_num_threads(max(1, min(6, os.cpu_count() or 1)))
        with skip_random_init() if fast_load else contextlib.nullcontext():
            self.model = AutoExtractor.from_pretrained(model_dir)
        self.model.eval()
        # Memory: the 250,000-word vocabulary table is two thirds of the model (770 MB as float32)
        # but is only looked up, never multiplied, so it is kept as bfloat16 and each lookup is
        # widened back to float32. Same answers, about 0.4 GB less; then drop what loading left behind.
        for module in self.model.modules():
            if isinstance(module, torch.nn.Embedding) and module.weight.shape[0] > 100_000:
                module.weight.data = module.weight.data.to(torch.bfloat16)
                module.register_forward_hook(lambda _module, _inputs, output: output.float())
        import gc
        gc.collect()
        self.schema = self.model.create_schema().classification("intent", list(LABELS))

    def route(self, text):
        import torch
        with torch.inference_mode():
            raw = self.model.extract(text, self.schema, include_confidence=True)
        value = raw.get("intent")
        label, confidence = None, 0.0
        if isinstance(value, dict):
            label, confidence = value.get("label"), float(value.get("confidence", 0.0))
        elif isinstance(value, list) and value and isinstance(value[0], dict):
            label, confidence = value[0].get("label"), float(value[0].get("confidence", 0.0))
        return {"intent": LABELS.get(label, "chat"), "confidence": round(confidence, 3)}


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stdin.reconfigure(encoding="utf-8")
    model_dir = sys.argv[1]
    started = time.time()
    try:
        router = Router(model_dir)
        router.route("open spotify")  # warm up
    except Exception as error:  # noqa: BLE001 - reported to Sudo, which falls back to the model
        print(json.dumps({"event": "error", "message": str(error)[:300]}), flush=True)
        return
    print(json.dumps({"event": "ready", "loadMs": int((time.time() - started) * 1000)}), flush=True)
    for line in sys.stdin:
        try:
            request = json.loads(line)
            text = str(request.get("text", ""))[:500]
            t = time.perf_counter()
            result = router.route(text) if text.strip() else {"intent": "chat", "confidence": 0.0}
            result["id"] = request.get("id")
            result["ms"] = int((time.perf_counter() - t) * 1000)
            print(json.dumps(result, ensure_ascii=False), flush=True)
        except Exception as error:  # noqa: BLE001
            print(json.dumps({"id": None, "intent": "chat", "confidence": 0.0, "error": str(error)[:300]}), flush=True)


if __name__ == "__main__":
    main()
