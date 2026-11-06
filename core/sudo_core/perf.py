"""Timing helpers shared by the Phase 0 benchmarks."""

from __future__ import annotations

import time
from dataclasses import dataclass

import numpy as np


class Stopwatch:
    """Context manager measuring wall time; `.ms` is set on exit."""

    ms: float = 0.0

    def __enter__(self) -> Stopwatch:
        self._start = time.perf_counter()
        return self

    def __exit__(self, *exc: object) -> None:
        self.ms = (time.perf_counter() - self._start) * 1000.0


@dataclass(frozen=True)
class Stats:
    n: int
    mean: float
    p50: float
    p95: float


def summarize(samples: list[float]) -> Stats:
    if not samples:
        raise ValueError("cannot summarize an empty sample list")
    arr = np.asarray(samples, dtype=float)
    return Stats(
        n=len(arr),
        mean=float(arr.mean()),
        p50=float(np.percentile(arr, 50)),
        p95=float(np.percentile(arr, 95)),
    )
