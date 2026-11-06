"""Compare speech-to-text setups for Sudo AI on real Greek and English speakers (dev tool).

FLEURS test recordings (google/fleurs, el_gr + en_us), word error rate after the same text
normalisation for every model (lower case, no punctuation, no Greek accents), plus average
recognition time per utterance. Every setup uses Sudo AI's own single-pass Asr class.
Run with the Jarvis voice environment:  python -I stt_bench.py --data <sudo data root> --bench <stt-bench>
"""
import argparse
import csv
import json
import re
import sys
import time
import unicodedata
from pathlib import Path

import numpy as np


def normalise(text):
    text = unicodedata.normalize('NFD', text.lower())
    text = ''.join(c for c in text if unicodedata.category(c) != 'Mn')  # drop accents / tonos
    text = text.replace('ς', 'σ')
    text = re.sub(r"[^\w\s']", ' ', text)
    return text.split()


def edit_distance(ref, hyp):
    row = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        prev, row[0] = row[0], i
        for j, h in enumerate(hyp, 1):
            prev, row[j] = row[j], min(row[j] + 1, row[j - 1] + 1, prev + (r != h))
    return row[-1]


def fleurs(folder, limit):
    import soundfile as sf
    items = []
    with open(folder / 'test.tsv', encoding='utf-8') as handle:
        for row in csv.reader(handle, delimiter='\t', quoting=csv.QUOTE_NONE):
            path = folder / 'test' / row[1]
            if path.is_file():
                items.append((path, row[2]))  # raw transcription with casing and punctuation
            if len(items) >= limit:
                break
    out = []
    for path, text in items:
        audio, rate = sf.read(str(path), dtype='float32')
        if rate != 16000:
            target = np.linspace(0, len(audio) - 1, int(len(audio) * 16000 / rate))
            audio = np.interp(target, np.arange(len(audio)), audio).astype(np.float32)
        if len(audio) <= 29 * 16000:
            out.append((audio, text))
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--bench', type=Path, required=True)
    parser.add_argument('--limit', type=int, default=200)
    parser.add_argument('--only', default='')
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src-tauri' / 'resources'))
    import jarvis_voice as jv

    sets = {lang: fleurs(args.bench / 'fleurs' / folder, args.limit) for lang, folder in [('el', 'el_gr'), ('en', 'en_us')]}
    turbo = args.data / 'jarvis-models' / 'whisper-large-v3-turbo'
    setups = [
        ('turbo (shipped before)', turbo, 3, []),
        ('turbo + beam 5 + vocabulary', turbo, 5, jv.VOCABULARY),
        ('large-v3 + beam 5 + vocabulary', args.bench / 'large-v3', 5, jv.VOCABULARY),
        ('greek fine-tune + beam 5 + vocabulary', args.bench / 'greek-ft-ct2', 5, jv.VOCABULARY),
    ]
    results = []
    for name, folder, beam, vocabulary in setups:
        if args.only and args.only not in name:
            continue
        if not (folder / 'model.bin').is_file():
            print(f'skip {name}: {folder} missing', flush=True)
            continue
        asr = jv.Asr(folder, beam_size=beam, vocabulary=vocabulary)
        asr.transcribe(np.zeros(16000, np.float32))
        row = {'setup': name, 'device': asr.device}
        for lang, items in sets.items():
            errors = words = wrong_lang = 0
            seconds = []
            for audio, text in items:
                start = time.perf_counter()
                hyp, detected = asr.transcribe(audio)
                seconds.append(time.perf_counter() - start)
                ref = normalise(text)
                errors += edit_distance(ref, normalise(hyp))
                words += len(ref)
                wrong_lang += detected != lang
            row[lang] = {'wer_percent': round(100 * errors / words, 2), 'utterances': len(items),
                         'avg_ms': round(1000 * float(np.mean(seconds))), 'wrong_language': wrong_lang}
            print(name, lang, row[lang], flush=True)
        results.append(row)
        del asr
    print(json.dumps(results, ensure_ascii=False, indent=2))
    (args.bench / 'results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding='utf-8')


if __name__ == '__main__':
    main()
