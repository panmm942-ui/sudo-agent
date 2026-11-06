"""Streaming tool-call client for llama-server's OpenAI-compatible API."""

from __future__ import annotations

import json
import time
from dataclasses import dataclass

import httpx


@dataclass(frozen=True)
class ToolCallResult:
    name: str | None
    arguments: dict | None
    valid: bool
    ttft_ms: float
    total_ms: float
    completion_tokens: int
    # Time until the function name is complete in the stream (the UI can switch to "acting").
    name_ms: float | None = None
    finish_reason: str | None = None


class LlmClient:
    def __init__(self, base_url: str, http: httpx.Client | None = None, model: str = "sudo",
                 max_tokens: int = 160) -> None:
        self._http = http or httpx.Client(timeout=httpx.Timeout(60.0))
        self._url = base_url.rstrip("/") + "/v1/chat/completions"
        self._model = model
        # Cap generation: a degenerate tool call (seen with `chat`) otherwise runs to the context limit.
        self._max_tokens = max_tokens

    def tool_call(self, messages: list[dict], tools: list[dict]) -> ToolCallResult:
        body = {
            "model": self._model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "required",
            "temperature": 0,
            "max_tokens": self._max_tokens,
            "stream": True,
            "stream_options": {"include_usage": True},
        }
        start = time.perf_counter()
        ttft_ms = 0.0
        # index -> {"name": str, "arguments": str}
        calls: dict[int, dict[str, str]] = {}
        completion_tokens = 0
        name_ms: float | None = None
        finish_reason: str | None = None

        with self._http.stream("POST", self._url, json=body) as resp:
            resp.raise_for_status()
            for line in resp.iter_lines():
                if not line.startswith("data:"):
                    continue
                payload = line[5:].strip()
                if payload == "[DONE]":
                    break
                try:
                    chunk = json.loads(payload)
                except json.JSONDecodeError:
                    continue
                usage = chunk.get("usage")
                if usage:
                    completion_tokens = int(usage.get("completion_tokens") or 0)
                for choice in chunk.get("choices") or []:
                    delta = choice.get("delta") or {}
                    if choice.get("finish_reason"):
                        finish_reason = choice["finish_reason"]
                    for tc in delta.get("tool_calls") or []:
                        fn = tc.get("function") or {}
                        slot = calls.setdefault(int(tc.get("index", 0)), {"name": "", "arguments": ""})
                        if not ttft_ms:
                            ttft_ms = (time.perf_counter() - start) * 1000.0
                        if fn.get("name"):
                            slot["name"] += fn["name"]
                            if not slot["arguments"] and min(calls) == int(tc.get("index", 0)):
                                name_ms = (time.perf_counter() - start) * 1000.0
                        if fn.get("arguments"):
                            slot["arguments"] += fn["arguments"]
                    if not ttft_ms and delta.get("content"):
                        ttft_ms = (time.perf_counter() - start) * 1000.0
        total_ms = (time.perf_counter() - start) * 1000.0
        if not ttft_ms:
            ttft_ms = total_ms

        if not calls:
            return ToolCallResult(None, None, False, ttft_ms, total_ms, completion_tokens, None, finish_reason)
        first = calls[min(calls)]
        name = first["name"] or None
        raw = first["arguments"].strip()
        try:
            args = json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            args = None
        # A call cut off by max_tokens is never trusted, even if its JSON happens to parse.
        valid = name is not None and isinstance(args, dict) and finish_reason != "length"
        return ToolCallResult(
            name, args if valid else None, valid, ttft_ms, total_ms, completion_tokens,
            name_ms if name else None, finish_reason,
        )

    def generate_text(self, prompt: str, max_tokens: int = 256) -> dict:
        """Non-streaming free-text generation; returns the response JSON (incl. server `timings`)."""
        body = {
            "model": self._model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "temperature": 0,
            "stream": False,
        }
        resp = self._http.post(self._url, json=body)
        resp.raise_for_status()
        return resp.json()
