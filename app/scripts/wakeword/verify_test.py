"""Measure the second wake step (dev tool): tiny Whisper on the CPU must hear "sudo" in the last
1.5 s before the detector fired. Accept rate on held-out "Hey Sudo" clips (with real talk or noise
before them) and on detector firings in held-out real speech.

Run with the Jarvis voice environment (faster-whisper), the training env's model and data.
"""
import argparse
import sys
import time
from pathlib import Path

import numpy as np

SR = 16000


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True, help='hey_sudo.onnx')
    parser.add_argument('--wake', type=Path, required=True)
    parser.add_argument('--tiny', type=Path, required=True)
    parser.add_argument('--threshold', type=float, default=0.9)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'src-tauri' / 'resources'))
    import jarvis_voice as jv
    load = lambda name: list(np.load(args.data / f'{name}.npz', allow_pickle=True)['clips'])  # our own generated files
    verifier = jv.WakeVerifier(args.tiny)

    rng = np.random.default_rng(5)
    speech_test = load('speech_test')
    accepted, seconds = 0, []
    positives = load('pos_test')
    for clip in positives:
        lead = speech_test[rng.integers(len(speech_test))]
        start = rng.integers(0, max(1, len(lead) - SR))
        audio = np.concatenate([lead[start:start + SR] * rng.uniform(0.2, 0.8), clip, np.zeros(int(0.1 * SR), np.float32)])
        t = time.perf_counter()
        ok, _ = verifier(audio[-int(1.5 * SR):] if len(clip) < 1.4 * SR else audio[-len(clip) - 1600:])
        seconds.append(time.perf_counter() - t)
        accepted += ok
    print(f'positives accepted by step 2: {accepted}/{len(positives)} ({100 * accepted / len(positives):.1f}%), avg {1000 * np.mean(seconds):.0f} ms', flush=True)

    from openwakeword.model import Model
    oww = Model(wakeword_models=[str(args.model)], inference_framework='onnx',
                melspec_model_path=str(args.wake / 'melspectrogram.onnx'), embedding_model_path=str(args.wake / 'embedding_model.onnx'))
    fired = passed = 0
    hours = 0.0
    heard = []
    for clip in speech_test:
        pcm = (np.clip(clip, -1, 1) * 32767).astype(np.int16)
        oww.reset()
        quiet = 0
        for i in range(0, len(pcm) - 1280, 1280):
            score = max(oww.predict(pcm[i:i + 1280]).values())
            if quiet:
                quiet -= 1
                continue
            if score >= args.threshold:
                fired += 1
                quiet = 25
                end = i + 1280
                ok, text = verifier(clip[max(0, end - int(1.5 * SR)):end])
                passed += ok
                heard.append((ok, text))
        hours += len(pcm) / SR / 3600
    print(f'real speech {hours:.2f} h: step 1 fired {fired} times ({fired / hours:.1f}/h), after step 2: {passed} ({passed / hours:.2f}/h)')
    for ok, text in heard[:25]:
        print('  ', 'ACCEPT' if ok else 'reject', repr(text))


if __name__ == '__main__':
    main()
