"""T1 candidate: embedding-similarity intent router (kNN over per-intent exemplar utterances).

`EmbedRouter` embeds the utterance, cosine-scores it against precomputed exemplar embeddings and returns the
intent whose best exemplar is closest; `Decision.score` is that cosine similarity, so abstention is a threshold
on the score, exactly like `Decider`. Exemplars are embedded once at construction (one batched call); the
only per-decision encoder call is the query. `add_exemplar` lets Phase 3 learn from corrections.

The encoder is anything with `encode(list[str]) -> np.ndarray` (n, dim). The real ones (`OnnxEncoder`,
`TorchEncoder`) do mean pooling + L2 normalisation, which is what both `intfloat/multilingual-e5-small` and
`paraphrase-multilingual-MiniLM-L12-v2` are trained for. e5 needs `query_prefix="query: "` on queries and
exemplars alike (symmetric task); MiniLM needs none.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Literal, Protocol

import numpy as np

from sudo_core.brain.decider import FALLBACK_INTENT, Decision
from sudo_core.perf import Stopwatch


class Encoder(Protocol):
    def encode(self, texts: list[str]) -> np.ndarray: ...


def _normalize(vecs: np.ndarray) -> np.ndarray:
    vecs = np.asarray(vecs, dtype=np.float32)
    if vecs.ndim == 1:
        vecs = vecs[None, :]
    norms = np.linalg.norm(vecs, axis=1, keepdims=True)
    return vecs / np.maximum(norms, 1e-12)


class EmbedRouter:
    def __init__(
        self,
        model_dir: Path | str,
        exemplars: dict[str, list[str]],
        device: Literal["cpu", "cuda"] = "cpu",
        encoder: Encoder | None = None,
        query_prefix: str = "",
        num_threads: int | None = None,
    ) -> None:
        """`model_dir` is only used to build the default encoder (ONNX if the dir has onnx/model.onnx, else torch)."""
        if encoder is None:
            encoder = load_encoder(Path(model_dir), device=device, num_threads=num_threads)
        self._encoder = encoder
        self._prefix = query_prefix
        self._texts: list[str] = []
        self._intents: list[str] = []
        flat = [(intent, text) for intent, texts in exemplars.items() for text in texts]
        if flat:
            self._intents = [i for i, _ in flat]
            self._texts = [t for _, t in flat]
            self._matrix = self._embed(self._texts)
        else:
            self._matrix = np.zeros((0, 0), dtype=np.float32)

    def _embed(self, texts: list[str]) -> np.ndarray:
        return _normalize(self._encoder.encode([self._prefix + t for t in texts]))

    def add_exemplar(self, intent: str, text: str) -> None:
        vec = self._embed([text])
        self._matrix = vec if len(self._intents) == 0 else np.vstack([self._matrix, vec])
        self._intents.append(intent)
        self._texts.append(text)

    def decide(self, text: str, *, exclude_text: str | None = None) -> Decision:
        """`exclude_text` ignores exemplars whose text equals it (leave-one-out evaluation); no state changes."""
        with Stopwatch() as sw:
            if not self._intents:
                return Decision(FALLBACK_INTENT, 0.0, sw.ms)
            sims = self._matrix @ self._embed([text])[0]
            if exclude_text is not None:
                keep = np.fromiter((t != exclude_text for t in self._texts), dtype=bool, count=len(self._texts))
                sims = np.where(keep, sims, -np.inf)
            best = int(np.argmax(sims))
            best_sim = float(sims[best])
        if not np.isfinite(best_sim):
            return Decision(FALLBACK_INTENT, 0.0, sw.ms)
        return Decision(self._intents[best], max(-1.0, min(1.0, best_sim)), sw.ms)


def _mean_pool(hidden: np.ndarray, mask: np.ndarray) -> np.ndarray:
    m = mask[..., None].astype(np.float32)
    return (hidden * m).sum(axis=1) / np.maximum(m.sum(axis=1), 1e-9)


class OnnxEncoder:
    """onnxruntime (CPU) + `tokenizers`; mean pooling, L2-normalised."""

    def __init__(self, model_dir: Path, onnx_file: str = "onnx/model.onnx", num_threads: int | None = None,
                 max_length: int = 64) -> None:
        import onnxruntime as ort
        from tokenizers import Tokenizer

        self._tok = Tokenizer.from_file(str(model_dir / "tokenizer.json"))
        self._tok.enable_truncation(max_length=max_length)
        self._tok.enable_padding()
        opts = ort.SessionOptions()
        if num_threads is not None:
            opts.intra_op_num_threads = num_threads
        self._sess = ort.InferenceSession(str(model_dir / onnx_file), opts, providers=["CPUExecutionProvider"])
        self._inputs = {i.name for i in self._sess.get_inputs()}

    def encode(self, texts: list[str]) -> np.ndarray:
        enc = self._tok.encode_batch(texts)
        ids = np.array([e.ids for e in enc], dtype=np.int64)
        mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
        feed: dict[str, Any] = {"input_ids": ids, "attention_mask": mask}
        if "token_type_ids" in self._inputs:
            feed["token_type_ids"] = np.zeros_like(ids)
        hidden = self._sess.run(None, feed)[0]
        return _normalize(_mean_pool(hidden, mask))


class TorchEncoder:
    """transformers AutoModel (CPU or CUDA); mean pooling, L2-normalised."""

    def __init__(self, model_dir: Path, device: str = "cpu", num_threads: int | None = None,
                 max_length: int = 64) -> None:
        import torch
        from transformers import AutoModel, AutoTokenizer

        if num_threads is not None:
            torch.set_num_threads(num_threads)
        self._torch = torch
        self._device = device
        self._max_length = max_length
        self._tok = AutoTokenizer.from_pretrained(str(model_dir))
        self._model = AutoModel.from_pretrained(str(model_dir)).to(device).eval()

    def encode(self, texts: list[str]) -> np.ndarray:
        torch = self._torch
        batch = self._tok(texts, padding=True, truncation=True, max_length=self._max_length, return_tensors="pt")
        batch = {k: v.to(self._device) for k, v in batch.items()}
        with torch.inference_mode():
            hidden = self._model(**batch).last_hidden_state
        return _normalize(_mean_pool(hidden.float().cpu().numpy(), batch["attention_mask"].cpu().numpy()))


def load_encoder(model_dir: Path, device: str = "cpu", num_threads: int | None = None,
                 backend: Literal["auto", "onnx", "torch"] = "auto", onnx_file: str = "onnx/model.onnx") -> Encoder:
    use_onnx = backend == "onnx" or (backend == "auto" and device == "cpu" and (model_dir / onnx_file).exists())
    if use_onnx:
        return OnnxEncoder(model_dir, onnx_file=onnx_file, num_threads=num_threads)
    return TorchEncoder(model_dir, device=device, num_threads=num_threads)
