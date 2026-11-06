import { describe, expect, it } from "vitest";
import { loadPreferences } from "./preferences";
const load = (saved: unknown) => loadPreferences({ getItem: () => JSON.stringify(saved) });
describe("appearance preference migration", () => {
  it('keeps the iPhone connection opt-in and only restores a boolean choice',()=>{
    expect(load({}).iphoneAlarms).toBe(false);
    expect(load({iphoneAlarms:true}).iphoneAlarms).toBe(true);
    expect(load({iphoneAlarms:'yes'}).iphoneAlarms).toBe(false);
  });
  it('preserves video behavior by default and independently saves fallback, cover, pause and quality choices',()=>{
    expect(load({})).toMatchObject({videoCanvasFallback:false,videoHideArtwork:false,videoPauseWithSong:true,videoQuality:'best'});
    expect(load({videoCanvasFallback:true,videoHideArtwork:true,videoPauseWithSong:false,videoQuality:'720'})).toMatchObject({videoCanvasFallback:true,videoHideArtwork:true,videoPauseWithSong:false,videoQuality:'720'});
    expect(load({videoQuality:'random',videoPauseWithSong:'no'})).toMatchObject({videoQuality:'best',videoPauseWithSong:true});
  });
  it('preserves the default player and independently selects optional Spotify controls',()=>{
    expect(load({})).toMatchObject({spotifyLayout:'centered',spotifyShowRepeat:false,spotifyShowShuffle:false});
    expect(load({spotifyLayout:'wide',spotifyShowRepeat:true,spotifyShowShuffle:true})).toMatchObject({spotifyLayout:'wide',spotifyShowRepeat:true,spotifyShowShuffle:true});
    expect(load({spotifyShowLike:true,spotifyShowRepeat:true})).not.toHaveProperty('spotifyShowLike');
  });
  it('keeps Canvas for existing users and accepts optional animated artwork',()=>{
    expect(load({video:true})).toMatchObject({video:true,spotifyBackground:'canvas'});
    expect(load({spotifyBackground:'artwork'})).toMatchObject({spotifyBackground:'artwork'});
    expect(load({spotifyBackground:'image'})).toMatchObject({spotifyBackground:'image'});
    expect(load({spotifyBackground:'bad'})).toMatchObject({spotifyBackground:'canvas'});
  });
  it('keeps the current player default and accepts the optional large cover card',()=>{
    expect(load({})).toMatchObject({spotifyLayout:'centered'});
    expect(load({spotifyLayout:'cover-card'})).toMatchObject({spotifyLayout:'cover-card'});
  });
  it('defaults to a calm classic wave and preserves explicit visibility choices',()=>{
    expect(load({})).toMatchObject({waveStyle:'classic',waveReactivity:40,waveSmoothness:65,compactAlwaysVisible:true,pinnedEdgeClearance:false});
    expect(load({waveReactivity:300,waveSmoothness:-2,waveStyle:'spectrum',compactAlwaysVisible:false,pinnedEdgeClearance:true})).toMatchObject({waveReactivity:100,waveSmoothness:0,waveStyle:'spectrum',compactAlwaysVisible:false,pinnedEdgeClearance:true});
    expect(load({waveStyle:'invalid'})).toMatchObject({waveStyle:'classic'});
  });
  it('keeps Home color independent of artwork and validates reader choices',()=>{
    expect(load({homeArtwork:true})).toMatchObject({homeSongColor:false,lyricLayout:'spotlight',lyricScale:1});
    expect(load({homeSongColor:true,islandBorder:'pearl',lyricLayout:'focus',lyricScale:8,lyricOffset:-20,lyricWeight:990})).toMatchObject({homeSongColor:true,islandBorder:'pearl',lyricLayout:'focus',lyricScale:1.4,lyricOffset:-5,lyricWeight:800});
    expect(load({lyricLayout:'unknown',lyricFont:'bad',lyricAlign:'bad'})).toMatchObject({lyricLayout:'spotlight',lyricFont:'sans',lyricAlign:'left'});
  });
  it("persists vocal level only and clamps it to valid blend endpoints", () => {
    expect(load({})).toMatchObject({vocalVolume:30});
    expect(load({vocalVolume:155})).toMatchObject({vocalVolume:100});
    expect(load({vocalVolume:-8})).toMatchObject({vocalVolume:0});
    expect(load({vocalVolume:45})).toMatchObject({vocalVolume:45});
    expect(load({vocalVolume:'bad',vocalEnabled:true})).toMatchObject({vocalVolume:30});
    expect(load({vocalEnabled:true})).not.toHaveProperty('vocalEnabled');
  });
  it("retains the current player and keeps Home artwork opt-in for older settings", () => {
    expect(load({ video: true })).toMatchObject({ video: true, spotifyLayout: "centered", homeArtwork: false, alarmFade: false, reactiveWaves: false, edgeAvoidance: false });
  });
  it("loads valid appearance choices and rejects unknown layouts", () => {
    expect(load({ spotifyLayout: "artwork-left", homeArtwork: true })).toMatchObject({ spotifyLayout: "artwork-left", homeArtwork: true });
    expect(load({ spotifyLayout: "bad", homeArtwork: "yes" })).toMatchObject({ spotifyLayout: "centered", homeArtwork: false });
  });
  it("keeps glass by default and accepts the white outline appearance", () => {
    expect(load({})).toMatchObject({ islandBorder: "glass" });
    expect(load({islandBorder:"white-line"})).toMatchObject({islandBorder:"white-line"});
    expect(load({islandBorder:"other"})).toMatchObject({islandBorder:"glass"});
  });
  it("migrates mutually exclusive lyric modes and clamps darkness", () => {
    expect(load({lyricRomanization:true,lyricTranslation:true})).toMatchObject({lyricRomanization:true,lyricTranslation:false});
    expect(load({islandDarkness:500})).toMatchObject({islandDarkness:100});
    expect(load({islandDarkness:-1})).toMatchObject({islandDarkness:0});
  });
});
