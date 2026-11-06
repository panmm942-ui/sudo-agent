from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

from sudo_core.stt import parakeet
from sudo_core.stt.parakeet import ParakeetStt, Transcript

MODEL_DIR = Path(__file__).resolve().parents[2] / "models" / "stt"
SAMPLE = Path(__file__).parent / "data" / "open_discord.wav"  # SAPI-generated, see data/README note in report


class FakeRecognizer:
    def __init__(self):
        self.calls = []

    def recognize(self, waveform, *, sample_rate=16000, **kw):
        self.calls.append((waveform, sample_rate))
        return "open discord"


def test_stereo_48k_is_converted_to_mono_16k(monkeypatch):
    fake = FakeRecognizer()
    seen = {}

    def fake_load(name, path, **kw):
        seen.update(name=name, path=path, **kw)
        return fake

    monkeypatch.setattr(parakeet.onnx_asr, "load_model", fake_load)
    monkeypatch.setattr(parakeet.ort, "preload_dlls", lambda **kw: None)
    stt = ParakeetStt(Path("x"), device="cpu")
    assert seen["providers"] == ["CPUExecutionProvider"]

    t = np.arange(48000) / 48000
    stereo = np.stack([np.sin(2 * np.pi * 440 * t), np.sin(2 * np.pi * 440 * t)], axis=1).astype(np.float32)
    out = stt.transcribe(stereo, sample_rate=48000)

    assert isinstance(out, Transcript) and out.text == "open discord" and out.latency_ms >= 0
    wave, rate = fake.calls[0]
    assert rate == 16000
    assert wave.ndim == 1 and wave.shape == (16000,) and wave.dtype == np.float32


@pytest.mark.model
def test_transcribes_tts_sample():
    data, sr = sf.read(SAMPLE, dtype="float32")
    stt = ParakeetStt(MODEL_DIR, device="cuda")
    print("provider:", stt.active_provider)
    assert stt.active_provider == "CUDAExecutionProvider"
    out = stt.transcribe(data, sample_rate=sr)
    print("transcript:", out.text, f"{out.latency_ms:.0f} ms")
    assert "discord" in out.text.lower()
