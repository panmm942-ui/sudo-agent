import { playbackSeconds } from './lyricTiming';

export type VideoSyncClock = {
  key: string; position?: number | null; positionUpdatedAt?: number;
  duration?: number | null; playing: boolean; pauseWithSong?: boolean;
};

/** Each surface derives its frame from the same epoch-based song timeline.
 * Loading/opening time never starts a new loop. The native timeline remains
 * authoritative: a seek, song change, pause or resume corrects immediately.
 * Between snapshots a bounded 10 Hz check gently corrects small decoder drift;
 * circular distance avoids a spurious seek at a normal loop boundary. Media
 * completion events cannot recursively seek before a decoder settles, and a
 * target is only requested once the browser reports it as seekable.
 *
 * With pauseWithSong=false a paused clip continues from its song anchor until
 * playback resumes, when song sync takes precedence again. Repeated paused
 * snapshots do not reset that free-running anchor. Without a readable native
 * position playback can continue, but no song/frame alignment is fabricated.
 */
export function attachVideoSync(video: HTMLVideoElement, getClock: () => VideoSyncClock | null | undefined,
  options: { now?: () => number; intervalMs?: number;active?:boolean; onerror?: (error: unknown) => void } = {}) {
  const now = options.now ?? Date.now;
  const intervalMs = Math.max(100, options.intervalMs ?? 100);
  let stopped = false, failed = false, playRevision = 0;
  let active=options.active!==false,aligned=false;
  let alignedDuration=Number.NaN;
  let lastCorrectionAt = Number.NEGATIVE_INFINITY;
  let wantedPlaying: boolean | undefined;
  let previous: { clock: VideoSyncClock; at: number; seconds: number | null } | undefined;
  let freeAnchor: { seconds: number; at: number } | undefined;
  let timer: ReturnType<typeof setInterval>|undefined;

  function setPlaying(playing: boolean) {
    if (wantedPlaying === playing) return;
    wantedPlaying = playing;
    const revision = ++playRevision;
    if (!playing) { video.pause(); return; }
    void video.play().catch(error => {
      if (stopped || failed || revision !== playRevision || !wantedPlaying) return;
      if (error?.name === 'AbortError') { wantedPlaying = undefined; return; }
      failed = true;
      video.pause();
      clearInterval(timer);
      options.onerror?.(error);
    });
  }

  function update(force = false) {
    if (stopped || failed||!active) return;
    const clock = getClock();
    if (!clock) { previous = undefined; freeAnchor = undefined; setPlaying(false); return; }
    const at = now();
    const hasPosition = typeof clock.position === 'number' && Number.isFinite(clock.position);
    const seconds = hasPosition ? playbackSeconds(clock, at) : null;
    const free = clock.pauseWithSong === false && !clock.playing;
    const changed = !previous || previous.clock.key !== clock.key ||
      previous.clock.playing !== clock.playing || previous.clock.pauseWithSong !== clock.pauseWithSong;
    let seek = false;
    if (previous && seconds !== null && previous.seconds !== null) {
      const expected = playbackSeconds({ ...previous.clock, position: previous.seconds, positionUpdatedAt: previous.at }, at);
      seek = Math.abs(seconds - expected) > 0.25;
    }
    if (free && seconds !== null) {
      if (!freeAnchor || changed || seek) freeAnchor = { seconds, at: clock.positionUpdatedAt ?? at };
    } else freeAnchor = undefined;
    const target = freeAnchor ? freeAnchor.seconds + Math.max(0, at - freeAnchor.at) / 1000 : seconds;
    previous = { clock: { ...clock }, at, seconds };
    if (video.readyState < 1) return;
    const playing = clock.playing || clock.pauseWithSong === false;
    const duration = video.duration;
    if(force&&duration!==alignedDuration){aligned=false;alignedDuration=duration;}
    let rate=1;
    if (target !== null && Number.isFinite(target) && Number.isFinite(duration) && duration > 0) {
      const desired = ((target % duration) + duration) % duration;
      const distance = Math.abs(desired - video.currentTime) % duration;
      const drift = Math.min(distance, duration - distance);
      const exact=changed||seek||!playing||!aligned;
      const tolerance = exact ? 0.002 : 0.75;
      const signed=((desired-video.currentTime+duration*1.5)%duration)-duration/2;
      let seekable = false;
      for (let index = 0; index < video.seekable.length; index++) {
        if (desired >= video.seekable.start(index) && desired <= video.seekable.end(index)) { seekable = true; break; }
      }
      if (!video.seeking && seekable && drift > tolerance &&
        (changed || seek || !aligned || at - lastCorrectionAt >= (exact?intervalMs:1000))) {
        lastCorrectionAt = at;
        // Some engines expose duration before seeking is legal. Retry on the
        // next metadata/data event or timer instead of rejecting the source.
        try { video.currentTime = desired;aligned=true; } catch { /* not seekable yet */ }
      }else if(playing&&!exact&&!video.seeking&&video.readyState>=2&&seekable&&drift>0.035&&drift<=0.75){
        // Keep already decoded frames flowing. Small clock differences do not
        // justify flushing a decoder/keyframe queue every hundred milliseconds.
        rate=1+Math.max(-0.05,Math.min(0.05,signed*0.15));
      }
      if(drift<=0.002&&video.readyState>=2)aligned=true;
    }
    if (Math.abs(video.playbackRate-rate)>0.002)video.playbackRate=rate;
    setPlaying(playing);
  }

  const metadata = () => update(true);
  const data = () => update();
  for (const event of ['loadedmetadata', 'durationchange']) video.addEventListener(event, metadata);
  video.addEventListener('loadeddata', data);
  // A seek completion can report a clamped/rounded position. Correcting from
  // its handler forms a feedback loop that can starve decoding and play().
  // The bounded timer alone handles any remaining drift after completion.
  if(active){timer = setInterval(update, intervalMs);update(true);}
  return {
    update() { update(); },
    setActive(next:boolean){
      if(stopped||failed||active===next)return;
      active=next;clearInterval(timer);timer=undefined;
      if(!active){++playRevision;wantedPlaying=false;video.pause();return;}
      wantedPlaying=undefined;aligned=false;
      timer=setInterval(update,intervalMs);update(true);
    },
    stop() {
      if (stopped) return;
      stopped = true; ++playRevision;
      clearInterval(timer);
      for (const event of ['loadedmetadata', 'durationchange']) video.removeEventListener(event, metadata);
      video.removeEventListener('loadeddata', data);
      video.pause();
    },
  };
}
