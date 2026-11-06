"""Create the throwaway CUDA-torch venv for the GLiNER-on-CUDA T1 probe (does not touch the project venv).

Run from core/:  uv run python ../bench/setup_gliner_cuda.py [--torch-index URL] [--dry-run]

The venv goes to $SUDO_DATA/venv-gliner-cuda (default <repo>/.data/venv-gliner-cuda). torch comes from the
PyTorch CUDA 12.8 wheel index. UNVERIFIED, to verify on the laptop: that this recipe completes there and that the
torch build lists sm_120 (Blackwell, the RTX 5050 laptop) in torch.cuda.get_arch_list(); it has never been run.
The dev box is Turing (sm_75). The project venv's PyPI torch is CPU-only on Windows. Downloads ~3 GB.
Afterwards bench_t1.py finds the venv by itself:  uv run python ../bench/bench_t1.py --candidates gliner-cuda
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from collections.abc import Mapping
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from benchenv import gliner_venv_dir, venv_python  # noqa: E402

TORCH_INDEX = "https://download.pytorch.org/whl/cu128"
PACKAGES = ["gliner2==2.1.0", "psutil", "numpy"]  # gliner2 matches core/uv.lock
REQUIRED_ARCH = ["sm_120"]
VERIFY = ("import torch; print('torch', torch.__version__, 'cuda', torch.version.cuda, "
          "'available', torch.cuda.is_available()); print(' '.join(torch.cuda.get_arch_list()))")
AVAILABLE_RE = "available True"


def venv_dir(env: Mapping[str, str] | None = None) -> Path:
    return gliner_venv_dir(env)


def commands(venv: Path, torch_index: str, uv: str = "uv") -> list[list[str]]:
    py = str(venv_python(venv))
    return [
        [uv, "venv", str(venv), "--python", "3.12", "--allow-existing"],
        [uv, "pip", "install", "--python", py, "torch", "--index-url", torch_index, "--reinstall-package", "torch"],
        [uv, "pip", "install", "--python", py, *PACKAGES],
    ]


def missing_arch(arch_list: list[str]) -> list[str]:
    return [a for a in REQUIRED_ARCH if a not in arch_list]


def main(argv: list[str] | None = None) -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--torch-index", default=TORCH_INDEX, help="PyTorch wheel index (default: CUDA 12.8)")
    ap.add_argument("--dry-run", action="store_true", help="print the commands only")
    args = ap.parse_args(argv)

    venv = venv_dir()
    cmds = commands(venv, args.torch_index, uv=os.environ.get("UV", "uv"))
    for cmd in cmds:
        print("+", " ".join(cmd), flush=True)
        if not args.dry_run:
            subprocess.run(cmd, check=True)
    if args.dry_run:
        return
    out = subprocess.run([str(venv_python(venv)), "-c", VERIFY], check=True, capture_output=True, text=True).stdout
    print(out, end="")
    lines = out.strip().splitlines()
    if AVAILABLE_RE not in lines[0]:
        sys.exit("torch.cuda.is_available() is False: CUDA is not usable from this venv (check the NVIDIA driver with "
                 "nvidia-smi and that the installed torch is a CUDA build); this is not an architecture problem")
    missing = missing_arch(lines[-1].split())
    if missing:
        sys.exit(f"CUDA is available but {missing} is not in torch.cuda.get_arch_list() (needed for Blackwell); "
                 "try a newer --torch-index, e.g. https://download.pytorch.org/whl/cu130")
    print(f"ready: {venv_python(venv)}")


if __name__ == "__main__":
    main()
