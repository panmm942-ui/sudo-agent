"""Train the "Hey Sudo" wake-word detector on openWakeWord features (dev tool, never shipped).

Input: the .npz splits from generate.py. Output: hey_sudo.onnx with the same interface as
openWakeWord's pretrained models ([1, 16, 96] embeddings -> [1, 1] probability), plus an honest
report: recall on held-out voices, false triggers on held-out look-alikes, and false triggers per
hour streaming held-out real speech through openWakeWord exactly as the app does.
"""
import argparse
import json
from pathlib import Path

import numpy as np

SR = 16000
FRAMES = 16          # embedding frames the detector sees (~1.28 s)
CLIP = 2 * SR        # audio rendered per training window before feature extraction


def load(folder, name):
    # allow_pickle is needed for the ragged clip arrays; these files are only ever the ones
    # generate.py just wrote locally, never downloaded data.
    return list(np.load(folder / f'{name}.npz', allow_pickle=True)['clips'])


class Augment:
    def __init__(self, babble, seed):
        self.rng = np.random.default_rng(seed)
        self.babble = babble

    def noise(self, n):
        kind = self.rng.integers(0, 4)
        if kind == 3 and self.babble:  # other people talking in the background
            src = self.babble[self.rng.integers(len(self.babble))]
            if len(src) > n:
                start = self.rng.integers(0, len(src) - n)
                return src[start:start + n].copy()
        white = self.rng.standard_normal(n).astype(np.float32)
        if kind == 0:
            return white
        spectrum = np.fft.rfft(white)
        f = np.maximum(np.fft.rfftfreq(n), 1 / n)
        spectrum /= f ** (0.5 if kind == 1 else 1.0)  # pink / brown
        out = np.fft.irfft(spectrum, n).astype(np.float32)
        return out / (np.std(out) + 1e-9)

    def _speech(self, n):
        for _ in range(20):
            src = self.babble[self.rng.integers(len(self.babble))]
            if len(src) > n:
                start = self.rng.integers(0, len(src) - n)
                return src[start:start + n].copy()
        return np.zeros(n, dtype=np.float32)

    def reverb(self, x):
        rt60 = self.rng.uniform(0.15, 0.7)
        n = int(rt60 * SR)
        t = np.arange(n) / SR
        rir = self.rng.standard_normal(n) * np.exp(-6.9 * t / rt60)
        rir[0] = 1.0
        from scipy.signal import fftconvolve
        y = fftconvolve(x, rir)[:len(x)]
        return (y / (np.max(np.abs(y)) + 1e-9) * np.max(np.abs(x))).astype(np.float32)

    def __call__(self, clip, at_end, clean=False):
        clip = np.asarray(clip, dtype=np.float32)
        if len(clip) > CLIP:
            start = len(clip) - CLIP if at_end else self.rng.integers(0, len(clip) - CLIP + 1)
            clip = clip[start:start + CLIP]
        out = np.zeros(CLIP, dtype=np.float32)
        if at_end:  # the phrase has just finished when the detector looks, as in live use
            tail = int(self.rng.uniform(0.0, 0.25) * SR)
            start = max(0, CLIP - tail - len(clip))
        else:
            start = self.rng.integers(0, CLIP - len(clip) + 1)
        if not clean and start > 0 and self.babble and self.rng.random() < 0.5:
            # Someone was already talking before the wake phrase: the detector must not rely on silence.
            lead = self._speech(start)
            out[:start] = lead * (np.max(np.abs(clip)) + 1e-9) / (np.max(np.abs(lead)) + 1e-9) * self.rng.uniform(0.3, 1.0)
        out[start:start + len(clip)] = clip[:CLIP - start]
        if clean:
            return out
        if self.rng.random() < 0.4:
            out = self.reverb(out)
        peak = np.max(np.abs(out)) + 1e-9
        out = out / peak * self.rng.uniform(0.05, 0.9)
        if self.rng.random() < 0.75:
            noise = self.noise(CLIP)
            snr = self.rng.uniform(0, 25)
            sig = np.sqrt(np.mean(out ** 2)) + 1e-9
            out = out + noise / (np.sqrt(np.mean(noise ** 2)) + 1e-9) * sig / (10 ** (snr / 20))
        return np.clip(out, -1, 1)


def windows(clips, hop):
    for clip in clips:
        for start in range(0, max(1, len(clip) - CLIP + 1), hop):
            window = clip[start:start + CLIP]
            yield np.pad(window, (0, CLIP - len(window))) if len(window) < CLIP else window  # recordings under 2 s


class Features:
    def __init__(self, wake_folder, device):
        if device == 'gpu':
            import onnxruntime as ort
            ort.preload_dlls()  # CUDA/cuDNN DLLs shipped with the installed torch
        from openwakeword.utils import AudioFeatures
        self.f = AudioFeatures(melspec_model_path=str(wake_folder / 'melspectrogram.onnx'),
                               embedding_model_path=str(wake_folder / 'embedding_model.onnx'), ncpu=4, device=device)

    def __call__(self, audio_list):
        out = []
        for i in range(0, len(audio_list), 1024):
            batch = np.stack([(np.clip(a, -1, 1) * 32767).astype(np.int16) for a in audio_list[i:i + 1024]])
            emb = self.f.embed_clips(batch, batch_size=256)
            out.append(emb[:, -FRAMES:, :].astype(np.float32))
            print(f'    features {i + len(batch)}/{len(audio_list)}', flush=True)
        return np.concatenate(out)

    def continuous(self, clips, stride=2):
        """Long recordings embedded in one pass, exactly like the live stream (no 2 s clip edges),
        cut into 16-frame windows every `stride` frames (80 ms each)."""
        out = []
        for clip in clips:
            pcm = (np.clip(clip, -1, 1) * 32767).astype(np.int16)
            emb = self.f._get_embeddings(pcm)
            for start in range(0, len(emb) - FRAMES + 1, stride):
                out.append(emb[start:start + FRAMES])
        return np.asarray(out, dtype=np.float32)


def build_model():
    import torch
    return torch.nn.Sequential(
        torch.nn.Flatten(),
        torch.nn.Linear(FRAMES * 96, 128), torch.nn.LayerNorm(128), torch.nn.ReLU(),
        torch.nn.Linear(128, 128), torch.nn.LayerNorm(128), torch.nn.ReLU(),
        torch.nn.Linear(128, 1), torch.nn.Sigmoid())


def train(pos, neg, epochs, seed, device, max_negative_weight=30.0):
    import torch
    torch.manual_seed(seed)
    dev = 'cuda' if device == 'gpu' and torch.cuda.is_available() else 'cpu'
    model = build_model().to(dev)
    # Half precision storage filled part by part (no full copy in RAM): hundreds of thousands of
    # windows fit in GPU memory; batches run in float32.
    parts = [pos] + list(neg)
    n_neg = sum(len(part) for part in neg)
    x = torch.empty((len(pos) + n_neg, FRAMES, 96), dtype=torch.float16, device=dev)
    at = 0
    for part in parts:
        for i in range(0, len(part), 50000):
            chunk = np.asarray(part[i:i + 50000], dtype=np.float16)
            x[at:at + len(chunk)] = torch.from_numpy(chunk).to(dev)
            at += len(chunk)
    y = torch.cat([torch.ones(len(pos)), torch.zeros(n_neg)]).to(dev)
    neg = range(n_neg)  # only its length is used below
    weight = torch.ones(len(y), device=dev)
    hard = torch.zeros(n_neg, device=dev)
    opt = torch.optim.Adam(model.parameters(), lr=1e-3, weight_decay=1e-5)
    for epoch in range(epochs):
        model.train()
        # Balanced sampling: as many positives as negatives each epoch.
        order = torch.cat([torch.randint(0, len(pos), (len(neg),)), len(pos) + torch.randperm(len(neg))])
        order = order[torch.randperm(len(order))].to(dev)
        total = 0.0
        for i in range(0, len(order), 512):
            idx = order[i:i + 512]
            p = model(x[idx].float()).squeeze(1)
            loss = (torch.nn.functional.binary_cross_entropy(p, y[idx], reduction='none') * weight[idx]).mean()
            opt.zero_grad()
            loss.backward()
            opt.step()
            total += float(loss.detach()) * len(idx)
        # False wakes cost more and more as training goes on (as openWakeWord trains its models).
        neg_weight = 1.0 + (max_negative_weight - 1.0) * min(1.0, epoch / max(1, epochs * 0.6))
        if epoch >= epochs // 4:
            # Hard negatives: anything the model still scores high gets extra weight.
            model.eval()
            with torch.no_grad():
                scores = torch.cat([model(x[i:i + 65536].float()).squeeze(1) for i in range(len(pos), len(x), 65536)])
            hard = (scores > 0.2).float()
        weight[len(pos):] = neg_weight * (1.0 + 9.0 * hard)
        print(f'  epoch {epoch + 1}/{epochs} loss {total / len(order):.4f}', flush=True)
    model.eval()
    return model.cpu()


def export(model, path):
    import torch
    torch.onnx.export(model, torch.zeros(1, FRAMES, 96), str(path), input_names=['x'], output_names=['score'],
                      opset_version=13, dynamic_axes=None, dynamo=False)


def stream_false_triggers(model_path, wake_folder, clips, threshold):
    """Run real speech through openWakeWord exactly like the app (80 ms frames)."""
    from openwakeword.model import Model
    oww = Model(wakeword_models=[str(model_path)], inference_framework='onnx',
                melspec_model_path=str(wake_folder / 'melspectrogram.onnx'),
                embedding_model_path=str(wake_folder / 'embedding_model.onnx'))
    triggers, seconds, scores = 0, 0.0, []
    for clip in clips:
        pcm = (np.clip(clip, -1, 1) * 32767).astype(np.int16)
        oww.reset()
        cooldown = 0
        for i in range(0, len(pcm) - 1280, 1280):
            score = max(oww.predict(pcm[i:i + 1280]).values())
            scores.append(score)
            if cooldown:
                cooldown -= 1
            elif score >= threshold:
                triggers += 1
                cooldown = 25  # the app ignores 2 s after a wake
        seconds += len(pcm) / SR
    return triggers, seconds / 3600, float(np.percentile(scores, 99.9))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--wake', type=Path, required=True, help='folder with melspectrogram.onnx / embedding_model.onnx')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--epochs', type=int, default=40)
    parser.add_argument('--threshold', type=float, default=0.5)
    parser.add_argument('--device', choices=['cpu', 'gpu'], default='cpu')
    parser.add_argument('--negative-features', type=Path, help='openWakeWord ACAV100M feature windows (.npy)')
    parser.add_argument('--negative-limit', type=int, default=300000)
    parser.add_argument('--validation-features', type=Path, help='openWakeWord validation_set_features.npy')
    parser.add_argument('--target-fph', type=float, default=0.5, help='false wakes per hour allowed when picking the threshold')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    import torch

    speech_train = load(args.data, 'speech_train')
    aug = Augment(speech_train, seed=1)
    feats = Features(args.wake, args.device)
    print('building training windows', flush=True)
    pos_train = [aug(c, at_end=True) for c in load(args.data, 'pos_train') for _ in range(3)]
    near_train = [aug(c, at_end=False) for c in load(args.data, 'near_train') for _ in range(2)]
    near_train += [aug(c, at_end=True) for c in load(args.data, 'near_train')]  # near misses ending right at the window edge
    noise = [aug(np.zeros(1, np.float32), at_end=False) for _ in range(2000)]
    pos_x = feats(pos_train)
    speech_x = feats.continuous(speech_train)
    parts = [feats(near_train + noise).astype(np.float16), speech_x.astype(np.float16)]
    if args.negative_features:
        acav = np.load(args.negative_features, mmap_mode='r')
        if args.negative_limit >= len(acav):
            parts.append(acav)  # all of it, memory-mapped: read chunk by chunk straight into GPU memory
        else:
            pick = np.sort(np.random.default_rng(3).choice(len(acav), args.negative_limit, replace=False))
            parts.append(acav[pick])
    neg_x = parts
    print(f'  {len(pos_x)} positive, {len(near_train)} near-miss, {len(speech_x)} real-speech, {len(parts[-1]) if args.negative_features else 0} library, {len(noise)} noise windows', flush=True)

    print('training', flush=True)
    model = train(pos_x, neg_x, args.epochs, seed=1, device=args.device)
    onnx_path = args.out / 'hey_sudo.onnx'
    export(model, onnx_path)

    print('evaluating on held-out voices and speech', flush=True)
    test_aug = Augment(load(args.data, 'speech_test'), seed=99)
    pos_test_clips = load(args.data, 'pos_test')
    pos_clean = feats([test_aug(c, at_end=True, clean=True) for c in pos_test_clips])
    pos_noisy = feats([test_aug(c, at_end=True) for c in pos_test_clips])
    near_test = feats([test_aug(c, at_end=True) for c in load(args.data, 'near_test')])
    score = lambda a: model(torch.from_numpy(a)).squeeze(1).detach().numpy()
    with torch.no_grad():
        s_clean, s_noisy, s_near = score(pos_clean), score(pos_noisy), score(near_test)
    threshold = args.threshold
    validation = {}
    if args.validation_features:
        frames = np.load(args.validation_features).astype(np.float32)
        if frames.ndim == 2:  # a continuous stream of frames: slide the 16-frame window like live use
            from numpy.lib.stride_tricks import sliding_window_view
            val = sliding_window_view(frames, (FRAMES, frames.shape[1]))[:, 0]
        else:
            val = frames
        with torch.no_grad():
            s_val = np.concatenate([score(np.ascontiguousarray(val[i:i + 32768])) for i in range(0, len(val), 32768)])
        val_hours = len(val) * 0.08 / 3600  # one window per 80 ms frame

        def wakes(scores, thr, cooldown=25):
            """Separate wake events, like the app: after a wake it ignores 2 s."""
            count, quiet = 0, 0
            for hit in scores >= thr:
                if quiet:
                    quiet -= 1
                elif hit:
                    count, quiet = count + 1, cooldown
            return count
        table = []
        threshold = None
        for candidate in [0.5, 0.6, 0.7, 0.8, 0.85, 0.9, 0.93, 0.95, 0.97, 0.98, 0.99, 0.995]:
            fph = wakes(s_val, candidate) / val_hours
            table.append({'threshold': candidate, 'validation_false_wakes_per_hour': round(fph, 2),
                          'recall_noisy': round(float((s_noisy >= candidate).mean()), 3)})
            if threshold is None and fph <= args.target_fph:
                threshold = candidate  # most sensitive setting within the false-wake budget
        threshold = threshold if threshold is not None else 0.995
        validation = {'validation_hours': round(val_hours, 2), 'threshold_table': table}
    triggers, hours, p999 = stream_false_triggers(onnx_path, args.wake, load(args.data, 'speech_test'), threshold)
    report = {
        'threshold': threshold, **validation,
        'recall_clean_heldout_voices': round(float((s_clean >= threshold).mean()), 3),
        'recall_noisy_heldout_voices': round(float((s_noisy >= threshold).mean()), 3),
        'near_miss_false_accept_rate': round(float((s_near >= threshold).mean()), 3),
        'real_speech_false_triggers': triggers, 'real_speech_hours': round(hours, 2),
        'false_triggers_per_hour': round(triggers / hours, 2) if hours else None,
        'real_speech_score_p99_9': round(p999, 3),
        'training_windows': {'positive': len(pos_x), 'negative': sum(len(part) for part in neg_x)},
    }
    (args.out / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
