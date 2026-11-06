"""Optional local Sudo AI voice: microphone -> push-to-talk / "hey jarvis" -> Silero VAD ->
faster-whisper (Greek/English, never translated) and Supertonic 3 speech out with barge-in.

Runs only after explicit setup; models live under the Sudo data folder. JSON lines on stdio
carry control/status. Microphone audio exists only in bounded memory while listening.
"""
import argparse
import collections
import contextlib
import json
import os
from pathlib import Path
import queue
import sys
import threading
import time

SR = 16000
FRAME = 512                 # Silero VAD frame at 16 kHz (32 ms)
WAKE_FRAME = 1280           # openWakeWord frame (80 ms)
PREROLL = int(SR * .4)      # audio kept before a press / wake so the first syllable survives
END_SILENCE = .7            # wake mode: this much silence after speech ends the turn
NO_SPEECH_TIMEOUT = 5.0     # wake mode: give up if nothing is said after the wake word
MAX_UTTERANCE = 29.0          # Whisper sees one 30 s window
SPEECH_PROB = .5
WAKE_THRESHOLD = .9         # step 1 ("Hey Sudo" detector); step 2 (tiny Whisper) removes false wakes
VERIFY_SECONDS = 1.5        # audio before the detection that step 2 listens to
LANGS = ('el', 'en')
# Greek must survive the pipe: -I ignores PYTHONIOENCODING and Windows pipes default to cp1252.
sys.stdout.reconfigure(encoding='utf-8')
sys.stderr.reconfigure(encoding='utf-8', errors='replace')
CONTROL = sys.stdout


EMIT_LOCK = threading.Lock()


def emit(event, **values):
    """One whole JSON line per event: several threads report (mic, voice levels, recognition), and
    unsynchronised prints can interleave two events on one line."""
    line = json.dumps({'event': event, **values}, ensure_ascii=False) + '\n'
    try:
        with EMIT_LOCK:
            CONTROL.write(line)
            CONTROL.flush()
    except (OSError, ValueError):
        pass


def add_cuda_dlls():
    """cuBLAS/cuDNN come from pip wheels; expose their bin folders before importing ctranslate2."""
    base = Path(sys.prefix) / 'Lib' / 'site-packages' / 'nvidia'
    for folder in sorted(base.glob('*/bin')):
        os.add_dll_directory(str(folder))
        os.environ['PATH'] = str(folder) + os.pathsep + os.environ['PATH']


def control_lines(stop):
    """Command lines from the app without a blocking stdin read (see vocal_engine.py)."""
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
            return
        if available.value:
            chunk = os.read(fd, available.value)
            if not chunk:
                return
            buffer += chunk
            while b'\n' in buffer:
                line, buffer = buffer.split(b'\n', 1)
                yield line.decode('utf-8', 'replace')
        else:
            time.sleep(.01)


def microphones():
    """Input devices by full name (WASAPI lists untruncated names), default first."""
    import sounddevice as sd
    names = []
    for device in sd.query_devices():
        if device['max_input_channels'] > 0 and sd.query_hostapis(device['hostapi'])['name'] == 'Windows WASAPI':
            if device['name'] not in names:
                names.append(device['name'])
    return names


def find_microphone(wanted):
    """Index of the chosen microphone on the MME host API, which resamples to 16 kHz for us
    (WASAPI shared mode only opens at the device rate). MME truncates names to 31 characters."""
    if not wanted:
        return None
    import sounddevice as sd
    wanted = wanted.strip().lower()
    candidates = [(i, d) for i, d in enumerate(sd.query_devices()) if d['max_input_channels'] > 0]
    for i, device in candidates:
        name = device['name'].strip().lower()
        if sd.query_hostapis(device['hostapi'])['name'] == 'MME' and name and wanted.startswith(name):
            return i
    for i, device in candidates:
        if wanted in device['name'].lower():
            return i
    raise RuntimeError(f'The chosen microphone is not connected: {wanted}. Pick another one in Settings → Sudo AI.')


class Meter:
    """Orb features from short audio blocks: loudness, brightness, pitch, syllable onsets."""

    def __init__(self, rate):
        self.rate = rate
        self.slow = 0.0

    def __call__(self, block):
        import numpy as np
        x = np.asarray(block, dtype=np.float32).reshape(-1)
        if len(x) < 64:
            return None
        rms = float(np.sqrt(np.mean(x * x)) + 1e-9)
        level = min(1.0, max(0.0, (20 * np.log10(rms) + 52) / 40))  # -52 dBFS silent .. -12 dBFS loud
        spectrum = np.abs(np.fft.rfft(x * np.hanning(len(x))))
        freqs = np.fft.rfftfreq(len(x), 1 / self.rate)
        total = float(spectrum.sum()) + 1e-9
        centroid = min(1.0, float((spectrum * freqs).sum() / total) / 4000.0)
        pitch = 0.0
        if level > 0.2:
            lo, hi = int(self.rate / 320), min(int(self.rate / 90), len(x) - 1)
            ac = np.correlate(x, x, 'full')[len(x) - 1:]
            if ac[0] > 0 and hi > lo:
                lag = lo + int(np.argmax(ac[lo:hi]))
                if ac[lag] / ac[0] > 0.45:
                    pitch = self.rate / lag
        onset = level - self.slow > 0.18 and level > 0.3
        self.slow += (level - self.slow) * 0.25
        return {'rms': round(float(level), 3), 'centroid': round(centroid, 3), 'pitchHz': round(pitch, 1), 'onset': bool(onset)}


def pick_language(detected, probabilities):
    """Sudo AI speaks Greek or English: map Whisper's guess onto the more likely of the two."""
    if detected in LANGS:
        return detected
    scores = dict(probabilities or [])
    return max(LANGS, key=lambda lang: scores.get(lang, 0.0))


class Vad:
    """Stateful Silero VAD (ONNX, CPU) for 512-sample frames at 16 kHz."""

    def __init__(self, path):
        import numpy as np
        import onnxruntime as ort
        self.np = np
        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        self.session = ort.InferenceSession(str(path), sess_options=options, providers=['CPUExecutionProvider'])
        self.sr = np.array(SR, dtype=np.int64)
        self.reset()

    def reset(self):
        self.state = self.np.zeros((2, 1, 128), dtype=self.np.float32)
        self.context = self.np.zeros((1, 64), dtype=self.np.float32)

    def __call__(self, frame):
        x = self.np.concatenate([self.context, frame.reshape(1, -1)], axis=1)
        out, self.state = self.session.run(None, {'input': x, 'state': self.state, 'sr': self.sr})
        self.context = x[:, -64:]
        return float(out[0][0])


# Words Whisper should expect. Given as "previous text", they keep names in their real spelling
# (Latin letters inside Greek sentences: "άνοιξε το Spotify", not "Σποτιφάι") and fix rare words.
VOCABULARY = ['Sudo', 'Sudo AI', 'Spotify', 'YouTube', 'Chrome', 'Discord', 'Viber', 'WhatsApp', 'Windows',
              'Google', 'Netflix', 'Steam', 'Word', 'Excel', 'PowerPoint', 'Gmail', 'Instagram', 'TikTok',
              'Bluetooth', 'Wi-Fi', 'PDF', 'email', 'Outlook', 'Telegram', 'VS Code']


class WakeVerifier:
    """Second wake step: tiny Whisper on the CPU (~75 MB) listens to the last 1.5 s before the
    detector fired and accepts only if it hears "Sudo" (English or Greek spelling). TV, music and
    look-alike words rarely produce that, so false wakes almost disappear and the GPU stays free."""

    TARGETS = ('sudo', 'σουντο', 'σουδο')

    def __init__(self, folder):
        from faster_whisper import WhisperModel
        self.model = WhisperModel(str(folder), device='cpu', compute_type='int8', cpu_threads=2)

    @staticmethod
    def normalise(text):
        import re
        import unicodedata
        text = unicodedata.normalize('NFD', text.lower())
        text = ''.join(c for c in text if unicodedata.category(c) != 'Mn')
        return re.sub(r'[^\w]+', ' ', text).split()

    @classmethod
    def matches(cls, text):
        words = [w.replace('pseudo', 'sudo') for w in cls.normalise(text)]  # "pseudo" sounds exactly like "sudo"
        # Whole words within one letter ("sudu"), or "sue do" / "σου ντο" written as two words.
        candidates = words + [a + b for a, b in zip(words, words[1:]) if a in ('sue', 'su', 'soo', 'σου', 'σου')]
        # "Hey" and "Sudo" heard as one word: "hesudo", "heysudo", "χαισουντο".
        for word in list(candidates):
            for prefix in ('hey', 'hei', 'he', 'hay', 'χει', 'χε', 'χαι', 'ει', 'αι'):
                if word.startswith(prefix) and len(word) - len(prefix) >= 3:
                    candidates.append(word[len(prefix):])
        return any(abs(len(word) - len(target)) <= 1 and _distance(word, target) <= 1
                   for word in candidates for target in cls.TARGETS)

    def __call__(self, audio):
        # English letters for both languages ("Χέι Σούντο" sounds the same); auto-detection on such
        # a short clip often guessed Japanese or Spanish.
        # The "Hey Sudo." hint lifts acceptance of real wakes from 76% to 96% (held-out voices); it never
        # made ordinary speech pass, and look-alikes must fool the detector first (step 1).
        segments, _ = self.model.transcribe(audio, language='en', beam_size=1, vad_filter=False, without_timestamps=True,
                                            condition_on_previous_text=False, temperature=0.0, initial_prompt='Hey Sudo.')
        text = ' '.join(s.text for s in segments).strip()
        return self.matches(text), text


def _distance(a, b):
    row = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        prev, row[0] = row[0], i
        for j, y in enumerate(b, 1):
            prev, row[j] = row[j], min(row[j] + 1, row[j - 1] + 1, prev + (x != y))
    return row[-1]


class Asr:
    def __init__(self, folder, beam_size=5, vocabulary=VOCABULARY):
        add_cuda_dlls()
        from faster_whisper import WhisperModel
        self.beam_size = beam_size
        self.vocabulary = list(vocabulary or [])
        try:
            self.model = WhisperModel(str(folder), device='cuda', compute_type='int8_float16')
            self.device = 'cuda'
        except Exception as failure:  # no usable GPU: slower, but still local and correct
            print(f'ASR on CPU: {failure}', file=sys.stderr)
            self.model = WhisperModel(str(folder), device='cpu', compute_type='int8')
            self.device = 'cpu'

    def transcribe(self, audio):
        """One encoder pass shared by language detection and decoding (WhisperModel.transcribe
        encodes twice when the language is unknown, which doubled recognition time)."""
        from faster_whisper.audio import pad_or_trim
        from faster_whisper.tokenizer import Tokenizer
        from faster_whisper.transcribe import get_suppressed_tokens
        model = self.model
        encoded = model.encode(pad_or_trim(model.feature_extractor(audio)))
        scores = {token[2:-2]: prob for token, prob in model.model.detect_language(encoded)[0]}
        language = pick_language(max(scores, key=scores.get), scores.items())
        # task='transcribe' with the spoken language: Whisper never translates here.
        tokenizer = Tokenizer(model.hf_tokenizer, model.model.is_multilingual, task='transcribe', language=language)
        prompt = []
        if self.vocabulary:
            prompt = [tokenizer.sot_prev] + tokenizer.encode(' ' + ', '.join(self.vocabulary) + '.')[-200:]
        result = model.model.generate(
            encoded, [prompt + list(tokenizer.sot_sequence) + [tokenizer.no_timestamps]], beam_size=self.beam_size,
            max_length=448, suppress_blank=True, return_no_speech_prob=True,
            suppress_tokens=list(get_suppressed_tokens(tokenizer, [-1])))[0]
        if result.no_speech_prob > .6:
            return '', language
        return tokenizer.decode(result.sequences_ids[0]).strip(), language


class Speaker:
    """Supertonic 3 on CPU; sentences are synthesized while earlier ones play. stop() = barge-in."""

    RATE = 44100  # Supertonic 3 output rate; the stream opens before the model loads

    def __init__(self, folder, voice):
        import numpy as np
        import sounddevice as sd
        self.np = np
        self.folder, self.voice = folder, voice
        self.tts = None
        self.style = None
        self.model_lock = threading.Lock()
        self.rate = self.RATE
        self.jobs = queue.Queue()
        self.lock = threading.Lock()
        self.chunks = collections.deque()   # (generation, turn, samples)
        self.offset = 0
        self.generation = 0
        self.playing_turn = None
        self.started_turns = set()
        self.ended_turns = set()
        self.eos = {}                        # turn -> perf_counter at end of speech
        self.synth = {}                      # turn -> first synthesis ms
        self.stream = sd.OutputStream(samplerate=self.rate, channels=1, dtype='float32',
                                      latency='low', callback=self.callback)
        self.stream.start()
        self.heard_block = None
        self.meter = Meter(self.rate)
        threading.Thread(target=self.worker, daemon=True).start()
        threading.Thread(target=self.levels, daemon=True).start()

    def load(self):
        """Load and warm the voice model (about 0.4 GB of RAM, CPU only)."""
        with self.model_lock:
            if self.tts is not None:
                return
            from supertonic import TTS
            tts = TTS(model_dir=str(self.folder), auto_download=False)
            style = tts.get_voice_style(self.voice)
            if tts.sample_rate != self.rate:
                raise RuntimeError(f'Unexpected voice sample rate {tts.sample_rate}.')
            tts.synthesize('Hi.', voice_style=style, lang='en', total_steps=5)
            self.tts, self.style = tts, style

    def unload(self):
        with self.model_lock:
            self.tts = self.style = None

    def say(self, turn, text, lang):
        with self.lock:
            generation = self.generation
        self.jobs.put((generation, turn, text, lang if lang in LANGS else 'en'))

    def end(self, turn):
        with self.lock:
            generation = self.generation
        self.jobs.put((generation, turn, None, None))

    def stop(self):
        with self.lock:
            self.generation += 1
            interrupted = self.playing_turn is not None or bool(self.chunks)
            self.chunks.clear()
            self.offset = 0
            self.playing_turn = None
        if interrupted:
            emit('speech_stopped')

    def worker(self):
        while True:
            generation, turn, text, lang = self.jobs.get()
            with self.lock:
                if generation != self.generation:
                    continue
            if text is None:
                with self.lock:
                    self.chunks.append((generation, turn, None))
                continue
            first = turn not in self.synth
            start = time.perf_counter()
            try:
                self.load()  # usually already loaded by the wake; typed questions may get here first
                wav, _ = self.tts.synthesize(text, voice_style=self.style, lang=lang,
                                             total_steps=5 if first else 8, silence_duration=.12)
            except Exception as failure:
                emit('error', message=f'Speech synthesis failed: {failure}')
                continue
            if first:
                self.synth[turn] = round((time.perf_counter() - start) * 1000)
            samples = self.np.ascontiguousarray(wav.reshape(-1), dtype=self.np.float32)
            with self.lock:
                if generation == self.generation:
                    self.chunks.append((generation, turn, samples))

    def levels(self):
        """The orb follows Sudo AI's own voice while it speaks."""
        while True:
            time.sleep(1 / 30)
            block, self.heard_block = self.heard_block, None
            if block is not None and self.playing_turn is not None:
                features = self.meter(block)
                if features:
                    emit('level', source='tts', **features)

    def callback(self, outdata, frames, timing, status):
        out = outdata[:, 0]
        filled = 0
        events = []
        with self.lock:
            while filled < frames and self.chunks:
                generation, turn, samples = self.chunks[0]
                if samples is None:
                    self.chunks.popleft()
                    self.playing_turn = None
                    events.append(('spoken', turn))
                    continue
                if turn not in self.started_turns:
                    self.started_turns.add(turn)
                    events.append(('speaking', turn))
                self.playing_turn = turn
                count = min(frames - filled, len(samples) - self.offset)
                out[filled:filled + count] = samples[self.offset:self.offset + count]
                filled += count
                self.offset += count
                if self.offset >= len(samples):
                    self.chunks.popleft()
                    self.offset = 0
        out[filled:] = 0
        if filled:
            self.heard_block = out[:filled].copy()
        for name, turn in events:
            # Device output latency is added so "first audio" means heard, not just queued.
            heard = time.perf_counter() + self.stream.latency
            values = {'turn': turn}
            if name == 'speaking':
                values['synthMs'] = self.synth.get(turn)
                if turn in self.eos:
                    values['eosToAudioMs'] = round((heard - self.eos[turn]) * 1000)
            emit(name, **values)


class Voice:
    def __init__(self, args):
        import numpy as np
        self.np = np
        self.args = args
        self.data = Path(args.data)
        self.stop_event = threading.Event()
        self.frames = queue.Queue(maxsize=600)
        self.preroll = collections.deque(maxlen=PREROLL // FRAME + 1)
        self.recent = collections.deque(maxlen=int(VERIFY_SECONDS * SR) // FRAME + 2)  # for wake step 2
        self.verifier = None
        self.unverified = None   # utterance recorded since a detection that step 2 has not confirmed yet
        self.held = None         # its finished audio, released only if step 2 confirms
        self.recording = None        # 'ptt' | 'wake' | None
        self.buffer = []
        self.heard_speech = False
        self.silent_for = 0.0
        self.record_started = 0.0
        self.utterance = 0
        self.wake_enabled = args.wake
        self.wake = None
        self.wake_pending = np.zeros(0, dtype=np.int16)
        self.lock = threading.Lock()
        self.jobs = queue.Queue()
        self.speaker = None
        self.asr = None
        self.asr_ready = threading.Event()
        self.heavy_lock = threading.Lock()
        self.loading = False
        self.sleep_after_load = False
        self.mic_meter = Meter(SR)

    # -- microphone ---------------------------------------------------------------------------
    def mic_callback(self, indata, frames, timing, status):
        try:
            self.frames.put_nowait(indata[:, 0].copy())
        except queue.Full:
            pass  # A stalled consumer drops audio rather than growing memory.

    def load_wake(self):
        if self.wake is not None:
            return
        from openwakeword.model import Model
        folder = self.data / 'jarvis-models' / 'wake'
        tiny = self.data / 'jarvis-models' / 'whisper-tiny'
        if (tiny / 'model.bin').is_file():
            self.verifier = WakeVerifier(tiny)
        self.wake = Model(wakeword_models=[str(folder / 'hey_sudo.onnx')], inference_framework='onnx',
                          melspec_model_path=str(folder / 'melspectrogram.onnx'),
                          embedding_model_path=str(folder / 'embedding_model.onnx'))

    def warm(self):
        """Load Whisper (GPU) and the voice (CPU) in the background. Called the moment someone says
        "Hey Sudo" or presses the key, so loading overlaps with them speaking their question."""
        with self.heavy_lock:
            if self.loading or (self.asr is not None and self.speaker.tts is not None):
                return
            self.loading = True
        threading.Thread(target=self._warm, daemon=True).start()

    def _warm(self):
        started = time.perf_counter()
        emit('warming')
        try:
            if self.asr is None:
                asr = Asr(self.data / 'jarvis-models' / 'whisper-large-v3-turbo')
                asr.transcribe(self.np.zeros(SR, dtype=self.np.float32))
                self.asr = asr
            self.asr_ready.set()
            asr_ms = round((time.perf_counter() - started) * 1000)
            self.speaker.load()
            emit('warm', asrDevice=self.asr.device, asrLoadMs=asr_ms, loadMs=round((time.perf_counter() - started) * 1000))
        except Exception as failure:
            emit('error', message=f'Could not load speech models: {failure}')
        finally:
            with self.heavy_lock:
                self.loading = False
                deferred, self.sleep_after_load = self.sleep_after_load, False
            if deferred:
                self.sleep()

    def sleep(self):
        """Free Whisper's GPU memory and the voice model; only the light wake listener keeps running."""
        with self.heavy_lock:
            if self.loading:
                self.sleep_after_load = True  # unload as soon as the current load finishes
                return
            self.asr_ready.clear()
            self.asr = None
            self.speaker.unload()
        import gc
        gc.collect()
        emit('cold')

    def confirm_wake(self, utterance, audio):
        """Wake step 2 in the background: recording already runs, nothing heavy has loaded yet."""
        ok, heard = self.verifier(audio) if self.verifier else (True, '')
        with self.lock:
            current = self.unverified == utterance
            self.unverified = None
            held, self.held = (self.held, None) if current else (None, self.held)
            ended = self.recording is None  # the recording already stopped while step 2 ran
        if not current:
            return
        if not ok:
            emit('wake_rejected', heard=heard)
            self.finish(cancelled=True, quiet=True)
            return
        self.speaker.stop()  # barge-in
        self.warm()
        emit('wake', utterance=utterance)
        if held is not None:
            self.jobs.put(held)  # the question already ended while step 2 ran
        elif ended:
            emit('transcript', utterance=utterance, text='', language=None, reason='no-speech')

    def begin(self, mode, warm=True):
        if warm:
            self.warm()
        with self.lock:
            self.utterance += 1
            self.recording = mode
            self.buffer = list(self.preroll)
            self.heard_speech = False
            self.silent_for = 0.0
            self.record_started = time.perf_counter()
            self.vad.reset()
            return self.utterance

    def finish(self, cancelled=False, quiet=False):
        with self.lock:
            if not self.recording:
                return
            audio =self.np.concatenate(self.buffer) if self.buffer else self.np.zeros(0, dtype=self.np.float32)
            heard = self.heard_speech
            utterance = self.utterance
            mode, self.recording = self.recording, None
            self.buffer = []
        # End of speech = when the user stopped talking: the key release for push-to-talk, the last
        # voiced frame (not the later VAD decision) for the wake word.
        eos = time.perf_counter() - (self.silent_for if mode == 'wake' else 0.0)
        if cancelled:
            if not quiet:
                emit('cancelled', utterance=utterance)
        elif self.unverified == utterance:
            # Wake step 2 has not answered yet: keep the question until it does.
            with self.lock:
                self.held = (utterance, audio, eos) if heard else None
        elif not heard:
            emit('transcript', utterance=utterance, text='', language=None, reason='no-speech')
        else:
            self.jobs.put((utterance, audio, eos))

    def process(self):
        np = self.np
        while not self.stop_event.is_set():
            try:
                frame = self.frames.get(timeout=.1)
            except queue.Empty:
                continue
            if self.wake_enabled and self.recording is None and self.wake is not None:
                self.wake_pending = np.concatenate([self.wake_pending, (frame * 32767).astype(np.int16)])
                while len(self.wake_pending) >= WAKE_FRAME:
                    chunk, self.wake_pending = self.wake_pending[:WAKE_FRAME], self.wake_pending[WAKE_FRAME:]
                    scores = self.wake.predict(chunk)
                    if max(scores.values(), default=0) >= WAKE_THRESHOLD:
                        self.wake.reset()
                        self.wake_pending = np.zeros(0, dtype=np.int16)
                        audio = np.concatenate(list(self.recent))[-int(VERIFY_SECONDS * SR):]
                        # Record from now (the question follows at once) but load nothing until step 2 agrees.
                        utterance = self.begin('wake', warm=False)
                        with self.lock:
                            self.unverified, self.held = utterance, None
                        threading.Thread(target=self.confirm_wake, args=(utterance, audio), daemon=True).start()
                        break
            self.recent.append(frame)
            with self.lock:
                mode = self.recording
                if mode is None:
                    self.preroll.append(frame)
                    continue
                self.buffer.append(frame)
            features = self.mic_meter(frame)
            if features:
                emit('level', source='mic', **features)
            speech = self.vad(frame) >= SPEECH_PROB
            elapsed = time.perf_counter() - self.record_started
            if speech:
                self.heard_speech = True
                self.silent_for = 0.0
            else:
                self.silent_for += FRAME / SR
            if mode == 'wake' and ((self.heard_speech and self.silent_for >= END_SILENCE)
                                   or (not self.heard_speech and elapsed >= NO_SPEECH_TIMEOUT)):
                self.finish()
            elif elapsed >= MAX_UTTERANCE:
                self.finish()

    def recognize(self):
        while not self.stop_event.is_set():
            try:
                utterance, audio, eos = self.jobs.get(timeout=.2)
            except queue.Empty:
                continue
            waited = time.perf_counter()
            if not self.asr_ready.wait(180):
                emit('error', message='Speech recognition did not load in time.')
                continue
            start = time.perf_counter()
            try:
                text, language = self.asr.transcribe(audio)
            except Exception as failure:
                emit('error', message=f'Speech recognition failed: {failure}')
                continue
            asr_ms = round((time.perf_counter() - start) * 1000)
            self.speaker.eos[utterance] = eos
            emit('transcript', utterance=utterance, text=text, language=language, asrMs=asr_ms,
                 audioSeconds=round(len(audio) / SR, 2), asrWaitMs=round((start - waited) * 1000))

    # -- control ------------------------------------------------------------------------------
    def controls(self):
        try:
            for raw in control_lines(self.stop_event):
                try:
                    command = json.loads(raw)
                    name = command.get('command')
                    if name == 'stop':
                        break
                    if self.speaker is None:
                        continue  # still loading; the app only sends audio commands once ready
                    if name == 'listen_start':
                        self.speaker.stop()
                        emit('listening', utterance=self.begin('ptt'))
                    elif name == 'listen_auto':  # orb click: VAD decides when the question ends
                        self.speaker.stop()
                        emit('listening', utterance=self.begin('wake'))
                    elif name == 'listen_stop':
                        self.finish()
                    elif name == 'listen_cancel':
                        self.finish(cancelled=True)
                    elif name == 'speak':
                        self.speaker.say(int(command['turn']), str(command['text']), command.get('lang'))
                    elif name == 'speak_end':
                        self.speaker.end(int(command['turn']))
                    elif name == 'stop_speaking':
                        self.speaker.stop()
                    elif name == 'warm':
                        self.warm()
                    elif name == 'sleep':
                        self.sleep()
                    elif name == 'wake':
                        enabled = bool(command.get('enabled'))
                        if enabled:
                            self.load_wake()
                        self.wake_enabled = enabled
                except (ValueError, KeyError, TypeError, AttributeError) as failure:
                    emit('error', message=f'Bad command: {failure}')
                except Exception as failure:
                    emit('error', message=str(failure))
        finally:
            self.stop_event.set()

    def run(self):
        import sounddevice as sd
        models = self.data / 'jarvis-models'
        threading.Thread(target=self.controls, daemon=True).start()
        emit('loading', message='Starting the wake listener…')
        started = time.perf_counter()
        self.vad = Vad(models / 'silero_vad.onnx')
        speaker = Speaker(models / 'supertonic-3', self.args.voice)
        if self.wake_enabled:
            self.load_wake()
        self.speaker = speaker
        if self.args.keep:
            self.warm()  # "Keep AI loaded": ready before the first question
        # JARVIS_INPUT_DEVICE: the microphone chosen in Settings (full name); unset = Windows default.
        device = find_microphone(os.environ.get('JARVIS_INPUT_DEVICE'))
        with sd.InputStream(samplerate=SR, channels=1, dtype='float32', blocksize=FRAME, device=device,
                            latency='low', callback=self.mic_callback) as mic:
            threading.Thread(target=self.process, daemon=True).start()
            threading.Thread(target=self.recognize, daemon=True).start()
            emit('ready', loadMs=round((time.perf_counter() - started) * 1000),
                 microphone=sd.query_devices(mic.device if isinstance(mic.device, int) else None, 'input')['name'])
            # Wake detections also start loading the AI (see process()).
            while not self.stop_event.wait(.2):
                if not mic.active:
                    raise RuntimeError('The microphone was disconnected.')
        speaker.stream.close()


def bench(args):
    """Offline check: Supertonic speaks a phrase, Whisper must transcribe it in the same language."""
    import numpy as np
    models = Path(args.data) / 'jarvis-models'
    asr = Asr(models / 'whisper-large-v3-turbo')
    from supertonic import TTS
    tts = TTS(model_dir=str(models / 'supertonic-3'), auto_download=False)
    style = tts.get_voice_style(args.voice)
    asr.transcribe(np.zeros(SR, dtype=np.float32))
    results = []
    for lang, text in [('el', 'Καλημέρα Τζάρβις, τι καιρό θα κάνει αύριο στην Αθήνα;'),
                       ('en', 'Hey Jarvis, what is the weather like in Athens tomorrow?'),
                       ('el', 'Βάλε ένα ξυπνητήρι για τις επτά το πρωί.')]:
        wav, _ = tts.synthesize(text, voice_style=style, lang=lang)
        audio = wav.reshape(-1).astype(np.float32)
        # 44.1 kHz -> 16 kHz (linear interpolation is enough for a recognition check).
        target = np.linspace(0, len(audio) - 1, int(len(audio) * SR / tts.sample_rate))
        audio16 = np.interp(target, np.arange(len(audio)), audio).astype(np.float32)
        start = time.perf_counter()
        heard, language = asr.transcribe(audio16)
        results.append({'lang': lang, 'detected': language, 'asrMs': round((time.perf_counter() - start) * 1000),
                        'audioSeconds': round(len(audio16) / SR, 2), 'said': text, 'heard': heard})
    emit('bench', device=asr.device, results=results)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['run', 'bench', 'devices'])
    parser.add_argument('--data', required=True)
    parser.add_argument('--voice', default='M1')
    parser.add_argument('--wake', action='store_true')
    parser.add_argument('--keep', action='store_true', help='load Whisper and the voice at start')
    args = parser.parse_args()
    os.environ.setdefault('HF_HUB_OFFLINE', '1')
    try:
        with contextlib.redirect_stdout(sys.stderr):
            if args.command == 'devices':
                import sounddevice as sd
                emit('devices', microphones=microphones(), defaultInput=sd.query_devices(kind='input')['name'],
                     output=sd.query_devices(kind='output')['name'])
            elif args.command == 'bench':
                bench(args)
            else:
                Voice(args).run()
    except Exception as failure:
        emit('error', message=str(failure))
        return 1
    emit('off')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
