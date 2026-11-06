"""Unit tests for bench/benchenv.py (SUDO_DATA location, CUDA cache default, ONNX provider check)."""

import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BENCH = REPO / "bench"
sys.path.insert(0, str(BENCH))

from benchenv import (  # noqa: E402
    configure_cuda_cache,
    gliner_venv_dir,
    provider_problem,
    sudo_data,
    venv_python,
)


def test_sudo_data_from_env(tmp_path: Path):
    assert sudo_data({"SUDO_DATA": str(tmp_path)}) == tmp_path


def test_sudo_data_defaults_to_repo_dot_data():
    assert sudo_data({}) == REPO / ".data"
    assert sudo_data({"SUDO_DATA": ""}) == REPO / ".data"


def test_configure_cuda_cache_defaults_under_sudo_data(tmp_path: Path):
    env = {"SUDO_DATA": str(tmp_path)}
    configure_cuda_cache(env)
    assert env["CUDA_CACHE_PATH"] == str(tmp_path / "cuda-cache")
    assert int(env["CUDA_CACHE_MAXSIZE"]) == 4 * 1024**3


def test_configure_cuda_cache_keeps_existing_value(tmp_path: Path):
    env = {"SUDO_DATA": str(tmp_path), "CUDA_CACHE_PATH": "elsewhere"}
    configure_cuda_cache(env)
    assert env["CUDA_CACHE_PATH"] == "elsewhere"


def test_gliner_venv_lives_under_sudo_data(tmp_path: Path):
    venv = gliner_venv_dir({"SUDO_DATA": str(tmp_path)})
    assert venv == tmp_path / "venv-gliner-cuda"
    assert venv_python(venv).name.startswith("python")
    assert venv in venv_python(venv).parents


def test_provider_problem_flags_cpu_fallback_on_cuda():
    assert provider_problem("cuda", "CUDAExecutionProvider") is None
    msg = provider_problem("cuda", "CPUExecutionProvider")
    assert msg is not None and "ran on CPU" in msg and "CPUExecutionProvider" in msg
    assert provider_problem("cpu", "CPUExecutionProvider") is None


def _tracked_python_files() -> list[Path]:
    """Tracked source only (never walks core/.venv); falls back to the source dirs if git is unavailable."""
    try:
        out = subprocess.run(["git", "ls-files", "--cached", "--others", "--exclude-standard", "*.py"], cwd=REPO, check=True, capture_output=True, text=True).stdout
        files = [REPO / line for line in out.splitlines() if line.strip()]
        if files:
            return files
    except (OSError, subprocess.CalledProcessError):
        pass
    return [p for d in (BENCH, REPO / "core" / "sudo_core", REPO / "core" / "tests") for p in d.rglob("*.py")]


def test_no_python_file_hardcodes_the_dev_box_data_drive():
    offenders = []
    for p in _tracked_python_files():
        if ".venv" in p.parts or p == Path(__file__).resolve():
            continue
        text = p.read_text(encoding="utf-8")
        if "sudo-data" in text.lower() or "D:/" in text or "D:\\" in text:
            offenders.append(p.relative_to(REPO).as_posix())
    assert offenders == []
