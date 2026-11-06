"""Generate training/test audio for the "Hey Sudo" wake word (dev tool, never shipped).

Positives: "Hey Sudo" in English (Piper LibriTTS-R, 904 speakers) and Greek ("Χέι Σούντο",
Supertonic 3, small pitch shifts for more voices), varied speed and expressiveness. Piper's Greek
voices are not used: they garble short phrases (checked with Whisper).
Negatives: look-alike phrases and ordinary sentences from the same voices, plus real speech
(LibriSpeech dev-clean). Test sets use speakers/voices never seen in training.

Writes 16 kHz float32 clips to <out>/<split>.npz (object arrays of 1-D clips).
"""
import argparse
import random
from pathlib import Path

import numpy as np

SR = 16000
POS_EN = ['hey sudo', 'hey, sudo', 'hey sudo!', 'hey sudo.', 'Hey Sudo?', 'hey Sudo', 'Hey, Sudo!', 'heyy sudo']
POS_EL = ['Χέι Σούντο', 'Χέι, Σούντο', 'Έι Σούντο', 'Χέι Σούντο!', 'Χέι Σούντο.', 'Έι, Σούντο!']
NEAR_EN = ['hey sue', 'sudoku', 'hey judo', 'hey Jude', 'hey studio', 'so do I',
           'hey soda', 'hey Sophie', 'hey Siri', 'hey Jarvis', 'okay Google', 'hey there', 'hey you do',
           'hey sumo', 'hey dude', 'hey sudden', 'sudo', 'hey so', 'hey Susan', 'they sued oh', 'hello', 'hey Sam', 'hey, what is this', 'sushi', 'hey Suzy', 'say so', 'hey Judy', 'hay', 'hey Tudor']
NEAR_EL = ['Χέι σούπα', 'σούπερ', 'Έι Σούλα', 'Σούζι', 'σούντα', 'χέι εσύ', 'γεια σου', 'γεια σου ντε', 'σουτ',
           'τζούντο', 'Γιούντο', 'σε σένα το', 'Έι εσύ το', 'πες το', 'σουβλάκι', 'Χέι Σούλη', 'σου το είπα', 'σούρουπο',
           'Έι Σταύρο', 'Σούσου', 'Έλα Σούλα', 'χέι χέι', 'σούπα το βράδυ', 'Σωτήρη', 'σουδάρι']
SENT_EN = [
    'Can you turn the volume down a little', 'What time is the meeting tomorrow', 'I think we should order pizza tonight',
    'The weather looks nice this afternoon', 'Please send the report before Friday', 'Did you see the game last night',
    'Open the window, it is hot in here', 'My phone battery is almost empty', 'Let us take a short break',
    'Remind me to call my mother later', 'The train was late again this morning', 'I need to buy milk and bread',
    'This song is really good', 'How far is the airport from here', 'Turn on the lights in the kitchen',
    'We are going to the beach on Sunday', 'The computer is running very slowly today', 'Hey, how are you doing',
    'So do you want to come with us', 'Sure, I can do that for you', 'Pseudo random numbers are not truly random',
    'Run sudo apt update in the terminal', 'Hey, do you know where my keys are', 'Sue told me the story yesterday',
    'Play some music by the Beatles', 'Hey Jude, don’t make it bad', 'Studio sessions start at nine',
    'She bought a new sofa for the living room', 'The judo class is on Tuesday', 'Can you hear me now',
]
SENT_EL = [
    'Μπορείς να χαμηλώσεις λίγο την ένταση', 'Τι ώρα είναι η συνάντηση αύριο', 'Πάμε για καφέ το απόγευμα',
    'Ο καιρός είναι πολύ ωραίος σήμερα', 'Στείλε μου την αναφορά μέχρι την Παρασκευή', 'Είδες τον αγώνα χθες βράδυ',
    'Άνοιξε το παράθυρο, κάνει ζέστη', 'Το κινητό μου έχει σχεδόν τελειώσει', 'Ας κάνουμε ένα μικρό διάλειμμα',
    'Θύμισέ μου να πάρω τη μητέρα μου', 'Το τρένο άργησε πάλι σήμερα', 'Πρέπει να πάρω γάλα και ψωμί',
    'Αυτό το τραγούδι είναι πολύ ωραίο', 'Πόσο απέχει το αεροδρόμιο από εδώ', 'Άναψε τα φώτα στην κουζίνα',
    'Την Κυριακή θα πάμε στη θάλασσα', 'Ο υπολογιστής είναι πολύ αργός σήμερα', 'Γεια σου, τι κάνεις',
    'Έλα, θέλεις να έρθεις μαζί μας', 'Βέβαια, μπορώ να το κάνω αυτό', 'Βάλε λίγη μουσική να ακούσουμε',
    'Η σούπα είναι έτοιμη στο τραπέζι', 'Ο Σούλης έρχεται σε λίγο', 'Το τζούντο είναι την Τρίτη',
]


def resample(audio, rate):
    if rate == SR:
        return audio.astype(np.float32)
    target = np.linspace(0, len(audio) - 1, int(len(audio) * SR / rate))
    return np.interp(target, np.arange(len(audio)), audio).astype(np.float32)


class Piper:
    def __init__(self, path):
        from piper import PiperVoice
        self.voice = PiperVoice.load(str(path))
        self.speakers = max(1, self.voice.config.num_speakers)

    def say(self, text, speaker, rng):
        from piper import SynthesisConfig
        config = SynthesisConfig(speaker_id=speaker if self.speakers > 1 else None,
                                 length_scale=rng.uniform(0.75, 1.35), noise_scale=rng.uniform(0.4, 0.9),
                                 noise_w_scale=rng.uniform(0.5, 1.0))
        chunks = list(self.voice.synthesize(text, syn_config=config))
        audio = np.concatenate([c.audio_float_array for c in chunks]) if chunks else np.zeros(1, np.float32)
        return resample(audio, chunks[0].sample_rate if chunks else SR)


def pitch_shift(audio, semitones):
    """Resample-based shift (also changes speed slightly): widens a small set of voices."""
    factor = 2 ** (semitones / 12)
    target = np.arange(0, len(audio) - 1, factor)
    return np.interp(target, np.arange(len(audio)), audio).astype(np.float32)


class Supertonic:
    def __init__(self, folder):
        from supertonic import TTS
        self.tts = TTS(model_dir=str(folder), auto_download=False)
        self.styles = {name: self.tts.get_voice_style(name) for name in self.tts.voice_style_names}

    def say(self, text, voice, lang, rng):
        wav, _ = self.tts.synthesize(text, voice_style=self.styles[voice], lang=lang, total_steps=6,
                                     speed=rng.uniform(0.85, 1.25), silence_duration=0.05)
        return resample(wav.reshape(-1), self.tts.sample_rate)


def librispeech(folder, speakers):
    import soundfile as sf
    clips = []
    for path in sorted(Path(folder).rglob('*.flac')):
        if path.parts[-3] in speakers:
            audio, rate = sf.read(str(path), dtype='float32')
            clips.append(resample(audio, rate))
    return clips


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--voices', type=Path, required=True)
    parser.add_argument('--supertonic', type=Path, required=True)
    parser.add_argument('--librispeech', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--scale', type=float, default=1.0, help='multiply clip counts (quick runs: 0.1)')
    args = parser.parse_args()
    rng = random.Random(7)
    n = lambda count: max(4, int(count * args.scale))
    english = Piper(args.voices / 'en_US-libritts_r-medium.onnx')
    tonic = Supertonic(args.supertonic)
    train_voices = [v for v in tonic.styles if v not in ('F5', 'M5')]
    test_voices = ['F5', 'M5']
    train_speakers, test_speakers = range(0, 800), range(800, english.speakers)

    def batch(texts_en, texts_el, count_en, count_el, speakers, tonic_voices):
        clips = []
        for _ in range(count_en):
            clips.append(english.say(rng.choice(texts_en), rng.choice(speakers), rng))
        for _ in range(count_el):
            lang = rng.choice(['el', 'el', 'en'])
            text = rng.choice(texts_el if lang == 'el' else ['Hey Sudo', 'Hey, Sudo!', 'Hey Sudo.'] if texts_el is POS_EL else texts_en)
            clips.append(pitch_shift(tonic.say(text, rng.choice(tonic_voices), lang, rng), rng.uniform(-3, 3)))
        return clips

    args.out.mkdir(parents=True, exist_ok=True)
    splits = {
        'pos_train': batch(POS_EN, POS_EL, n(3000), n(1200), train_speakers, train_voices),
        'pos_test': batch(POS_EN, POS_EL, n(400), n(200), test_speakers, test_voices),
        'near_train': batch(NEAR_EN + SENT_EN, NEAR_EL + SENT_EL, n(3000), n(1500), train_speakers, train_voices),
        'near_test': batch(NEAR_EN + SENT_EN, NEAR_EL + SENT_EL, n(500), n(300), test_speakers, test_voices),
    }
    speakers = sorted({p.parts[-3] for p in args.librispeech.rglob('*.flac')})
    held_out = set(speakers[::4])  # a quarter of LibriSpeech speakers only for the false-trigger test
    splits['speech_train'] = librispeech(args.librispeech, set(speakers) - held_out)
    splits['speech_test'] = librispeech(args.librispeech, held_out)
    for name, clips in splits.items():
        array = np.empty(len(clips), dtype=object)
        array[:] = clips
        np.savez_compressed(args.out / f'{name}.npz', clips=array)
        print(f'{name}: {len(clips)} clips, {sum(len(c) for c in clips) / SR / 60:.1f} min', flush=True)


if __name__ == '__main__':
    main()
