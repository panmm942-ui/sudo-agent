"""Explicit optional setup only; never imported by the normal app or audio engine."""
import argparse
import filecmp
import hashlib
import logging
import os
from pathlib import Path
import shutil
import time

MODEL = 'UVR-MDX-NET-Inst_HQ_3.onnx'
# UVR's catalogue identifies models by the MD5 of their last 10,000 KiB (mdx_model_data.json);
# the SHA-256 pins the whole file.
MODEL_MD5 = '55657dd70583b0fedfba5f67df11d711'
MODEL_SHA256 = '317554b07fe1ea5279a77f2b1520a41ea4b93432560c4ffd08792c30fddf9adc'


def prepare(data):
    import torch
    import onnxruntime as ort
    import sounddevice
    import pycaw
    import imageio_ffmpeg
    if not torch.cuda.is_available():
        raise RuntimeError('The optional CUDA runtime cannot use this NVIDIA GPU. Update its graphics driver before retrying.')
    # The separator runs the model in PyTorch, so check that PyTorch really runs on this GPU
    # (a build without this GPU's architecture fails here, not later during playback).
    try:
        probe = torch.nn.functional.conv1d(torch.ones(1, 2, 64, device='cuda'), torch.ones(4, 2, 3, device='cuda'))
        torch.cuda.synchronize()
        if not torch.isfinite(probe).all():
            raise RuntimeError('invalid result')
    except Exception as error:
        raise RuntimeError(f'PyTorch cannot run on this GPU ({torch.cuda.get_device_name(0)}): {error}. Update the graphics driver and retry.') from error
    binary_folder = data / 'vocal-bin'
    binary_folder.mkdir(parents=True, exist_ok=True)
    # Repairs re-run this: an identical ffmpeg.exe is kept (it may be in use or being scanned).
    source, target = Path(imageio_ffmpeg.get_ffmpeg_exe()), binary_folder / 'ffmpeg.exe'
    if not (target.is_file() and target.stat().st_size == source.stat().st_size and filecmp.cmp(source, target, shallow=False)):
        for attempt in range(5):
            try:
                shutil.copy2(source, target)
                break
            except PermissionError:
                if attempt == 4:
                    raise RuntimeError('ffmpeg.exe in the Sudo data folder is in use. Turn vocal reduction off and retry.')
                time.sleep(1)
    os.environ['PATH'] = str(binary_folder) + os.pathsep + os.environ['PATH']
    from audio_separator.separator import Separator
    separator = Separator(log_level=logging.ERROR, model_file_dir=str(data / 'vocal-models'), output_dir=str(data / 'tmp'))
    model_path = data / 'vocal-models' / MODEL
    # Catalogue download also includes the model's accompanying metadata.
    separator.download_model_and_data(MODEL)
    with model_path.open('rb') as model:
        whole = hashlib.file_digest(model, 'sha256').hexdigest()
        model.seek(-10000 * 1024, os.SEEK_END)
        catalogue = hashlib.md5(model.read()).hexdigest()
    if whole != MODEL_SHA256 or catalogue != MODEL_MD5:
        raise RuntimeError('The instrumental model failed its published integrity check. Remove the model file and retry setup.')
    # The model file itself must load (ONNX Runtime on the CPU is enough to check that).
    ort.InferenceSession(str(model_path), providers=['CPUExecutionProvider'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', required=True, type=Path)
    prepare(parser.parse_args().data)
