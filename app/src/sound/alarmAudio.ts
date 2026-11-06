import type { Preferences } from "../island/preferences";
import { sfx } from "./sfx";
import watchAlarm from '../assets/alarms/watch.mp3?url';
import bellAlarm from '../assets/alarms/bell.mp3?url';

export function createAlarmAudio() {
  let audio: HTMLAudioElement | undefined;
  let timeout: ReturnType<typeof setTimeout> | undefined;
  let fade: ReturnType<typeof setInterval> | undefined;

  function stop() {
    clearTimeout(timeout);
    clearInterval(fade);
    fade = undefined;
    audio?.pause();
    audio = undefined;
    sfx.stop("alarm");
  }

  function play(settings: Pick<Preferences, "tone" | "volume"> & Partial<Pick<Preferences, "alarmFade">>, custom?: string, previewEnded?: () => void) {
    stop();
    const synthesized = () => sfx.play("alarm", {
      tone: settings.tone === "custom" ? "chime" : settings.tone,
      volume: settings.volume,
      fadeSeconds: settings.alarmFade ? 10 : 0,
    });
    if (settings.volume > 0) {
      {
        const source=settings.tone==='custom'&&custom?custom:settings.tone==='bell'?bellAlarm:watchAlarm;
        const player = new Audio(source);
        audio = player;
        player.loop = true;
        const target = Math.min(1, Math.max(0, settings.volume / 100));
        player.volume = settings.alarmFade ? 0 : target;
        void player.play().then(() => {
          if (audio !== player || !settings.alarmFade) return;
          const start = performance.now();
          fade = setInterval(() => {
            const progress = Math.min(1, (performance.now() - start) / 10_000);
            player.volume = target * progress;
            if (progress >= 1) { clearInterval(fade); fade = undefined; }
          }, 50);
        }).catch(() => { if (audio === player) { player.pause(); synthesized(); } });
      }
    }
    if (previewEnded) timeout = setTimeout(() => { stop(); previewEnded(); }, 3500);
  }

  return { play, stop };
}
