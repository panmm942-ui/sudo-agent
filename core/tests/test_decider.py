from sudo_core.brain.decider import Decider, Decision

LABELS = {"media.next": "Skip to the next track", "volume.set": "Set the volume", "chat": "General chat"}


class FakeExtractor:
    def __init__(self, result):
        self.result = result
        self.calls: list[tuple] = []

    def classify_text(self, text, tasks, **kwargs):
        self.calls.append((text, tasks, kwargs))
        return self.result


def test_decide_maps_label_and_score():
    fake = FakeExtractor({"intent": {"label": "media.next", "confidence": 0.91}})
    d = Decider(extractor=fake).decide("next song", LABELS)
    assert isinstance(d, Decision)
    assert d.intent == "media.next"
    assert d.score == 0.91
    assert d.latency_ms >= 0.0
    text, tasks, kwargs = fake.calls[0]
    assert text == "next song"
    assert tasks == {"intent": {"labels": LABELS}}
    assert kwargs["include_confidence"] is True


def test_unknown_or_missing_label_falls_back_to_chat():
    for result in ({"intent": None}, {}, {"intent": {"label": "bogus.intent", "confidence": 0.99}}):
        d = Decider(extractor=FakeExtractor(result)).decide("hmm", LABELS)
        assert (d.intent, d.score) == ("chat", 0.0)


def test_plain_string_result_is_tolerated():
    d = Decider(extractor=FakeExtractor({"intent": "volume.set"})).decide("louder", LABELS)
    assert (d.intent, d.score) == ("volume.set", 0.0)


def test_descriptions_mode_sends_flat_description_labels_and_maps_back():
    fake = FakeExtractor({"intent": {"label": "Skip to the next track", "confidence": 0.8}})
    d = Decider(extractor=fake, label_mode="descriptions").decide("next song", LABELS)
    assert (d.intent, d.score) == ("media.next", 0.8)
    assert fake.calls[0][1] == {"intent": list(LABELS.values())}


def test_descriptions_mode_unknown_label_falls_back_to_chat():
    fake = FakeExtractor({"intent": {"label": "something else", "confidence": 0.9}})
    d = Decider(extractor=fake, label_mode="descriptions").decide("x", LABELS)
    assert (d.intent, d.score) == ("chat", 0.0)
