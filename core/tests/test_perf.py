import time

import pytest

from sudo_core.perf import Stopwatch, summarize


def test_summarize_percentiles():
    s = summarize([float(x) for x in range(1, 11)])
    assert (s.n, s.mean, s.p50, round(s.p95, 2)) == (10, 5.5, 5.5, 9.55)


def test_summarize_empty_raises():
    with pytest.raises(ValueError):
        summarize([])


def test_stopwatch_measures_elapsed_ms():
    with Stopwatch() as sw:
        time.sleep(0.02)
    assert 15 <= sw.ms < 500
