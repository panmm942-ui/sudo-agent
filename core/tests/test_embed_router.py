import numpy as np
import pytest

from sudo_core.brain.decider import Decision
from sudo_core.brain.embed_router import EmbedRouter

VECS = {
    "pause": [1.0, 0.0, 0.0],
    "stop it": [0.9, 0.1, 0.0],
    "louder": [0.0, 1.0, 0.0],
    "volume up": [0.1, 0.9, 0.0],
    "next": [0.0, 0.0, 1.0],
    "off topic": [-1.0, 0.0, 0.0],
    "between": [1.0, 1.0, 0.0],
}


class FakeEncoder:
    """Deterministic: looks the (prefix-stripped) text up in VECS; counts calls and texts."""

    def __init__(self, vecs=VECS):
        self.vecs = vecs
        self.calls: list[list[str]] = []

    def encode(self, texts: list[str]) -> np.ndarray:
        self.calls.append(list(texts))
        return np.array([self.vecs[t.removeprefix("query: ")] for t in texts], dtype=np.float32)


EXEMPLARS = {
    "media.play_pause": ["pause", "stop it"],
    "volume.up": ["louder", "volume up"],
    "media.next": ["next"],
}


def make(exemplars=EXEMPLARS, **kw):
    enc = FakeEncoder()
    return EmbedRouter(".", exemplars, encoder=enc, **kw), enc


def test_nearest_exemplar_intent_wins_and_returns_decision():
    router, _ = make()
    d = router.decide("louder")
    assert isinstance(d, Decision)
    assert d.intent == "volume.up"
    assert d.latency_ms >= 0.0
    assert router.decide("next").intent == "media.next"
    assert router.decide("stop it").intent == "media.play_pause"


def test_score_is_max_cosine_similarity_in_range():
    router, _ = make()
    assert router.decide("pause").score == pytest.approx(1.0, abs=1e-5)
    d = router.decide("between")  # closest exemplar is "stop it": cos = 1.0 / (sqrt(2) * sqrt(0.82))
    assert d.score == pytest.approx(1.0 / np.sqrt(2 * 0.82), abs=1e-4)
    assert -1.0 <= router.decide("off topic").score <= 1.0


def test_vectors_are_normalized_so_scale_does_not_matter():
    enc = FakeEncoder({"a": [10.0, 0.0], "b": [0.0, 0.1], "q": [3.0, 0.0]})
    router = EmbedRouter(".", {"A": ["a"], "B": ["b"]}, encoder=enc)
    d = router.decide("q")
    assert d.intent == "A"
    assert d.score == pytest.approx(1.0, abs=1e-5)


def test_intent_with_empty_exemplar_list_is_ignored():
    router, _ = make({**EXEMPLARS, "empty": []})
    for text in VECS:
        assert router.decide(text).intent != "empty"


def test_no_exemplars_at_all_degrades_to_chat():
    router, _ = make({"a": [], "b": []})
    d = router.decide("pause")
    assert (d.intent, d.score) == ("chat", 0.0)


def test_add_exemplar_changes_later_decision():
    router, _ = make()
    assert router.decide("between").intent == "media.play_pause"
    router.add_exemplar("media.next", "between")
    d = router.decide("between")
    assert d.intent == "media.next"
    assert d.score == pytest.approx(1.0, abs=1e-5)


def test_add_exemplar_creates_new_intent():
    router, _ = make()
    router.add_exemplar("brand.new", "off topic")
    assert router.decide("off topic").intent == "brand.new"


def test_exemplars_encoded_once_at_construction():
    router, enc = make()
    assert len(enc.calls) == 1
    assert sorted(enc.calls[0]) == sorted(["pause", "stop it", "louder", "volume up", "next"])
    for _ in range(3):
        router.decide("louder")
    # each decide encodes only the query
    assert len(enc.calls) == 4
    assert all(len(c) == 1 for c in enc.calls[1:])


def test_add_exemplar_encodes_only_the_new_text():
    router, enc = make()
    n = len(enc.calls)
    router.add_exemplar("media.next", "off topic")
    assert len(enc.calls) == n + 1
    assert len(enc.calls[-1]) == 1


def test_query_prefix_applied_to_queries_and_exemplars():
    enc = FakeEncoder()
    router = EmbedRouter(".", EXEMPLARS, encoder=enc, query_prefix="query: ")
    router.decide("louder")
    router.add_exemplar("media.next", "off topic")
    seen = [t for call in enc.calls for t in call]
    assert seen and all(t.startswith("query: ") for t in seen)


def test_exclude_text_drops_matching_exemplars_for_leave_one_out():
    router, _ = make()
    assert router.decide("louder").score == pytest.approx(1.0, abs=1e-5)
    d = router.decide("louder", exclude_text="louder")
    assert d.intent == "volume.up"  # "volume up" is still there
    assert d.score < 1.0
    # excluding the only exemplar of an intent removes that intent from contention
    assert router.decide("next", exclude_text="next").intent != "media.next"
    # the router itself is not modified
    assert router.decide("next").intent == "media.next"
