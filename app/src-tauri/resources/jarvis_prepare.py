"""Explicit Sudo AI setup only: downloads pinned speech models and proves CUDA ASR loads."""
import argparse
from pathlib import Path
import urllib.request

WHISPER = ('mobiuslabsgmbh/faster-whisper-large-v3-turbo', '0a363e9161cbc7ed1431c9597a8ceaf0c4f78fcf')
TINY = ('Systran/faster-whisper-tiny', 'd90ca5fe260221311c53c58e660288d3deb8d356')  # "Hey Sudo" step 2
SUPERTONIC = ('Supertone/supertonic-3', '724fb5abbf5502583fb520898d45929e62f02c0b')
FILES = {
    # name: (url, sha256)
    'wake/melspectrogram.onnx': ('https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/melspectrogram.onnx', None),
    'wake/embedding_model.onnx': ('https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/embedding_model.onnx', None),
    'silero_vad.onnx': ('https://raw.githubusercontent.com/snakers4/silero-vad/v6.2/src/silero_vad/data/silero_vad.onnx', None),
}
SIZES = {'wake/melspectrogram.onnx': 1087958,
         'wake/embedding_model.onnx': 1326578, 'silero_vad.onnx': 2327524}


def fetch(models, name, url):
    target = models / name
    if target.is_file() and target.stat().st_size == SIZES[name]:
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=120) as response:
        body = response.read()
    if len(body) != SIZES[name]:
        raise RuntimeError(f'{name} has an unexpected size; retry setup.')
    target.write_bytes(body)


SPEECH_BYTES = 2_105_000_000  # Whisper turbo (1.62 GB) + Supertonic 3 (0.40 GB) + Whisper tiny (75 MB)


def folder_bytes(path):
    return sum(f.stat().st_size for f in path.rglob('*') if f.is_file()) if path.exists() else 0


def report_progress(models, stop):
    """Status lines for the Settings progress bar while Hugging Face downloads run."""
    import json
    import time
    while not stop.wait(1.0):
        done = sum(folder_bytes(models / name) for name in ('whisper-large-v3-turbo', 'supertonic-3', 'whisper-tiny'))
        print(json.dumps({'phase': 'downloading', 'component': 'all', 'message': 'Downloading speech recognition and voice models',
                          'step': 4, 'steps': 4, 'done': min(done, SPEECH_BYTES), 'total': SPEECH_BYTES}), flush=True)


def prepare(data):
    import threading
    from huggingface_hub import snapshot_download
    models = data / 'jarvis-models'
    stop = threading.Event()
    threading.Thread(target=report_progress, args=(models, stop), daemon=True).start()
    snapshot_download(WHISPER[0], revision=WHISPER[1], local_dir=models / 'whisper-large-v3-turbo')
    snapshot_download(TINY[0], revision=TINY[1], local_dir=models / 'whisper-tiny')
    snapshot_download(SUPERTONIC[0], revision=SUPERTONIC[1], local_dir=models / 'supertonic-3',
                      allow_patterns=['onnx/*', 'voice_styles/*', 'config.json', 'LICENSE'])
    for name, (url, _) in FILES.items():
        fetch(models, name, url)
    stop.set()
    # Prove the GPU path really loads (DLLs + model), not just that packages exist.
    import jarvis_voice
    jarvis_voice.add_cuda_dlls()
    import ctranslate2
    if ctranslate2.get_cuda_device_count() < 1:
        raise RuntimeError('Speech recognition cannot use the NVIDIA GPU. Update the graphics driver and retry.')
    from faster_whisper import WhisperModel
    WhisperModel(str(models / 'whisper-large-v3-turbo'), device='cuda', compute_type='int8_float16')


if __name__ == '__main__':
    import sys
    sys.path.insert(0, str(Path(__file__).parent))
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', required=True, type=Path)
    prepare(parser.parse_args().data)
