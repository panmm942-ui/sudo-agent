"""Unit tests for bench/metrics.py and the pure helpers in bench/record_samples.py."""

import json
import sys
from pathlib import Path

import numpy as np
import pytest

BENCH = Path(__file__).resolve().parents[2] / "bench"
sys.path.insert(0, str(BENCH))

from metrics import normalize, wer  # noqa: E402
from record_samples import is_silent, is_spoken_form, select_fixtures  # noqa: E402


def test_normalize_drops_english_fillers():
    assert normalize("sudo, um, play the music") == "sudo play the music"
    assert normalize("Uh open, erm, hmm Spotify") == "open spotify"


def test_normalize_drops_greek_fillers():
    assert normalize("Εε, άνοιξε το chrome") == "ανοιξε το chrome"
    assert normalize("εμ βάλε ε μουσική χμ") == "βαλε μουσικη"


def test_normalize_keeps_words_that_only_contain_fillers():
    assert normalize("umbrella hummus") == "umbrella hummus"


def test_wer_ignores_fillers_on_both_sides():
    assert wer("sudo, um, play the music", "sudo play the music") == 0.0
    assert wer("open spotify", "uh open spotify") == 0.0


def test_spoken_form_filter():
    assert is_spoken_form("play the next one")
    assert is_spoken_form("am I free on Friday afternoon")  # "am" the verb is not a time
    for text in ["set the volume to 30", "open youtube.com", "άνοιξε το skroutz.gr", "alarm at seven AM",
                 "call mom at six p.m.", "go to https://example.org", "open www.example"]:
        assert not is_spoken_form(text), text


def test_normalize_strips_accents_case_punct():
    assert normalize("Άνοιξε το Spotify!") == "ανοιξε το spotify"


def test_normalize_final_sigma():
    assert normalize("Πρόσεχε, ΣΟΥΔΑΣ") == "προσεχε σουδασ"


def test_normalize_collapses_spaces():
    assert normalize("  hello   world  ") == "hello world"


def test_wer_ignores_case_and_accents():
    assert wer("άνοιξε το spotify", "Ανοιξε το Spotify") == 0.0


def test_wer_counts_errors():
    assert wer("open the door", "open a door") == pytest.approx(1 / 3)


def test_is_silent():
    assert is_silent(np.zeros(16000)) is True
    assert is_silent(0.1 * np.sin(np.linspace(0, 2000, 16000))) is False
    assert is_silent(np.zeros(0)) is True


def test_select_fixtures_counts_and_determinism():
    path = BENCH / "fixtures" / "commands.jsonl"
    rows = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    a = select_fixtures(rows)
    b = select_fixtures(list(reversed(rows)))
    assert [r["id"] for r in a] == [r["id"] for r in b]
    assert len(a) == 40 and len({r["id"] for r in a}) == 40
    assert sum(r["lang"] == "en" for r in a) == 20
    assert sum(r["lang"] in ("el", "mixed") for r in a) == 20
    # spoken form only: no digits, URLs/domains or AM/PM, so WER measures recognition, not formatting
    assert not any(any(ch.isdigit() for ch in r["text"]) for r in a)
    assert all(is_spoken_form(r["text"]) for r in a)
    # stratified: every intent that has an eligible English fixture appears in the English half
    eligible_en = {r["intent"] for r in rows if r["lang"] == "en" and is_spoken_form(r["text"])}
    assert {r["intent"] for r in a if r["lang"] == "en"} == eligible_en


def test_capture_stops_on_enter():
    from record_samples import capture

    n = {"reads": 0}

    def read():
        n["reads"] += 1
        return np.ones((1600, 1), dtype=np.float32)

    out = capture(read, lambda: n["reads"] >= 3, lambda: 0.0)
    assert len(out) == 3 * 1600 and out.ndim == 1


def test_capture_caps_at_max_seconds_without_enter():
    from record_samples import capture

    t = {"now": 0.0}

    def read():
        t["now"] += 0.1  # each block takes 0.1 s of fake time
        return np.ones(1600, dtype=np.float32)

    out = capture(read, lambda: False, lambda: t["now"], max_seconds=6.0)
    assert 59 <= len(out) / 1600 <= 61


def test_capture_empty_when_enter_immediately():
    from record_samples import capture

    assert len(capture(lambda: np.ones(1), lambda: True, lambda: 0.0)) == 0
