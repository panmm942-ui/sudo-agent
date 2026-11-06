"""llama-server subprocess wrapper."""

from __future__ import annotations

import collections
import socket
import subprocess
import threading
import time
from pathlib import Path

import httpx


NGRAM_SPEC_TYPE = "ngram-simple"


class LlamaServerError(RuntimeError):
    def __init__(self, message: str, stderr_tail: str = "") -> None:
        super().__init__(f"{message}\n--- stderr tail ---\n{stderr_tail}" if stderr_tail else message)
        self.stderr_tail = stderr_tail


class LlamaServer:
    """Wraps one llama-server child process.

    Speculative decoding (draft_model / spec_ngram) is OFF by default: on Qwen3.5-4B (hybrid
    recurrent) with b11982 on a GTX 1660 Super the 0.8B draft was ~6x slower and over the VRAM
    budget, and n-gram speculation gave no gain (see bench/results/llm_tuning.json, Task 8).
    """

    def __init__(
        self,
        exe: Path,
        model: Path,
        mmproj: Path | None = None,
        port: int = 8765,
        ctx: int = 8192,
        flash_attn: bool = True,
        kv_type: str = "q8_0",
        draft_model: Path | None = None,
        draft_max: int = 16,
        draft_min: int = 1,
        spec_ngram: bool = False,
        ngram_type: str = NGRAM_SPEC_TYPE,
        ngram_args: list[str] | None = None,
    ) -> None:
        self.exe = Path(exe)
        self.model = Path(model)
        self.mmproj = Path(mmproj) if mmproj else None
        self.port = port
        self.ctx = ctx
        self.flash_attn = flash_attn
        self.kv_type = kv_type
        self.draft_model = Path(draft_model) if draft_model else None
        self.draft_max = draft_max
        self.draft_min = draft_min
        self.spec_ngram = spec_ngram
        self.ngram_type = ngram_type
        self.ngram_args = list(ngram_args or [])
        self._proc: subprocess.Popen | None = None
        self._stderr: collections.deque[str] = collections.deque(maxlen=200)
        self._reader: threading.Thread | None = None

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def flags_description(self) -> str:
        fa = "on" if self.flash_attn else "off"
        desc = f"-c {self.ctx} -fa {fa} --cache-type-k {self.kv_type} --cache-type-v {self.kv_type}"
        if self.mmproj:
            desc += f" --mmproj {self.mmproj.name}"
        spec = self._spec_argv()
        return f"{desc} {' '.join(spec)}" if spec else desc

    def _spec_argv(self) -> list[str]:
        """Speculative-decoding flags (b11982 names: see `llama-server --help`, speculative section)."""
        types: list[str] = []
        argv: list[str] = []
        if self.draft_model:
            types.append("draft-simple")
            argv += [
                "-md", str(self.draft_model),
                "-ngld", "99",
                "--spec-draft-n-max", str(self.draft_max),
                "--spec-draft-n-min", str(self.draft_min),
            ]
        if self.spec_ngram:
            types.append(self.ngram_type)
            argv += self.ngram_args
        return ["--spec-type", ",".join(types), *argv] if types else []

    def _argv(self) -> list[str]:
        argv = [
            str(self.exe),
            "-m", str(self.model),
            "--host", "127.0.0.1",
            "--port", str(self.port),
            "--jinja",
            "-c", str(self.ctx),
            "-ngl", "99",
            "-fa", "on" if self.flash_attn else "off",
            "--cache-type-k", self.kv_type,
            "--cache-type-v", self.kv_type,
            "--chat-template-kwargs", '{"enable_thinking":false}',
        ]
        if self.mmproj:
            argv += ["--mmproj", str(self.mmproj)]
        return argv + self._spec_argv()

    def _drain(self, stream) -> None:
        for line in stream:
            self._stderr.append(line.rstrip())

    def stderr_tail(self) -> str:
        if self._reader is not None:
            self._reader.join(timeout=1.0)
        return "\n".join(self._stderr)

    def start(self, timeout_s: float = 60) -> None:
        if self._proc is not None:
            raise RuntimeError("server already started")
        self._stderr.clear()
        self._fail_if_port_in_use()
        self._proc = subprocess.Popen(
            self._argv(),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        try:
            self._reader = threading.Thread(target=self._drain, args=(self._proc.stderr,), daemon=True)
            self._reader.start()
            self._wait_healthy(timeout_s)
        except BaseException:
            # Never leave a half-started llama-server holding VRAM (incl. KeyboardInterrupt).
            self.stop()
            raise

    def _fail_if_port_in_use(self) -> None:
        """A stale server on our port would make /health succeed against the wrong process."""
        try:
            with socket.create_connection(("127.0.0.1", self.port), timeout=0.5):
                pass
        except OSError:
            return
        raise LlamaServerError(f"127.0.0.1:{self.port} is already accepting connections (stale llama-server?)")

    def _wait_healthy(self, timeout_s: float) -> None:
        assert self._proc is not None
        deadline = time.monotonic() + timeout_s
        with httpx.Client(timeout=2.0) as http:
            while time.monotonic() < deadline:
                if self._proc.poll() is not None:
                    code = self._proc.returncode
                    raise LlamaServerError(f"llama-server exited with code {code}", self.stderr_tail())
                try:
                    if http.get(f"{self.base_url}/health").status_code == 200:
                        return
                except httpx.HTTPError:
                    pass
                time.sleep(0.2)
        raise LlamaServerError(f"llama-server not healthy after {timeout_s}s", self.stderr_tail())

    def is_alive(self) -> bool:
        return self._proc is not None and self._proc.poll() is None

    def stop(self) -> None:
        proc, self._proc = self._proc, None
        if proc is None:
            return
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

    def __enter__(self) -> LlamaServer:
        self.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop()
