import { beforeEach, describe, expect, it, vi } from 'vitest';
const fake = vi.hoisted(() => ({ instances: [] as any[], supported: true }));
vi.mock('hls.js', () => ({ default: class {
  static isSupported() { return fake.supported; }
  static Events = { ERROR: 'error', MANIFEST_PARSED: 'manifest' };
  config: any; handlers: Record<string, Function> = {}; levels = [{height:360, videoCodec:'avc1.64001f'}, {height:1080,videoCodec:'avc1.640020'}]; autoLevelCapping = -1;
  constructor(config: any) { this.config=config; fake.instances.push(this); }
  attachMedia = vi.fn(); loadSource = vi.fn(); destroy = vi.fn();
  startLoad = vi.fn(); startLevel = -1; loadLevel = -1;
  on(event: string, callback: Function) { this.handlers[event]=callback; }
} }));
import { attachArtworkVideo } from './artworkPlayback';
const url = 'https://mvod.itunes.apple.com/itunes-assets/HLSMusic211/v4/example/default.m3u8';
function video() { return {canPlayType:vi.fn(()=>''),pause:vi.fn(),removeAttribute:vi.fn(),load:vi.fn(),src:''} as unknown as HTMLVideoElement; }
async function flush() { await vi.dynamicImportSettled(); }

describe('animated artwork playback lifecycle', () => {
  beforeEach(()=>{fake.instances.length=0;fake.supported=true;});
  it('starts bounded HLS loading and releases the decoder/source on cleanup', async()=>{
    const element=video(); const fail=vi.fn(); const release=attachArtworkVideo(element,url,fail); await flush();
    const hls=fake.instances[0]; expect(hls).toBeDefined(); expect(hls.loadSource).toHaveBeenCalledWith(url);
    expect(hls.config.enableWorker).toBe(true);
    expect(hls.config.maxBufferLength).toBeLessThanOrEqual(12); expect(hls.config.maxBufferSize).toBeLessThanOrEqual(8_000_000);
    release(); expect(hls.destroy).toHaveBeenCalledOnce(); expect(element.pause).toHaveBeenCalled(); expect(element.removeAttribute).toHaveBeenCalledWith('src'); expect(fail).not.toHaveBeenCalled();
  });
  it('does not initialize HLS after disposal during its lazy import', async()=>{
    const release=attachArtworkVideo(video(),url); release(); await flush(); expect(fake.instances).toHaveLength(0);
  });
  it('starts at the highest resolution and bitrate without a display-size quality cap', async()=>{
    const release=attachArtworkVideo(video(),url);await flush();const hls=fake.instances[0];
    hls.levels=[{width:1080,height:1080,bitrate:5_000_000},{width:360,height:360,bitrate:300_000},{width:2160,height:2160,bitrate:20_000_000},{width:2160,height:2160,bitrate:28_000_000}];
    hls.handlers.manifest();
    expect(hls.config.autoStartLoad).toBe(false);expect(hls.config.capLevelToPlayerSize).toBe(false);
    expect(hls.startLevel).toBe(3);expect(hls.loadLevel).toBe(3);expect(hls.startLoad).toHaveBeenCalledOnce();release();
  });
  it('uses bitrate to choose between variants at the same resolution',async()=>{
    const release=attachArtworkVideo(video(),url);await flush();const hls=fake.instances[0];
    hls.levels=[{width:1080,height:1080,bitrate:10_000_000},{width:1080,height:1080,bitrate:5_000_000}];
    hls.handlers.manifest();expect(hls.loadLevel).toBe(0);release();
  });
  it('starts and stays on the best supported rendition at or below the requested quality', async()=>{
    const release=attachArtworkVideo(video(),url,vi.fn(),'720');await flush();const hls=fake.instances[0];
    hls.levels=[{width:2160,height:2160,bitrate:20_000_000},{width:480,height:480,bitrate:300_000},{width:720,height:720,bitrate:2_000_000}];
    hls.handlers.manifest();expect(hls.startLevel).toBe(2);expect(hls.loadLevel).toBe(2);release();
  });
  it('fails once if neither MSE nor native HLS is supported', async()=>{
    fake.supported=false;const fail=vi.fn();const element=video();attachArtworkVideo(element,url,fail);await flush();
    expect(fake.instances).toHaveLength(0);expect(fail).toHaveBeenCalledOnce();expect(element.src).toBe('');
  });
  it('uses native HLS only when MSE is unavailable and native support is advertised', async()=>{
    fake.supported=false;const fail=vi.fn();const element=video();vi.mocked(element.canPlayType).mockReturnValue('probably');
    const release=attachArtworkVideo(element,url,fail);await flush();expect(fake.instances).toHaveLength(0);expect(element.src).toBe(url);expect(fail).not.toHaveBeenCalled();release();
  });
  it('prefers MSE HLS even when Chromium advertises unworking native HLS', async()=>{
    const element=video(); vi.mocked(element.canPlayType).mockReturnValue('maybe');
    attachArtworkVideo(element,url); await flush(); expect(fake.instances).toHaveLength(1);
  });
  it('falls back once for a fatal stream failure and blocks untrusted child fetches', async()=>{
    const fail=vi.fn(); attachArtworkVideo(video(),url,fail); await flush(); const hls=fake.instances[0];
    expect(()=>hls.config.xhrSetup({withCredentials:false},'https://evil.test/a.ts')).toThrow();
    hls.handlers.error('error',{fatal:true}); hls.handlers.error('error',{fatal:true});
    expect(fail).toHaveBeenCalledOnce(); expect(hls.destroy).toHaveBeenCalledOnce();
  });
  it('rejects an untrusted initial source before constructing a player', async()=>{
    const fail=vi.fn(); attachArtworkVideo(video(),'https://evil.test/a.m3u8',fail); await flush(); expect(fake.instances).toHaveLength(0); expect(fail).toHaveBeenCalledOnce();
  });
});
