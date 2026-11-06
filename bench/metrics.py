"""Text normalization and word error rate for the STT bench (Greek + English)."""

from __future__ import annotations

import re
import unicodedata

import jiwer

_PUNCT = re.compile(r"[^\w\s]", re.UNICODE)

# Hesitation tokens dropped from reference and hypothesis alike: Parakeet does not transcribe disfluencies, so a
# scripted "um" would count as a deletion. Kept deliberately tiny; compared after accent stripping/casefolding.
#   en: um, uh, erm, hmm        el: εε, εμ, ε, χμ
FILLERS = frozenset({"um", "uh", "erm", "hmm", "εε", "εμ", "ε", "χμ"})


def normalize(text: str) -> str:
    """NFD, drop combining marks, casefold, final sigma -> sigma, strip punctuation, drop FILLERS, collapse spaces."""
    decomposed = unicodedata.normalize("NFD", text)
    stripped = "".join(c for c in decomposed if not unicodedata.combining(c))
    folded = stripped.casefold().replace("ς", "σ")
    return " ".join(w for w in _PUNCT.sub(" ", folded).split() if w not in FILLERS)


def wer(ref: str, hyp: str) -> float:
    """Word error rate between normalized strings. An empty reference is an error (ValueError)."""
    r, h = normalize(ref), normalize(hyp)
    if not r:
        raise ValueError("empty reference after normalization")
    if not h:
        return 1.0  # jiwer rejects an empty hypothesis in some versions; all words deleted = 1.0
    return float(jiwer.wer(r, h))
