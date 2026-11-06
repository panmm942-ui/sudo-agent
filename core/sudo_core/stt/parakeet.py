"""Parakeet-TDT 0.6B v3 (ONNX, via onnx-asr) STT adapter used by the Phase 0 STT bench and VRAM check."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Literal

import numpy as np
import onnx_asr
import onnxruntime as ort

from sudo_core.perf import Stopwatch

TARGET_RATE = 16000
MODEL_NAME = "nemo-parakeet-tdt-0.6b-v3"


@dataclass(frozen=True)
class Transcript:
    text: str
    latency_ms: float


def _to_mono_16k(audio: np.ndarray, sample_rate: int) -> np.ndarray:
    x = np.asarray(audio, dtype=np.float32)
    if x.ndim == 2:  # (samples, channels) from soundfile; accept (channels, samples) too
        x = x.mean(axis=1 if x.shape[1] <= x.shape[0] else 0)
    if x.ndim != 1:
        raise ValueError(f"expected 1-D or 2-D audio, got shape {audio.shape}")
    if sample_rate != TARGET_RATE:
        if sample_rate % TARGET_RATE == 0:  # integer decimation: box-filter average acts as a crude anti-alias
            k = sample_rate // TARGET_RATE
            n = len(x) // k * k
            x = x[:n].reshape(-1, k).mean(axis=1)
        else:
            n_out = int(round(len(x) * TARGET_RATE / sample_rate))
            x = np.interp(np.linspace(0, len(x) - 1, n_out), np.arange(len(x)), x)
    return np.ascontiguousarray(x, dtype=np.float32)


class ParakeetStt:
    def __init__(self, model_dir: Path, device: Literal["cuda", "cpu"] = "cuda") -> None:
        if device == "cuda":
            ort.preload_dlls()
            providers = ["CUDAExecutionProvider", "CPUExecutionProvider"]
        else:
            providers = ["CPUExecutionProvider"]
        self._model = onnx_asr.load_model(MODEL_NAME, Path(model_dir), providers=providers)

    @property
    def active_provider(self) -> str:
        """First provider of the encoder session (what actually got loaded, after any CUDA fallback)."""
        try:
            return self._model.asr._encoder.get_providers()[0]
        except AttributeError:  # e.g. a stubbed recognizer
            return "unknown"

    def transcribe(self, audio: np.ndarray, sample_rate: int = TARGET_RATE) -> Transcript:
        wave = _to_mono_16k(audio, sample_rate)
        with Stopwatch() as sw:
            text = self._model.recognize(wave, sample_rate=TARGET_RATE)
        return Transcript(text=str(text).strip(), latency_ms=sw.ms)
