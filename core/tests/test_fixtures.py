"""Coverage checks for the bilingual command fixture set (bench/fixtures)."""

import json
from collections import Counter, defaultdict
from pathlib import Path

FIXTURES = Path(__file__).resolve().parents[2] / "bench" / "fixtures"

EXPECTED_INTENTS = {
    "media.play_pause", "media.next", "media.previous", "volume.set", "volume.mute",
    "app.open", "app.close", "window.snap", "web.search", "web.open", "alarm.set",
    "timer.set", "reminder.create", "schedule.query", "calendar.create",
    "email.summary", "system.lock", "system.screenshot", "settings.open", "chat",
}


def _load():
    intents = json.loads((FIXTURES / "intents.json").read_text(encoding="utf-8"))
    lines = [
        json.loads(line)
        for line in (FIXTURES / "commands.jsonl").read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    return intents, lines


def test_fixture_coverage():
    intents, lines = _load()

    assert set(intents) == EXPECTED_INTENTS
    for iid, spec in intents.items():
        assert {"en", "el", "params"} <= set(spec), iid
        assert spec["en"] and spec["el"], iid
        assert isinstance(spec["params"], dict), iid

    assert len(lines) >= 150
    ids = [c["id"] for c in lines]
    assert len(ids) == len(set(ids)), "duplicate ids"
    texts = [c["text"].strip().lower() for c in lines]
    assert len(texts) == len(set(texts)), "duplicate texts"

    by_intent = defaultdict(Counter)
    for c in lines:
        assert set(c) == {"id", "text", "lang", "intent", "slots"}, c["id"]
        assert c["text"].strip(), c["id"]
        assert c["lang"] in {"en", "el", "mixed"}, c["id"]
        assert c["intent"] in intents, c["id"]
        assert isinstance(c["slots"], dict), c["id"]
        extra = set(c["slots"]) - set(intents[c["intent"]]["params"])
        assert not extra, f"{c['id']}: slots {extra} not in params of {c['intent']}"
        by_intent[c["intent"]][c["lang"]] += 1

    for iid in intents:
        counts = by_intent[iid]
        assert counts["en"] >= 3, f"{iid}: only {counts['en']} en"
        assert counts["el"] + counts["mixed"] >= 3, f"{iid}: only {counts['el'] + counts['mixed']} el/mixed"

    assert sum(1 for c in lines if c["lang"] == "mixed") >= 15
