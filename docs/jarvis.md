# Sudo AI — local assistant and voice

Sudo AI is Sudo's optional local assistant. Hold **Right Ctrl**, speak Greek or English, and it answers aloud in the same language. Everything runs on the computer: Qwen3.5 4B through llama.cpp on the NVIDIA GPU, faster-whisper large-v3-turbo for speech recognition, and Supertonic 3 for the voice (on the CPU). Nothing is sent online.

## Setup (for people who download Sudo)

1. Open Sudo → Settings → **Sudo AI**. Holding Right Ctrl before Sudo AI is set up opens this page too.
2. Sudo checks the computer first: Windows 64-bit, an NVIDIA GPU with at least 6 GB of video memory, a driver with CUDA 12 or newer, enough free disk space and a microphone.
3. Press **Set up Sudo AI**. It downloads about 7 GB into the Sudo data folder, checks every file against a published SHA-256, and turns Sudo AI on when it finishes. The page shows "Step N of 4" and the GB downloaded. If setup is interrupted, the next click resumes it.
4. Pick your microphone, then hold Right Ctrl and talk, or type on the Home view.

Data folder: `%LOCALAPPDATA%\Sudo\data` (or the `SUDO_DATA` environment variable). Logs and per-answer timings: `jarvis-setup.log`, `jarvis-llm.log`, `jarvis-voice.log`, `jarvis-metrics.jsonl`.

## Where development stands (Nov 5, 2026)

Tested on the development desktop (GTX 1660 SUPER 6 GB, i5-10400), not yet on the target laptop (RTX 5050 8 GB):

| metric (median of 7 spoken turns, el + en) | desktop |
|---|---|
| end of speech → first audio heard (wake word; includes the 0.7 s silence wait) | 3.71 s |
| speech recognition | 1.34 s |
| LLM time to first token | 0.71 s |
| first voice chunk synthesis (CPU) | 0.69 s |

Also verified: a full setup from an empty data folder (all 4 steps), Greek replies to Greek speech and English replies to English speech, barge-in (speech stops within about 10 ms), and that the model and voice processes die with the app.

## Laptop test checklist

1. Build: `cd app`, `npm install`, `npx tauri build --no-bundle`, then copy `src-tauri/target/release/sudo-island.exe` to `%LOCALAPPDATA%\sudo-island\` (close Sudo first).
2. Settings → Sudo AI: confirm every computer check is green, then press **Set up Sudo AI**.
3. When it says ready: hold Right Ctrl, ask something in Greek, then in English. Check that each reply is in the same language and is spoken aloud.
4. Try the "Hey Jarvis" wake word, typing on Home, and interrupting a reply by holding Right Ctrl.
5. Report the timings shown under "Last answer" (or `jarvis-metrics.jsonl`). The first start compiles GPU code once (about a minute); measure after that.
6. With vocal mode on, check music keeps playing while Sudo AI thinks (separation pauses, then resumes).

Not tested yet: the push-to-talk key on real hardware (only the wake word and the voice process were tested), false wake rate, and noisy rooms.

## Fast commands

Simple PC commands run at once, without the language model: take a screenshot (Win+Print Screen, saved to Pictures\Screenshots), open an installed app (matched against the Start menu), type text into the window you were in, play/pause, next/previous song, volume up/down and mute. Greek and English both work.

- **Exact phrases** ("take a screenshot", "άνοιξε το Spotify", "επόμενο τραγούδι") are matched in Rust (`app/src-tauri/src/commands.rs`) and always work.
- **Any wording** uses [GLiNER2.5-multi-Decide](https://huggingface.co/fastino/GLiNER2.5-multi-Decide) (287M, Apache-2.0) on the CPU, added from Settings → Sudo AI → Fast commands (about 1.4 GB, pinned revision, every file checksum-verified, PyTorch CPU in its own environment). It only classifies the command; Sudo checks the details (an app must really be installed, "write me a poem" is not "type") and questions always go to the language model. It loads when Sudo AI wakes and unloads with the model; about 1.5 GB of memory while loaded.
- Measured on a 6-core CPU: about 0.2 s per command, 48 of 61 Greek/English test sentences classified correctly, with every miss falling back to the model rather than doing the wrong thing (`app/scripts/router_bench.py`, `router_eval.jsonl`).

The model is also told it cannot act on the PC itself, so it never claims to have done something it didn't. The conversation is kept in `jarvis-history.json` in the data folder (Settings → Sudo AI → Forget this conversation deletes it).

## "Hey Sudo" wake word

Saying "Hey Sudo", clicking the orb or holding Right Ctrl starts Sudo AI. Each can be switched on or off in Settings → Sudo AI, and at least one always stays on. Waking takes two steps, and nothing heavy loads before both agree:

1. `hey_sudo.onnx` (ships with Sudo, 0.86 MB): a small openWakeWord detector that listens all the time on the CPU.
2. Tiny Whisper (75 MB, CPU) checks the last 1.5 s and accepts only if it hears "Sudo". This removes false wakes from TV, music and look-alike words.

Measured on held-out data (synthetic voices the model never trained on, plus 80 min of unseen real speech): about 93% caught when clean and 87% in noise, and 0 false wakes in the real-speech test (step 1 fired 12 times, step 2 rejected all 12). Not yet measured on a real human voice or microphone.

Training is reproducible with `app/scripts/wakeword/generate.py` and `train.py`. Positives come from Piper (LibriTTS-R, 904 speakers) and Supertonic 3 (Greek and English). Negatives come from LibriSpeech (CC BY 4.0) and openWakeWord's ACAV100M feature set (CC BY-NC-SA 4.0). **Because of that feature set, `hey_sudo.onnx` is licensed CC BY-NC-SA 4.0 (non-commercial)**, like openWakeWord's own pretrained models.

The speech-to-text comparison on FLEURS (`app/scripts/stt_bench.py`) measured word errors as Greek / English: turbo 10.96% / 5.20%, large-v3 10.66% / 4.62% (2.7× slower), and the Greek turbo fine-tune 9.41% / 4.99%.
