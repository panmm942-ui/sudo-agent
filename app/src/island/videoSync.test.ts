import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { attachVideoSync, type VideoSyncClock } from './videoSync';

// The browser owns decoding and these mutable media properties. This double
// exercises our clock, lifecycle and correction decisions without a decoder.
function media(duration = 8) {
  const element = new EventTarget() as EventTarget & {
    duration: number; currentTime: number; readyState: number; paused: boolean;
    playbackRate: number; seeking: boolean; seekable: TimeRanges; play: ReturnType<typeof vi.fn>; pause: ReturnType<typeof vi.fn>;
  };
  Object.assign(element, { duration, currentTime: 0, readyState: 2, paused: true, playbackRate: 1, seeking: false });
  element.seekable = {get length(){return Number.isFinite(element.duration)&&element.duration>0?1:0;},start:()=>0,end:()=>element.duration};
  element.play = vi.fn(() => { element.paused = false; return Promise.resolve(); });
  element.pause = vi.fn(() => { element.paused = true; });
  return element;
}
const song = (patch: Partial<VideoSyncClock> = {}): VideoSyncClock => ({
  key: 'song', position: 14, positionUpdatedAt: 1000, playing: true, pauseWithSong: true, ...patch,
});

describe('song anchored background video', () => {
  it('corrects small running drift gently without repeatedly seeking through decoder frames',()=>{
    const element=media();let writes=0,value=6;
    Object.defineProperty(element,'currentTime',{get:()=>value,set:(next:number)=>{writes++;value=next;},configurable:true});
    const sync=attachVideoSync(element as unknown as HTMLVideoElement,()=>song());
    vi.setSystemTime(1200);sync.update();
    expect(writes).toBe(0);expect(element.playbackRate).toBeGreaterThan(1);expect(element.playbackRate).toBeLessThanOrEqual(1.05);
    element.currentTime=6.4;writes=0;sync.update();expect(writes).toBe(0);expect(element.playbackRate).toBeLessThan(1);sync.stop();
  });
  it('suspends decoding checks while hidden, then resumes on the authoritative song frame',()=>{
    const element=media();const sync=attachVideoSync(element as unknown as HTMLVideoElement,()=>song());
    sync.setActive(false);expect(element.paused).toBe(true);expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(2000);expect(element.currentTime).toBe(6);
    sync.setActive(true);expect(element.currentTime).toBe(0);expect(element.paused).toBe(false);expect(vi.getTimerCount()).toBe(1);sync.stop();
  });
  it('preserves the free-running pause anchor across a hidden interval',()=>{
    const element=media();let clock=song({position:9,playing:false,pauseWithSong:false});
    const sync=attachVideoSync(element as unknown as HTMLVideoElement,()=>clock);
    expect(element.currentTime).toBe(1);sync.setActive(false);
    vi.setSystemTime(4000);clock={...clock,positionUpdatedAt:4000};sync.setActive(true);
    expect(element.currentTime).toBe(4);sync.stop();
  });
  beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(1000); });
  afterEach(() => { vi.useRealTimers(); });

  it('starts at the song position modulo the clip duration, including the first frame at song zero', () => {
    const element = media(); let clock = song();
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock);
    expect(element.currentTime).toBe(6);
    clock = song({ position: 0 }); sync.update(); expect(element.currentTime).toBe(0);
    clock = song({ position: 24 }); sync.update(); expect(element.currentTime).toBe(0);
    sync.stop();
  });

  it('opens a second surface later at the same song-derived frame without inheriting its mount time', () => {
    const first = media(); const second = media(); const clock = song({ position: 15.25 });
    const one = attachVideoSync(first as unknown as HTMLVideoElement, () => clock);
    vi.setSystemTime(3500);
    const two = attachVideoSync(second as unknown as HTMLVideoElement, () => clock);
    one.update();
    expect(first.currentTime).toBe(1.75); expect(second.currentTime).toBe(1.75);
    one.stop(); two.stop();
  });

  it('waits for metadata and handles a late clip duration before seeking', () => {
    const element = media(Number.NaN); element.readyState = 0;
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => song());
    expect(element.currentTime).toBe(0); expect(element.play).not.toHaveBeenCalled();
    element.duration = 8; element.readyState = 1; element.dispatchEvent(new Event('loadedmetadata'));
    expect(element.currentTime).toBe(6); expect(element.paused).toBe(false); sync.stop();
  });

  it('seeks immediately on a song seek, pause, resume or different song even when the media source stays the same', () => {
    const element = media(); let clock = song();
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock);
    clock = song({ position: 35.5 }); sync.update(); expect(element.currentTime).toBe(3.5);
    clock = song({ position: 36, playing: false }); sync.update();
    expect(element.currentTime).toBe(4); expect(element.paused).toBe(true);
    vi.setSystemTime(6000); sync.update(); expect(element.currentTime).toBe(4);
    clock = song({ position: 36, positionUpdatedAt: 6000 }); sync.update();
    expect(element.currentTime).toBe(4); expect(element.paused).toBe(false);
    clock = song({ key: 'next', position: 0, positionUpdatedAt: 6000 }); sync.update();
    expect(element.currentTime).toBe(0); sync.stop();
  });

  it('corrects drift on a bounded timer and does not fight ordinary decoding or a loop boundary', () => {
    const element = media(); let clock = song({ position: 7.98 });
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock);
    element.currentTime = 0.02; sync.update();
    expect(element.currentTime).toBe(0.02);
    clock = song({ position: 2 }); element.currentTime = 5; vi.advanceTimersByTime(100);
    expect(element.currentTime).toBeCloseTo(2.1);
    element.currentTime = 2.23; vi.advanceTimersByTime(100);
    expect(element.currentTime).toBe(2.23); sync.stop();
  });

  it('holds the exact paused frame and recovers after a buffering delay', () => {
    const element = media(); const clock = song({ position: 10.35, playing: false });
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock);
    expect(element.currentTime).toBeCloseTo(2.35); expect(element.paused).toBe(true);
    element.currentTime = 2.38; sync.update(); vi.advanceTimersByTime(100); expect(element.currentTime).toBeCloseTo(2.35);
    sync.stop();
  });

  it('keeps running from a song anchor when pause-with-song is off without resetting on repeated paused snapshots', () => {
    const element = media(); let clock = song({ position: 9, playing: false, pauseWithSong: false });
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock);
    vi.setSystemTime(3500); clock = { ...clock, positionUpdatedAt: 3500 }; sync.update();
    expect(element.currentTime).toBe(3.5); expect(element.paused).toBe(false);
    clock = { ...clock, position: 20, positionUpdatedAt: 3500 }; sync.update(); expect(element.currentTime).toBe(4);
    vi.setSystemTime(4500); clock = { ...clock, playing: true, positionUpdatedAt: 4500 }; sync.update();
    expect(element.currentTime).toBe(4); sync.stop();
  });

  it('runs a free clip past the song duration while the song stays paused', () => {
    const element = media(); const clock = song({ position: 179, duration: 180, playing: false, pauseWithSong: false });
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock);
    expect(element.currentTime).toBe(3);
    vi.setSystemTime(6000); sync.update(); expect(element.currentTime).toBe(0); sync.stop();
  });

  it('does not invent an authoritative song position when the native timeline is unavailable', () => {
    const element = media(); element.currentTime = 3;
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => song({ position: null }));
    expect(element.currentTime).toBe(3); expect(element.paused).toBe(false); sync.stop();
  });

  it('pauses while the clock is absent and clears its timer and listeners when a source is disposed', () => {
    const element = media(); let clock: VideoSyncClock | null = song();
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock);
    clock = null; sync.update(); expect(element.paused).toBe(true);
    sync.stop(); const calls = element.play.mock.calls.length;
    clock = song({ position: 3 }); vi.advanceTimersByTime(1000); element.dispatchEvent(new Event('loadedmetadata')); sync.update();
    expect(element.play.mock.calls).toHaveLength(calls); expect(element.currentTime).toBe(6);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('reports a playback failure once and ignores a pending old play rejection after source disposal', async () => {
    const element = media(); const failure = vi.fn();
    element.play.mockImplementation(() => Promise.reject(new Error('Decoder failed')));
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => song(), { onerror: failure });
    await Promise.resolve(); vi.advanceTimersByTime(500); expect(failure).toHaveBeenCalledOnce(); sync.stop();
    let reject!: (error: Error) => void;
    element.play.mockImplementation(() => new Promise<void>((_, fail) => { reject = fail; }));
    const old = attachVideoSync(element as unknown as HTMLVideoElement, () => song(), { onerror: failure });
    old.stop(); reject(new Error('Old source')); await Promise.resolve(); expect(failure).toHaveBeenCalledOnce();
  });

  it('ignores AbortError from an interrupted play and can play after a pause/resume', async () => {
    const element = media(); const failure = vi.fn(); let clock = song();
    element.play.mockImplementationOnce(() => Promise.reject(Object.assign(new Error('Interrupted'), { name: 'AbortError' })));
    const sync = attachVideoSync(element as unknown as HTMLVideoElement, () => clock, { onerror: failure });
    await Promise.resolve(); clock = song({ playing: false }); sync.update(); clock = song(); sync.update();
    expect(failure).not.toHaveBeenCalled(); expect(element.paused).toBe(false); sync.stop();
  });

  it('never recursively seeks when seeked and loadeddata arrive before the decoder settles', () => {
    const element=media(1);let writes=0;
    // A decoder may clamp/round a requested position. Its completion events
    // must not synchronously launch another seek and starve playback.
    Object.defineProperty(element,'currentTime',{get:()=>0,set:()=>{writes++;},configurable:true});
    const sync=attachVideoSync(element as unknown as HTMLVideoElement,()=>song({position:14.35}));
    expect(writes).toBe(1);
    for(let index=0;index<50;index++){
      element.dispatchEvent(new Event('seeked'));element.dispatchEvent(new Event('loadeddata'));element.dispatchEvent(new Event('loadedmetadata'));
    }
    expect(writes).toBe(1);
    vi.advanceTimersByTime(100);expect(writes).toBe(1);sync.stop();
  });

  it('plays without fighting an unavailable seek range and aligns once the target becomes seekable', () => {
    const element=media();element.seekable={length:1,start:()=>0,end:()=>0};
    const sync=attachVideoSync(element as unknown as HTMLVideoElement,()=>song());
    expect(element.currentTime).toBe(0);expect(element.paused).toBe(false);
    element.dispatchEvent(new Event('loadeddata'));vi.advanceTimersByTime(100);
    expect(element.currentTime).toBe(0);
    element.seekable={length:1,start:()=>0,end:()=>8};vi.advanceTimersByTime(100);
    expect(element.currentTime).toBeCloseTo(6.2);sync.stop();
  });
});
