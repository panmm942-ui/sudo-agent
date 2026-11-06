"""Unit tests for the pure helpers in bench/bench_vram.py."""

import sys
from pathlib import Path

BENCH = Path(__file__).resolve().parents[2] / "bench"
sys.path.insert(0, str(BENCH))

from bench_vram import pid_sum_mib, vram_figure  # noqa: E402


def test_pid_sum_when_every_sudo_pid_reports():
    rows = ["100, 3000", "200, 900", "300, 50"]
    assert pid_sum_mib(rows, {100, 200}) == 3900


def test_pid_sum_none_when_one_sudo_pid_is_na():
    rows = ["100, 3000", "200, [N/A]"]
    assert pid_sum_mib(rows, {100, 200}) is None


def test_pid_sum_none_when_one_sudo_pid_is_missing():
    rows = ["100, 3000", "300, 50"]
    assert pid_sum_mib(rows, {100, 200}) is None


def test_pid_sum_none_under_wddm():
    assert pid_sum_mib(["100, [N/A]", "200, [N/A]"], {100, 200}) is None


def test_vram_figure_prefers_complete_pid_sum_else_delta():
    assert vram_figure(3900, 4400) == (3900, "per-process sum")
    mib, method = vram_figure(None, 4400)
    assert mib == 4400 and method.startswith("total-used minus idle")
    assert vram_figure(None, None) == (None, "unavailable")
