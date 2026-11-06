"""Unit tests for the GLiNER-on-CUDA T1 probe recipe (bench/setup_gliner_cuda.py, bench/bench_t1.py hand-off)."""

import sys
from pathlib import Path

BENCH = Path(__file__).resolve().parents[2] / "bench"
sys.path.insert(0, str(BENCH))

import setup_gliner_cuda  # noqa: E402
from bench_t1 import plan_cuda_run  # noqa: E402


def test_setup_commands_build_venv_under_sudo_data_with_cu128_torch(tmp_path: Path):
    env = {"SUDO_DATA": str(tmp_path)}
    venv = setup_gliner_cuda.venv_dir(env)
    cmds = setup_gliner_cuda.commands(venv, setup_gliner_cuda.TORCH_INDEX, uv="uv")
    assert venv == tmp_path / "venv-gliner-cuda"
    assert cmds[0][:3] == ["uv", "venv", str(venv)]
    torch_cmd = next(c for c in cmds if "torch" in c)
    assert torch_cmd[torch_cmd.index("--index-url") + 1].endswith("/cu128")
    assert any(any(p.startswith("gliner2") for p in c) for c in cmds)
    assert all(str(venv) in " ".join(c) for c in cmds)


def test_setup_dry_run_prints_and_runs_nothing(tmp_path: Path, monkeypatch, capsys):
    monkeypatch.setenv("SUDO_DATA", str(tmp_path))
    monkeypatch.setattr(setup_gliner_cuda.subprocess, "run", lambda *a, **k: (_ for _ in ()).throw(AssertionError))
    setup_gliner_cuda.main(["--dry-run"])
    out = capsys.readouterr().out
    assert "venv-gliner-cuda" in out and "cu128" in out
    assert not (tmp_path / "venv-gliner-cuda").exists()


def test_arch_check_requires_blackwell_kernels():
    assert setup_gliner_cuda.missing_arch(["sm_75", "sm_86", "sm_120"]) == []
    assert setup_gliner_cuda.missing_arch(["sm_75", "sm_86", "sm_90"]) == ["sm_120"]


def test_plan_cuda_run_hands_cuda_candidates_to_the_venv():
    cands = ["e5-onnx", "gliner-cuda", "e5-torch-cuda", "gliner-cpu-ref"]
    assert plan_cuda_run(cands, cuda_torch=True) == (cands, [])
    assert plan_cuda_run(cands, cuda_torch=False) == (["e5-onnx", "gliner-cpu-ref"], ["gliner-cuda", "e5-torch-cuda"])
