"""Opt-in, local Spotify vocal attenuation. No microphone, recording, or network server.

Dependencies/model are optional and live outside the normal app. JSON lines over inherited
stdio carry control/status; raw audio only exists in bounded memory queues.
"""
import argparse
import contextlib
import json
import logging
import os
from pathlib import Path
import queue
import sys
import threading
import time

SR = 44100
BLOCK = 1024              # audio callback size (~23 ms)
HOP = int(SR * .75)       # fresh audio separated per step (0.75 s; longer on slower GPUs, see benchmark)
LEFT = SR                 # past context given to the model (1.0 s)
RIGHT = int(SR * .3)      # future context (0.3 s); also the crossfade source
XFADE = int(SR * .15)     # complementary (sum-to-one) crossfade between consecutive windows
MARGIN = int(SR * .3)     # extra buffered audio so a slow inference never stutters
WINDOW = LEFT + HOP + RIGHT
# Each step also re-processes LEFT + RIGHT context, so longer steps cost far less per second of
# music: about 0.36x, 0.23x and 0.14x of real time on an RTX desktop GPU. Slower (laptop) GPUs
# get a longer step and a little more delay instead of no vocal control at all.
STEP_CHOICES = (.75, 1.5, 2.5)


def set_step(seconds):
    global HOP, WINDOW
    HOP = int(SR * seconds)
    WINDOW = LEFT + HOP + RIGHT
SILENCE = int(SR * .35)   # digital silence this long = Spotify paused/skipped (beat drops are shorter)
QUIET = 1e-5
BASS_FROM, BASS_TO = 90.0, 160.0  # below this band the original song is used untouched
MODEL = 'UVR-MDX-NET-Inst_HQ_3.onnx'
CONTROL = sys.stdout


def emit(phase, **values):
    try:
        print(json.dumps({'phase': phase, **values}), file=CONTROL, flush=True)
    except (OSError, ValueError):
        pass  # The app closed its status pipe; run() still restores routing.


def device_list():
    import sounddevice as sd
    devices = []
    for index, device in enumerate(sd.query_devices()):
        api = sd.query_hostapis(device['hostapi'])['name']
        if api != 'Windows WASAPI':
            continue
        name = device['name']
        if device['max_input_channels'] >= 2 and 'CABLE' in name.upper() and 'OUTPUT' in name.upper():
            devices.append({'id': index, 'name': name, 'kind': 'input'})
        if device['max_output_channels'] >= 2 and 'CABLE' not in name.upper():
            devices.append({'id': index, 'name': name, 'kind': 'output'})
    return devices


def default_output(devices):
    from pycaw.pycaw import AudioUtilities
    name = AudioUtilities.GetSpeakers().FriendlyName
    return next((d['id'] for d in devices if d['kind'] == 'output' and d['name'] == name), -1)


def routing_lock(data):
    from filelock import FileLock
    folder = Path(data)
    folder.mkdir(parents=True, exist_ok=True)
    return FileLock(str(folder / 'vocal-routing.lock'), timeout=0)


def load_model(data):
    os.environ['PATH'] = str(data / 'vocal-bin') + os.pathsep + os.environ['PATH']
    os.environ.setdefault('CUDA_CACHE_PATH', str(data / 'cuda-cache'))
    import torch
    import onnxruntime as ort
    if not torch.cuda.is_available():
        raise RuntimeError('Vocal control needs an NVIDIA GPU with a recent driver (CUDA 12.6 or newer). '
                           'Update the NVIDIA driver from nvidia.com or the NVIDIA app, then try again.')
    major, minor = torch.cuda.get_device_capability(0)
    built = [a for a in torch.cuda.get_arch_list() if a.startswith('sm_')]
    if not any(int(a[3:]) // 10 == major and int(a[3:]) % 10 <= minor for a in built if a[3:].isdigit()):
        raise RuntimeError(f'The installed vocal runtime was built for older GPUs and cannot use the {torch.cuda.get_device_name(0)}. '
                           'Press Repair runtime in Settings → Spotify → Vocal reduction to install the right one.')
    try:
        ort.preload_dlls()  # only the ONNX path needs them; the PyTorch path runs without
    except Exception:  # noqa: BLE001
        pass
    from audio_separator.separator import Separator
    from audio_separator.separator.architectures import mdx_separator
    mdx_separator.tqdm = lambda iterable, **kwargs: iterable
    separator = Separator(log_level=logging.ERROR, model_file_dir=str(data / 'vocal-models'),
                          output_dir=str(data / 'tmp'), mdx_params={'segment_size': 64,
                          'hop_length': 1024, 'overlap': .25, 'batch_size': 1, 'enable_denoise': False})
    separator.load_model(MODEL)
    model = separator.model_instance
    if model.primary_stem_name.lower() != 'instrumental':
        raise RuntimeError('The installed model is not an instrumental separator.')
    return model, torch.cuda.get_device_name(0)


def step_time(model):
    """Slowest of three separations of one window at the current step size."""
    import numpy as np
    import torch
    rng = np.random.default_rng(1)
    source = (rng.standard_normal((2, WINDOW)) * .05).astype(np.float32)
    model.demix(source)
    timings = []
    for _ in range(3):
        start = time.perf_counter()
        result = model.demix(source)
        torch.cuda.synchronize()
        timings.append(time.perf_counter() - start)
        if result.shape != source.shape or not np.isfinite(result).all():
            raise RuntimeError('Vocal model returned invalid audio.')
    return max(timings)


def benchmark(model):
    """Picks the shortest step this GPU separates comfortably in time (under 70% of the step)."""
    for seconds in STEP_CHOICES:
        set_step(seconds)
        duration = step_time(model)
        if duration <= HOP / SR * .7:
            return duration
    raise RuntimeError(f'This GPU is too slow for live vocal control (it needs {duration:.1f} s for every {seconds} s of music). '
                       'Close games or other GPU-heavy apps and try again. Normal Spotify output is unchanged.')


def control_lines(stop):
    """Yield command lines from the app without a blocking read.

    On Windows a thread parked in ReadFile on the stdin pipe stalls DLL loading in
    every other thread (torch/CUDA imports hung until a command arrived). Peek the
    pipe and only read once a whole line is waiting.
    """
    if os.name != 'nt':
        yield from sys.stdin
        return
    import ctypes
    import msvcrt
    from ctypes import wintypes
    kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
    peek = kernel32.PeekNamedPipe
    peek.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.c_void_p,
                     ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
    peek.restype = wintypes.BOOL
    handle = msvcrt.get_osfhandle(sys.stdin.fileno())
    fd = sys.stdin.fileno()
    buffer = b''
    available = wintypes.DWORD()
    while not stop.is_set():
        if not peek(handle, None, 0, None, ctypes.byref(available), None):
            return  # Pipe closed: the app exited.
        if available.value:
            chunk = os.read(fd, available.value)
            if not chunk:
                return
            buffer += chunk
            while b'\n' in buffer:
                line, buffer = buffer.split(b'\n', 1)
                yield line.decode('utf-8', 'replace')
        else:
            time.sleep(.05)


class Pipeline:
    """Streaming separation with overlapping windows.

    Every HOP samples the model sees LEFT past + HOP fresh + RIGHT future samples.
    Only the fresh part is played; its first XFADE samples are crossfaded with the
    previous window's estimate of the same audio, so there are no seams. The
    original signal is never altered, so 100 % equals Spotify's own output.

    Capture and playback run as separate streams. Every captured block is time
    stamped, so the reported delay is "heard now" minus "produced by Spotify",
    including the drift between the cable's and the speakers' clocks; a one-sample
    skip/repeat per block holds that delay steady so lyrics never slip.
    """

    def __init__(self, model, volume):
        import collections
        import numpy as np
        self.np = np
        self.model = model
        self.volume = volume
        self.gain = volume
        self.paused = False        # Jarvis is thinking: play audio unprocessed, leave the GPU free
        self.stop = threading.Event()
        self.incoming = queue.Queue(maxsize=4096)
        self.lock = threading.Lock()
        self.fifo = collections.deque()  # [inst, orig, input index, capture time] ready to play
        self.fifo_offset = 0
        self.fifo_samples = 0
        self.received = 0
        self.started = False
        self.error = None
        self.underruns = 0
        self.in_latency = 0.0
        self.out_latency = 0.0
        self.latency = None        # seconds of delay you hear (0 until rerouted audio is audible)
        self.signal_at = None      # input index where Spotify's audio first reached the cable
        self.target = None         # delay held steady by drift correction
        self.settle = []
        self.adjust = 0
        self.silent_run = 0
        self.mute_before = -1      # buffered audio captured before a pause is dropped
        self.mute_gain = 1.0
        self.limit = 1.0        # smooth safety gain instead of hard clipping
        self.highpass = None    # crossover that keeps the original bass
        # Both windows estimate the SAME audio, so the fades must sum to exactly 1
        # (raised cosine). An equal-power sin/cos fade summed to 1.41 mid-fade and made
        # the bass swell by up to +3 dB every window.
        fade = np.linspace(0, np.pi / 2, XFADE, dtype=np.float32)[:, None]
        self.fade_in, self.fade_out = np.sin(fade) ** 2, np.cos(fade) ** 2

    def fail(self, message):
        if not self.error:
            self.error = message
        self.stop.set()

    def input_callback(self, indata, frames, timing, status):
        np = self.np
        if self.stop.is_set():
            return
        try:
            captured = time.monotonic() - self.in_latency - frames / SR
            if float(np.abs(indata).max()) <= QUIET:
                self.silent_run += frames
                if self.silent_run >= SILENCE:
                    # Paused or skipped: never play the music buffered before it.
                    self.mute_before = max(self.mute_before, self.received + frames - self.silent_run)
            else:
                self.silent_run = 0
                if self.signal_at is None:
                    self.signal_at = self.received
            self.incoming.put_nowait((indata.copy(), self.received, captured))
            self.received += frames
        except queue.Full:
            self.fail('Audio processing fell behind. Spotify output is being restored.')
        except Exception as failure:
            self.fail(str(failure))

    def take(self, frames):
        """Pop `frames` samples (caller holds the lock); also the first sample's index and capture time."""
        np = self.np
        inst = np.empty((frames, 2), dtype=np.float32)
        orig = np.empty((frames, 2), dtype=np.float32)
        head = self.fifo[0]
        first_index = head[2] + self.fifo_offset
        first_time = head[3] + self.fifo_offset / SR
        filled = 0
        while filled < frames:
            chunk_inst, chunk_orig = self.fifo[0][0], self.fifo[0][1]
            count = min(frames - filled, len(chunk_inst) - self.fifo_offset)
            inst[filled:filled + count] = chunk_inst[self.fifo_offset:self.fifo_offset + count]
            orig[filled:filled + count] = chunk_orig[self.fifo_offset:self.fifo_offset + count]
            filled += count
            self.fifo_offset += count
            if self.fifo_offset >= len(chunk_inst):
                self.fifo.popleft()
                self.fifo_offset = 0
        self.fifo_samples -= frames
        return inst, orig, first_index, first_time

    def output_callback(self, outdata, frames, timing, status):
        np = self.np
        if self.stop.is_set():
            outdata.fill(0)
            return
        try:
            heard = time.monotonic() + self.out_latency
            # Drift correction: skip or repeat one sample in this block.
            need = frames + self.adjust
            with self.lock:
                ready = self.fifo_samples >= need + (0 if self.started else MARGIN)
                if not ready:
                    outdata.fill(0)
                    if self.started:
                        self.underruns += 1
                        if self.underruns > 80:
                            self.fail('Processing could not keep up. Spotify output is being restored.')
                    return
                self.started = True
                self.underruns = 0
                inst, orig, first_index, first_time = self.take(need)
            if need != frames:
                # Stretch the block by one sample with linear interpolation (a 0.1 % speed
                # change): no discontinuity, unlike dropping or repeating a sample.
                source = np.linspace(0, need - 1, frames)
                inst = np.stack([np.interp(source, np.arange(need), inst[:, c]) for c in range(2)], axis=1).astype(np.float32)
                orig = np.stack([np.interp(source, np.arange(need), orig[:, c]) for c in range(2)], axis=1).astype(np.float32)
            self.measure(heard - first_time, first_index, frames)
            next_gain = self.gain + (self.volume - self.gain) * min(1.0, frames / (SR * .06))
            gains = np.linspace(self.gain, next_gain, frames, dtype=np.float32)[:, None]
            self.gain = next_gain
            np.multiply(orig - inst, gains, out=outdata)
            outdata += inst
            mute = 0.0 if first_index + frames <= self.mute_before else 1.0
            if mute != 1.0 or self.mute_gain != 1.0:
                ramp = np.linspace(self.mute_gain, mute, frames, dtype=np.float32)[:, None]
                outdata *= ramp
                self.mute_gain = mute
            peak = float(np.abs(outdata).max())
            wanted = min(1.0, .99 / peak) if peak > 0 else 1.0
            # Fast attack, ~0.5 s release; the ramp avoids a gain step inside the block.
            target = min(wanted, 1.0, self.limit + frames / (SR * .5))
            if target != 1.0 or self.limit != 1.0:
                outdata *= np.linspace(self.limit, target, frames, dtype=np.float32)[:, None]
                self.limit = target
            np.clip(outdata, -1, 1, out=outdata)
        except Exception as failure:
            outdata.fill(0)
            self.fail(str(failure))

    def flush(self):
        """Drop audio captured before now (pause/skip/seek pressed in Sudo)."""
        self.mute_before = max(self.mute_before, self.received)

    def measure(self, delay, first_index, frames):
        if self.signal_at is None or first_index + frames <= self.signal_at:
            if self.latency is None:
                self.latency = 0.0  # Spotify is still heard directly; do not delay lyrics yet.
            return
        self.latency = delay if not self.latency else self.latency * .9 + delay * .1
        if self.target is None:
            self.settle.append(delay)
            if len(self.settle) >= int(2 * SR / BLOCK):
                self.target = float(self.np.median(self.settle))
            return
        error = self.latency - self.target
        self.adjust = 1 if error > .025 else -1 if error < -.025 else (0 if abs(error) < .008 else self.adjust)

    def separate(self, window):
        np = self.np
        estimate = self.model.demix(np.ascontiguousarray(window.T))
        if estimate.shape != window.T.shape or not np.isfinite(estimate).all():
            raise RuntimeError('Vocal processor produced invalid audio.')
        return np.ascontiguousarray(estimate.T, dtype=np.float32)

    def keep_bass(self, window, estimate):
        """Take the bass straight from the original: vocals have almost nothing below
        ~120 Hz, but the model's bass varies window to window, which made 808s and
        kicks sound uneven or pumping when vocals were lowered."""
        np = self.np
        if self.highpass is None or len(self.highpass) != len(window) // 2 + 1:
            freqs = np.fft.rfftfreq(len(window), 1 / SR)
            ramp = np.clip((freqs - BASS_FROM) / (BASS_TO - BASS_FROM), 0, 1)
            self.highpass = ((1 - np.cos(np.pi * ramp)) / 2).astype(np.float32)[:, None]
        vocals = np.fft.irfft(np.fft.rfft(window - estimate, axis=0) * self.highpass, len(window), axis=0)
        return (window - vocals).astype(np.float32)

    def worker(self):
        import collections
        np = self.np
        history = np.zeros((LEFT, 2), dtype=np.float32)   # audio before the current hop
        pending = np.zeros((0, 2), dtype=np.float32)      # fresh + future audio not yet separated
        position = 0                                      # input index of pending[0]
        stamps = collections.deque()                      # (input index, capture time) per block
        previous_tail = None
        try:
            while not self.stop.is_set():
                parts = [pending]
                have = len(pending)
                while have < HOP + RIGHT and not self.stop.is_set():
                    try:
                        block, index, captured = self.incoming.get(timeout=.2)
                    except queue.Empty:
                        continue
                    parts.append(block)
                    stamps.append((index, captured))
                    have += len(block)
                if self.stop.is_set():
                    return
                pending = np.concatenate(parts, axis=0)
                while len(stamps) > 1 and stamps[1][0] <= position:
                    stamps.popleft()
                base_index, base_time = stamps[0]
                chunk_time = base_time + (position - base_index) / SR
                window = np.concatenate((history, pending[:HOP + RIGHT]), axis=0)
                if self.paused:
                    estimate = window.copy()   # Jarvis is thinking: play unprocessed, leave the GPU free
                    previous_tail = None
                else:
                    estimate = self.keep_bass(window, self.separate(window))
                inst = estimate[LEFT:LEFT + HOP].copy()
                if previous_tail is not None:
                    inst[:XFADE] = previous_tail * self.fade_out + inst[:XFADE] * self.fade_in
                previous_tail = estimate[LEFT + HOP:LEFT + HOP + XFADE].copy()
                orig = window[LEFT:LEFT + HOP].copy()
                with self.lock:
                    self.fifo.append((inst, orig, position, chunk_time))
                    self.fifo_samples += HOP
                history = window[HOP:HOP + LEFT].copy()
                pending = pending[HOP:]
                position += HOP
        except Exception as failure:
            self.fail(str(failure))

    def controls(self):
        try:
            for raw in control_lines(self.stop):
                try:
                    command = json.loads(raw)
                    if command.get('command') == 'stop':
                        break
                    if command.get('command') == 'flush':
                        self.flush()
                    if command.get('command') == 'pause':
                        self.paused = bool(command['value'])
                    if command.get('command') == 'volume':
                        self.volume = max(0, min(1, float(command['value']) / 100))
                except (ValueError, KeyError, TypeError, AttributeError):
                    continue
        finally:
            # Parent app closed/crashed, so its stdin pipe closed: restore routing in main finally.
            self.stop.set()


def run(args):
    import sounddevice as sd
    from vocal_route import Routes, cable_endpoint, restore
    data = Path(args.data)
    backup = data / 'vocal-routing-backup.json'
    restore(backup)
    choices = device_list()
    if not any(d['id'] == args.input and d['kind'] == 'input' for d in choices):
        raise RuntimeError('Choose the installed CABLE Output input. Microphones are not accepted.')
    if not any(d['id'] == args.output and d['kind'] == 'output' for d in choices):
        raise RuntimeError('Choose physical speakers/headphones, not the cable output.')
    endpoint = cable_endpoint()
    pipeline = Pipeline(None, args.volume / 100)
    threading.Thread(target=pipeline.controls, daemon=True).start()
    emit('loading', message='Loading the optional local model…')
    model, gpu = load_model(data)
    if pipeline.stop.is_set():
        emit('off', message='Optional vocal processing stopped before routing.')
        return
    emit('benchmarking', message='Checking processing speed before changing Spotify output…')
    seconds = benchmark(model)
    pipeline.model = model
    if pipeline.stop.is_set():
        emit('off', message='Optional vocal processing stopped before routing.')
        return
    routes = Routes(backup, endpoint)
    try:
        settings = dict(channels=2, dtype='float32', samplerate=SR, blocksize=BLOCK, latency='low',
                        extra_settings=sd.WasapiSettings(auto_convert=True))
        with sd.InputStream(device=args.input, callback=pipeline.input_callback, **settings) as capture, \
                sd.OutputStream(device=args.output, callback=pipeline.output_callback, **settings) as playback:
            pipeline.in_latency, pipeline.out_latency = capture.latency, playback.latency
            worker = threading.Thread(target=pipeline.worker, daemon=True)
            worker.start()
            if not pipeline.stop.is_set():
                routes.update()
            emit('running', message='Local vocal reduction is active.', latency=0, inference=round(seconds, 3), gpu=gpu)
            next_route = 0
            reported = 0.0
            while not pipeline.stop.wait(.1):
                if not capture.active or not playback.active:
                    raise RuntimeError('Audio device disconnected.')
                if time.monotonic() >= next_route:
                    routes.update()
                    next_route = time.monotonic() + 2
                # Report the measured delay so lyrics stay aligned with what you hear.
                if pipeline.latency is not None and abs(pipeline.latency - reported) > .02:
                    reported = pipeline.latency
                    emit('running', message='Local vocal reduction is active.', latency=round(reported, 3), inference=round(seconds, 3), gpu=gpu)
            if pipeline.error:
                raise RuntimeError(pipeline.error)
    finally:
        pipeline.stop.set()
        routes.close()
    emit('off', message='Normal Spotify output restored.')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['devices', 'benchmark', 'run', 'restore'])
    parser.add_argument('--data', required=True)
    parser.add_argument('--input', type=int)
    parser.add_argument('--output', type=int)
    parser.add_argument('--volume', type=int, default=30)
    args = parser.parse_args()
    # Model libraries may print progress to stdout; keep our control pipe valid JSON only.
    try:
        with contextlib.redirect_stdout(sys.stderr):
            if args.command == 'devices':
                devices = device_list()
                from pycaw.pycaw import AudioUtilities
                default_is_cable = 'CABLE' in AudioUtilities.GetSpeakers().FriendlyName.upper()
                emit('devices', devices=devices, defaultOutput=default_output(devices), defaultIsCable=default_is_cable)
            elif args.command == 'restore':
                from vocal_route import restore
                with routing_lock(args.data):
                    restore(Path(args.data) / 'vocal-routing-backup.json')
                emit('off', message='Normal Spotify output restored.')
            elif args.command == 'benchmark':
                emit('loading', message='Loading optional local model…')
                model, gpu = load_model(Path(args.data))
                seconds = benchmark(model)
                emit('ready', gpu=gpu, inference=round(seconds, 3), step=round(HOP / SR, 2), latency=round((HOP + RIGHT + MARGIN) / SR + seconds, 2))
            else:
                with routing_lock(args.data):
                    run(args)
    except Exception as failure:
        emit('error', message=str(failure))
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
