import json

import httpx

from sudo_core.llm.client import LlmClient
from sudo_core.llm.tools import intents_to_tools


def _sse(chunks: list[dict]) -> bytes:
    body = "".join(f"data: {json.dumps(c)}\n\n" for c in chunks) + "data: [DONE]\n\n"
    return body.encode()


def _tc(index: int = 0, name: str | None = None, args: str | None = None) -> dict:
    fn: dict = {}
    if name is not None:
        fn["name"] = name
    if args is not None:
        fn["arguments"] = args
    return {"choices": [{"index": 0, "delta": {"tool_calls": [{"index": index, "function": fn}]}}]}


def _client(chunks: list[dict], seen: list[dict] | None = None) -> LlmClient:
    def handler(request: httpx.Request) -> httpx.Response:
        if seen is not None:
            seen.append(json.loads(request.content))
        return httpx.Response(200, content=_sse(chunks), headers={"content-type": "text/event-stream"})

    http = httpx.Client(transport=httpx.MockTransport(handler))
    return LlmClient("http://test", http=http)


USAGE = {"choices": [], "usage": {"completion_tokens": 12}}


def test_tool_call_assembles_streamed_arguments():
    seen: list[dict] = []
    client = _client(
        [_tc(name="volume_set"), _tc(args='{"lev'), _tc(args='el": 30}'), USAGE], seen
    )
    r = client.tool_call([{"role": "user", "content": "volume 30"}], [])
    assert r.name == "volume_set"
    assert r.arguments == {"level": 30}
    assert r.valid is True
    assert r.ttft_ms > 0
    assert r.total_ms >= r.ttft_ms
    assert r.completion_tokens == 12
    body = seen[0]
    assert body["tool_choice"] == "required"
    assert body["temperature"] == 0
    assert body["stream"] is True
    assert body["max_tokens"] == 160
    assert body["stream_options"] == {"include_usage": True}


def test_malformed_arguments_are_invalid_not_raised():
    client = _client([_tc(name="volume_set"), _tc(args='{"level": }'), USAGE])
    r = client.tool_call([{"role": "user", "content": "x"}], [])
    assert r.name == "volume_set"
    assert r.valid is False
    assert r.arguments is None


def test_no_tool_call_is_invalid():
    client = _client(
        [{"choices": [{"index": 0, "delta": {"content": "hello there"}}]}, USAGE]
    )
    r = client.tool_call([{"role": "user", "content": "x"}], [])
    assert r.name is None
    assert r.valid is False


def test_empty_arguments_are_valid_empty_dict():
    client = _client([_tc(name="media_next"), USAGE])
    r = client.tool_call([{"role": "user", "content": "next"}], [])
    assert r.name == "media_next"
    assert r.arguments == {}
    assert r.valid is True


def test_intents_to_tools_names():
    intents = {
        "media.next": {"en": "Skip to the next track", "el": "x", "params": {}},
        "volume.set": {
            "en": "Set volume",
            "el": "y",
            "params": {"level": {"type": "integer"}},
        },
        "chat": {"en": "Casual conversation", "el": "z", "params": {}},
    }
    tools = intents_to_tools(intents)
    by_name = {t["function"]["name"]: t for t in tools}
    assert set(by_name) == {"media_next", "volume_set", "chat"}
    vol = by_name["volume_set"]
    assert vol["type"] == "function"
    assert vol["function"]["description"] == "Set volume"
    assert vol["function"]["parameters"] == {
        "type": "object",
        "properties": {"level": {"type": "integer"}},
    }
    assert by_name["media_next"]["function"]["parameters"] == {
        "type": "object",
        "properties": {},
    }
    assert by_name["chat"]["function"]["parameters"]["properties"] == {}


def test_name_ms_set_when_name_streams_before_args():
    client = _client([_tc(name="volume_set"), _tc(args='{"level": 30}'), USAGE])
    r = client.tool_call([{"role": "user", "content": "x"}], [])
    assert r.name_ms is not None
    assert 0 < r.name_ms <= r.total_ms
    assert r.name_ms >= r.ttft_ms


def test_name_ms_none_without_tool_call():
    client = _client([{"choices": [{"index": 0, "delta": {"content": "hi"}}]}, USAGE])
    r = client.tool_call([{"role": "user", "content": "x"}], [])
    assert r.name_ms is None


def test_finish_reason_length_is_invalid():
    chunks = [
        _tc(name="chat"),
        _tc(args="{}"),
        {"choices": [{"index": 0, "delta": {}, "finish_reason": "length"}]},
        USAGE,
    ]
    r = _client(chunks).tool_call([{"role": "user", "content": "x"}], [])
    assert r.finish_reason == "length"
    assert r.valid is False
    assert r.name == "chat"


def test_finish_reason_tool_calls_is_valid():
    chunks = [
        _tc(name="chat"),
        {"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]},
        USAGE,
    ]
    r = _client(chunks).tool_call([{"role": "user", "content": "x"}], [])
    assert r.finish_reason == "tool_calls"
    assert r.valid is True


def test_system_prompt_has_chat_vs_search_line():
    from sudo_core.llm.tools import SYSTEM_PROMPT

    assert (
        "Use chat for questions you can answer yourself or small talk; "
        "use web_search only when the user asks to search or look something up."
    ) in SYSTEM_PROMPT


def test_system_prompt_forbids_plain_text_replies():
    from sudo_core.llm.tools import SYSTEM_PROMPT

    # Measured: without this the model answers in plain text first and runs to max_tokens.
    assert "Never write a reply yourself" in SYSTEM_PROMPT
