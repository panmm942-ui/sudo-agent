"""Environment helpers shared by the benches.

- `sudo_data()`: the data/cache root. `$SUDO_DATA` if set (dev.sh / dev.ps1 set it), else `<repo>/.data`.
- `configure_cuda_cache()`: persist the CUDA JIT cache under the data root unless CUDA_CACHE_PATH is set.
- `gliner_venv_dir()` / `venv_python()`: the throwaway CUDA-torch venv for the GLiNER T1 probe
  (created by bench/setup_gliner_cuda.py).
- `provider_problem()`: detects onnxruntime's silent CPU fallback when CUDA was requested.
"""

from __future__ import annotations

import os
from collections.abc import MutableMapping
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CUDA_PROVIDER = "CUDAExecutionProvider"


def sudo_data(env: MutableMapping[str, str] | None = None) -> Path:
    env = os.environ if env is None else env
    value = env.get("SUDO_DATA", "")
    return Path(value) if value else REPO / ".data"


def configure_cuda_cache(env: MutableMapping[str, str] | None = None) -> None:
    """llama.cpp / torch CUDA builds JIT-compile kernels on first use (~80 s cold on a GPU without native
    kernels). Persist that cache under the data root so only the first run pays for it."""
    env = os.environ if env is None else env
    env.setdefault("CUDA_CACHE_PATH", str(sudo_data(env) / "cuda-cache"))
    env.setdefault("CUDA_CACHE_MAXSIZE", str(4 * 1024**3))


def gliner_venv_dir(env: MutableMapping[str, str] | None = None) -> Path:
    return sudo_data(env) / "venv-gliner-cuda"


def venv_python(venv: Path) -> Path:
    return venv / "Scripts" / "python.exe" if os.name == "nt" else venv / "bin" / "python"


def provider_problem(device: str, provider: str) -> str | None:
    """None when the session runs where it was asked to; otherwise a one-line reason the run is invalid."""
    if device == "cuda" and provider != CUDA_PROVIDER:
        return f"ran on CPU: onnxruntime loaded {provider}, not {CUDA_PROVIDER} (CUDA provider failed to load)"
    return None
