"""T1 decision engine: classify a spoken command into a registered intent (GLiNER2.5-multi-Decide).

Pinned API (verified against the installed `gliner2` source, `inference/runtime.py`, and by running
the local checkpoint; the model card's SKILL.md only documents the hosted Fastino API and is not relevant):

    extractor = AutoExtractor.from_pretrained(<local dir or hub id>)     # -> BoundaryExtractor
    extractor.classify_text(
        text,
        {"intent": {"labels": {intent_id: description, ...}}},           # dict labels = id -> description
        include_confidence=True,                                         # kwarg; default False
    )
    # -> {"intent": {"label": "<intent_id>", "confidence": <float 0..1>}}
    #    (without include_confidence the value is just the label string)

For a single-label head the model always returns its arg-max label; `threshold` (default 0.5) does NOT
suppress it, so abstention is the caller's job: compare `Decision.score` with a tuned threshold.
`confidence` is a sigmoid-style score, not a softmax over the labels, so it does not sum to 1.
Anything missing or not in `labels` degrades to `Decision("chat", 0.0, ...)`.

`label_mode`:
  "dict"         (default, the brief's form) sends {id: description}; the returned label is an intent id.
  "descriptions" sends the descriptions as a flat label list (`{"intent": [desc, ...]}`) and maps the returned
                 description back to its id. Measured on the fixtures this is markedly more accurate and ~2x
                 faster than "dict" (see bench/results/decider.json), so the bench reports both.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from sudo_core.perf import Stopwatch

FALLBACK_INTENT = "chat"
HEAD = "intent"


@dataclass(frozen=True)
class Decision:
    intent: str
    score: float
    latency_ms: float


class Decider:
    def __init__(
        self,
        model_id: str = "fastino/GLiNER2.5-multi-Decide",
        device: str = "cpu",
        extractor: Any = None,
        num_threads: int | None = None,
        label_mode: str = "dict",
    ) -> None:
        """`model_id` may be a hub id or a local directory (e.g. <repo>/models/decider)."""
        if extractor is None:
            import torch
            from gliner2 import AutoExtractor

            if num_threads is not None:
                torch.set_num_threads(num_threads)
            extractor = AutoExtractor.from_pretrained(model_id)
            extractor.to(device)
            extractor.eval()
        if label_mode not in ("dict", "descriptions"):
            raise ValueError(f"label_mode must be 'dict' or 'descriptions', got {label_mode!r}")
        self._extractor = extractor
        self._label_mode = label_mode

    def decide(self, text: str, labels: dict[str, str]) -> Decision:
        by_desc = {desc: intent for intent, desc in labels.items()}
        task = {HEAD: list(by_desc)} if self._label_mode == "descriptions" else {HEAD: {"labels": labels}}
        with Stopwatch() as sw:
            result = self._extractor.classify_text(text, task, include_confidence=True)
        intent, score = _parse(result, labels, by_desc if self._label_mode == "descriptions" else None)
        return Decision(intent, score, sw.ms)


def _parse(result: Any, labels: dict[str, str], by_desc: dict[str, str] | None) -> tuple[str, float]:
    entry = result.get(HEAD) if isinstance(result, dict) else None
    if isinstance(entry, dict):
        label, conf = entry.get("label"), entry.get("confidence", 0.0)
    elif isinstance(entry, str):
        label, conf = entry, 0.0
    else:
        return FALLBACK_INTENT, 0.0
    if by_desc is not None:
        label = by_desc.get(label)
    if label not in labels:
        return FALLBACK_INTENT, 0.0
    return label, float(conf)
