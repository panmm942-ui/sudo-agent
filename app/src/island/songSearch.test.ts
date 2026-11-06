import { describe, expect, it, vi } from 'vitest';
import { playSearchResult, searchSongs, type SongResult } from './songSearch';

const result: SongResult = { id: '3OHfY25tqY28d16oZczHc8', uri: 'spotify:track:3OHfY25tqY28d16oZczHc8',
  title: 'One', artist: 'Artist', album: 'Album', artwork: 'https://i.scdn.co/image/abc', duration: 180, source: 'spotify' };
const deezer: SongResult = { id: '1234567', uri: 'deezer:track:1234567', title: 'Robbery', artist: 'Juice WRLD', album: 'Death Race for Love',
  artwork: 'https://cdn-images.dzcdn.net/images/cover/2e018122cb56986277102d2041a592c8/250x250-000000-80-0-0.jpg', duration: 240, source: 'deezer' };

describe('native Spotify catalogue search', () => {
  it('uses the signed-in native Spotify catalogue and preserves playable identities', async () => {
    const call=vi.fn().mockResolvedValue([result]);
    expect(await searchSongs('  Song & artist  ',new AbortController().signal,call)).toEqual([result]);
    expect(call).toHaveBeenCalledExactlyOnceWith('spotify_search_tracks',{query:'Song & artist'});
  });
  it('stays idle on a short query or an already cancelled view', async () => {
    const call=vi.fn(); const controller=new AbortController();
    expect(await searchSongs(' x ',controller.signal,call)).toEqual([]);
    controller.abort(); await expect(searchSongs('Song',controller.signal,call)).rejects.toMatchObject({name:'AbortError'});
    expect(call).not.toHaveBeenCalled();
  });
  it('rejects an overlong query rather than silently changing the requested search', async () => {
    const call=vi.fn(); await expect(searchSongs('x'.repeat(201),new AbortController().signal,call)).rejects.toThrow(/200/);
    expect(call).not.toHaveBeenCalled();
  });
  it('cancels promptly and prevents a late old catalogue reply from becoming a result', async () => {
    let complete!:(value:SongResult[])=>void; const call=vi.fn(()=>new Promise<unknown>(resolve=>{complete=resolve as typeof complete;}));
    const old=new AbortController(); const pending=searchSongs('First',old.signal,call);
    old.abort(); await expect(pending).rejects.toMatchObject({name:'AbortError'});
    complete([result]); await Promise.resolve();
    const current=vi.fn().mockResolvedValue([{...result,title:'Second'}]);
    expect(await searchSongs('Second',new AbortController().signal,current)).toEqual([{...result,title:'Second'}]);
  });
  it('preserves actionable native connection and rate-limit errors without opening anything else', async () => {
    const call=vi.fn().mockRejectedValue('Spotify rate limited player controls. Try again in 42 seconds.');
    await expect(searchSongs('Song',new AbortController().signal,call)).rejects.toThrow(/42 seconds/);
    expect(call).toHaveBeenCalledOnce();
  });
  it('rejects malformed track identities and drops unsafe artwork before rendering', async () => {
    const call=vi.fn().mockResolvedValue([{...result,uri:'spotify:track:wrong'}, {...result,id:'4OHfY25tqY28d16oZczHc8',uri:'spotify:track:4OHfY25tqY28d16oZczHc8',artwork:'https://evil.test/pixel'}]);
    expect(await searchSongs('Song',new AbortController().signal,call)).toEqual([{...result,id:'4OHfY25tqY28d16oZczHc8',uri:'spotify:track:4OHfY25tqY28d16oZczHc8',artwork:null}]);
  });
  it('starts only the selected Spotify URI on the same native track and preserves rejection details', async () => {
    const call=vi.fn().mockResolvedValue('played');
    await expect(playSearchResult(result,'native-song-key',call)).resolves.toBe('played');
    expect(call).toHaveBeenCalledExactlyOnceWith('spotify_play_track',{uri:result.uri,trackKey:'native-song-key'});
    call.mockRejectedValue('Spotify Premium and playback permission are required.');
    await expect(playSearchResult(result,'native-song-key',call)).rejects.toThrow(/Premium/);
  });
  it('does not report playing after native rejection or send malformed selection requests', async () => {
    const call=vi.fn().mockResolvedValue(false);
    await expect(playSearchResult(result,'native-song-key',call)).rejects.toThrow(/confirm/);
    call.mockClear(); await expect(playSearchResult({...result,uri:'https://open.spotify.com/'},'native-song-key',call)).rejects.toThrow();
    expect(call).not.toHaveBeenCalled();
  });
  it('keeps Deezer fallback results with their own identity and trusted Deezer covers only', async () => {
    const call=vi.fn().mockResolvedValue([deezer,{...deezer},{...deezer,id:'7654321',uri:'deezer:track:7654321',artwork:'https://evil.test/images/cover/x.jpg'},
      {...deezer,id:'0123',uri:'deezer:track:0123'},{...deezer,id:'42',uri:'spotify:track:42'},{...deezer,source:'spotify'},{...result,source:'deezer'},
      {...deezer,id:'99',uri:'deezer:track:99',artwork:'https://e-cdns-images.dzcdn.net/images/cover/abc/250x250.jpg?x=1'}]);
    expect(await searchSongs('Robbery',new AbortController().signal,call)).toEqual([deezer,{...deezer,id:'7654321',uri:'deezer:track:7654321',artwork:null},
      {...deezer,id:'99',uri:'deezer:track:99',artwork:null}]);
  });
  it('does not accept Deezer cover hosts for Spotify rows or Spotify covers for Deezer rows', async () => {
    const call=vi.fn().mockResolvedValue([{...result,artwork:deezer.artwork},{...deezer,artwork:result.artwork}]);
    expect((await searchSongs('Song',new AbortController().signal,call)).map(row=>row.artwork)).toEqual([null,null]);
  });
  it('sends a chosen Deezer result natively and reports when it was opened in the Spotify app', async () => {
    const call=vi.fn().mockResolvedValue('opened');
    await expect(playSearchResult(deezer,'native-song-key',call)).resolves.toBe('opened');
    expect(call).toHaveBeenCalledExactlyOnceWith('spotify_play_track',{uri:'deezer:track:1234567',trackKey:'native-song-key',search:'Juice WRLD Robbery'});
    call.mockResolvedValue('searched');
    await expect(playSearchResult(deezer,'native-song-key',call)).resolves.toBe('searched');
    call.mockClear();
    await expect(playSearchResult({...deezer,uri:'deezer:album:1234567'},'native-song-key',call)).rejects.toThrow();
    await expect(playSearchResult({...deezer,source:'spotify'},'native-song-key',call)).rejects.toThrow();
    expect(call).not.toHaveBeenCalled();
  });
});
