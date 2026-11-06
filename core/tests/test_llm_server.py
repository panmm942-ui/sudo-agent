import socket
import subprocess
import sys
from pathlib import Path

import pytest

from sudo_core.llm.server import LlamaServer, LlamaServerError


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class _Crashing(LlamaServer):
    def _argv(self) -> list[str]:
        return [
            str(self.exe),
            "-c",
            "import sys; print('CUDA error: out of memory', file=sys.stderr); sys.exit(1)",
        ]


def test_server_start_raises_with_stderr_when_process_exits():
    srv = _Crashing(exe=Path(sys.executable), model=Path("m.gguf"), mmproj=None, port=_free_port())
    with pytest.raises(LlamaServerError) as ei:
        srv.start(timeout_s=10)
    assert "CUDA error" in ei.value.stderr_tail


def test_argv_has_required_flags():
    srv = LlamaServer(exe=Path("llama-server.exe"), model=Path("m.gguf"), mmproj=Path("mm.gguf"))
    argv = srv._argv()
    assert argv[argv.index("--port") + 1] == "8765"
    assert argv[argv.index("-c") + 1] == "8192"
    assert "--jinja" in argv
    assert argv[argv.index("-fa") + 1] == "on"
    assert argv[argv.index("--chat-template-kwargs") + 1] == '{"enable_thinking":false}'
    assert argv[argv.index("--mmproj") + 1] == "mm.gguf"


class _Sleeper(LlamaServer):
    def _argv(self) -> list[str]:
        return [str(self.exe), "-c", "import time; time.sleep(60)"]


def test_start_stops_child_when_polling_raises(monkeypatch):
    import httpx

    def boom(self, *a, **k):
        raise KeyboardInterrupt

    monkeypatch.setattr(httpx.Client, "get", boom)
    procs = []
    real_popen = subprocess.Popen

    def spy(*a, **k):
        procs.append(real_popen(*a, **k))
        return procs[-1]

    monkeypatch.setattr(subprocess, "Popen", spy)
    srv = _Sleeper(exe=Path(sys.executable), model=Path("m.gguf"), mmproj=None, port=_free_port())
    with pytest.raises(KeyboardInterrupt):
        srv.start(timeout_s=30)
    assert srv._proc is None
    assert len(procs) == 1 and procs[0].poll() is not None  # child was terminated


def test_flags_description_records_ctx_and_mmproj():
    plain = LlamaServer(exe=Path("llama-server.exe"), model=Path("m.gguf")).flags_description()
    assert "-c 8192" in plain and "--mmproj" not in plain
    vision = LlamaServer(exe=Path("llama-server.exe"), model=Path("m.gguf"), mmproj=Path("x/mm.gguf"))
    assert "--mmproj mm.gguf" in vision.flags_description()


def test_argv_has_no_draft_flags_by_default():
    argv = LlamaServer(exe=Path("llama-server.exe"), model=Path("m.gguf"))._argv()
    assert "-md" not in argv
    assert "-ngld" not in argv
    assert "--spec-type" not in argv


def test_argv_draft_flags_only_with_draft_model():
    srv = LlamaServer(
        exe=Path("llama-server.exe"), model=Path("m.gguf"), draft_model=Path("d.gguf"), draft_max=8, draft_min=2
    )
    argv = srv._argv()
    assert argv[argv.index("-md") + 1] == "d.gguf"
    assert argv[argv.index("-ngld") + 1] == "99"
    assert argv[argv.index("--spec-draft-n-max") + 1] == "8"
    assert argv[argv.index("--spec-draft-n-min") + 1] == "2"
    assert "draft-simple" in argv[argv.index("--spec-type") + 1].split(",")


def test_argv_ngram_only():
    argv = LlamaServer(exe=Path("llama-server.exe"), model=Path("m.gguf"), spec_ngram=True)._argv()
    assert "-md" not in argv
    types = argv[argv.index("--spec-type") + 1].split(",")
    assert any(t.startswith("ngram") for t in types)
    assert "draft-simple" not in types


def test_argv_draft_plus_ngram():
    argv = LlamaServer(
        exe=Path("llama-server.exe"), model=Path("m.gguf"), draft_model=Path("d.gguf"), spec_ngram=True
    )._argv()
    types = argv[argv.index("--spec-type") + 1].split(",")
    assert "draft-simple" in types and any(t.startswith("ngram") for t in types)


def test_start_fails_fast_when_port_already_listening():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        s.listen()
        port = s.getsockname()[1]
        srv = _Sleeper(exe=Path(sys.executable), model=Path("m.gguf"), mmproj=None, port=port)
        with pytest.raises(LlamaServerError, match=str(port)):
            srv.start(timeout_s=5)
        assert srv._proc is None  # never spawned


def test_is_alive_reflects_child_state():
    srv = _Sleeper(exe=Path(sys.executable), model=Path("m.gguf"), mmproj=None, port=_free_port())
    assert srv.is_alive() is False  # never started
    srv._proc = subprocess.Popen(srv._argv())
    try:
        assert srv.is_alive() is True
    finally:
        srv.stop()
    assert srv.is_alive() is False


def test_argv_ngram_type_and_args_are_configurable():
    srv = LlamaServer(
        exe=Path("llama-server.exe"), model=Path("m.gguf"), spec_ngram=True,
        ngram_type="ngram-map-k", ngram_args=["--spec-ngram-map-k-size-n", "3"],
    )
    argv = srv._argv()
    assert argv[argv.index("--spec-type") + 1] == "ngram-map-k"
    assert argv[argv.index("--spec-ngram-map-k-size-n") + 1] == "3"
