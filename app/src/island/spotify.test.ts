import { describe, expect, it } from "vitest";
import { coverTint } from "./spotify";

describe("Spotify cover tint", () => {
  it("uses each cover's hue and dims bright colors for the glass", () => {
    expect(coverTint(new Uint8ClampedArray([255, 0, 0, 255]))?.[0]).toBeCloseTo(0.56);
    expect(coverTint(new Uint8ClampedArray([0, 0, 255, 255]))?.[2]).toBeCloseTo(0.56);
    const rose = coverTint(new Uint8ClampedArray([255, 210, 210, 255]))!;
    expect(rose[0]).toBeLessThan(0.5);
    expect(rose[0]).toBeGreaterThan(rose[1]);
  });

  it("gives monochrome art a restrained graphite tint instead of bare glass", () => {
    const tint = coverTint(new Uint8ClampedArray([255, 0, 0, 0, 140, 140, 140, 255]))!;
    expect(tint).toBeDefined();
    expect(Math.max(...tint) - Math.min(...tint)).toBeLessThan(0.035);
    expect(Math.max(...tint)).toBeLessThan(0.4);
  });
  it("gives empty or transparent artwork a stable neutral fallback", () => {
    expect(coverTint(new Uint8ClampedArray())).toEqual(coverTint(new Uint8ClampedArray([200, 0, 0, 0])));
    expect(coverTint(new Uint8ClampedArray())).toBeDefined();
  });
});

import { afterEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { isSpotifyActionPending, openSpotify, watchSpotify } from "./spotify";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { vi.useRealTimers(); vi.resetAllMocks(); });
describe("Spotify optional action state", () => {
  it('keeps a high resolution cover through native thumbnail polls without repeating the lookup',async()=>{
    vi.useFakeTimers();const source={key:'high-res-watch',title:'Song',artist:'Artist',album:'Album',duration:180,artwork:'data:image/png;base64,bG93',playing:true,position:1};
    vi.mocked(invoke).mockImplementation(async(command)=>command==='spotify_cover_art'?{url:'data:image/png;base64,aGlnaA==',sourceUrl:'https://i.scdn.co/image/large',width:640,height:640,spotifyUrl:'https://open.spotify.com/track/3OHfY25tqY28d16oZczHc8',trackId:'3OHfY25tqY28d16oZczHc8'}:{...source});
    const values:unknown[]=[];const watcher=watchSpotify(value=>values.push(value));await vi.advanceTimersByTimeAsync(0);
    expect(values.at(-1)).toMatchObject({artwork:'data:image/png;base64,aGlnaA==',position:1});await vi.advanceTimersByTimeAsync(2500);
    expect(values.at(-1)).toMatchObject({artwork:'data:image/png;base64,aGlnaA=='});expect(vi.mocked(invoke).mock.calls.filter(([command])=>command==='spotify_cover_art')).toHaveLength(1);watcher.stop();
  });
  it('uses the Apple Music cover when Spotify has no larger one (soft 300 px thumbnails)',async()=>{
    vi.useFakeTimers();const source={key:'apple-cover',title:'Song',artist:'Artist',album:'Album',duration:180,artwork:'data:image/png;base64,bG93',playing:true};
    vi.mocked(invoke).mockImplementation(async(command)=>command==='spotify_cover_art'?null:command==='artwork_still_image'?'data:image/jpeg;base64,YXBwbGU=':{...source});
    const values:unknown[]=[];const watcher=watchSpotify(value=>values.push(value));await vi.advanceTimersByTimeAsync(0);
    expect(values.at(-1)).toMatchObject({key:'apple-cover',artwork:'data:image/jpeg;base64,YXBwbGU='});
    await vi.advanceTimersByTimeAsync(2500);
    expect(values.at(-1)).toMatchObject({artwork:'data:image/jpeg;base64,YXBwbGU='});
    expect(vi.mocked(invoke).mock.calls.filter(([command])=>command==='artwork_still_image')).toHaveLength(1);watcher.stop();
  });
  it('does not attach a delayed cover to the next native song',async()=>{
    vi.useFakeTimers();let finish:(value:unknown)=>void=()=>{};let read=0;
    vi.mocked(invoke).mockImplementation(async(command)=>{
      if(command==='spotify_cover_art')return read===1?new Promise(resolve=>finish=resolve):null;
      if(command==='artwork_still_image')return null;
      read++;return {key:`cover-race-${read}`,title:`Song ${read}`,artist:'Artist',album:'Album',duration:180,artwork:'native',playing:true};
    });
    const values:unknown[]=[];const watcher=watchSpotify(value=>values.push(value));await vi.advanceTimersByTimeAsync(1250);
    finish({url:'data:image/png;base64,aGlnaA==',sourceUrl:'https://i.scdn.co/image/large',width:640,height:640,spotifyUrl:'https://open.spotify.com/track/3OHfY25tqY28d16oZczHc8',trackId:'3OHfY25tqY28d16oZczHc8'});await vi.advanceTimersByTimeAsync(0);
    expect(values.at(-1)).toMatchObject({key:'cover-race-2',artwork:'native'});watcher.stop();
  });
  it("keeps independent controls available while shuffle or repeat is pending", () => {
    expect(isSpotifyActionPending('shuffle', ['shuffle'])).toBe(true);
    expect(isSpotifyActionPending('repeat', ['shuffle'])).toBe(false);
    expect(isSpotifyActionPending('toggle', ['shuffle', 'repeat'])).toBe(false);
    expect(isSpotifyActionPending('next', ['shuffle'])).toBe(false);
    expect(isSpotifyActionPending('previous', ['repeat'])).toBe(false);
  });
  it("serializes transport commands without blocking independent options", () => {
    expect(isSpotifyActionPending('toggle', ['next'])).toBe(true);
    expect(isSpotifyActionPending('next', ['previous'])).toBe(true);
    expect(isSpotifyActionPending('shuffle', ['toggle'])).toBe(false);
    expect(isSpotifyActionPending('repeat', [])).toBe(false);
  });
  it("opens only the fixed native Spotify launch command", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(true);
    await expect(openSpotify()).resolves.toBe(true);
    expect(invoke).toHaveBeenCalledWith("spotify_open");
  });
  it("keeps the last track through a transient failure and accepts a confirmed closed session", async () => {
    vi.useFakeTimers();
    const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
    const track = { key: "a", artwork: null, playing: true };
    vi.mocked(invoke).mockResolvedValueOnce(track).mockRejectedValueOnce("temporary session failure").mockResolvedValueOnce(null);
    const update = vi.fn();
    const watcher = watchSpotify(update);
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(1250);
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).toHaveBeenLastCalledWith(track);
    await vi.advanceTimersByTimeAsync(1250);
    expect(update).toHaveBeenLastCalledWith(null);
    watcher.stop();
    warning.mockRestore();
  });
  it("clears stale controls after three consecutive failed reads and recovers", async () => {
    vi.useFakeTimers();
    const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
    const track = { key: "a", artwork: null, playing: true };
    vi.mocked(invoke).mockResolvedValueOnce(track).mockRejectedValueOnce("failed").mockRejectedValueOnce("failed").mockRejectedValueOnce("failed").mockResolvedValueOnce(track);
    const update = vi.fn();
    const watcher = watchSpotify(update);
    await vi.advanceTimersByTimeAsync(2500);
    expect(update).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1250);
    expect(update).toHaveBeenLastCalledWith(null);
    expect(warning).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1250);
    expect(update).toHaveBeenLastCalledWith(track);
    watcher.stop();
    warning.mockRestore();
  });
  it("retries malformed IPC responses instead of treating them as an absent session", async () => {
    vi.useFakeTimers();
    const warning = vi.spyOn(console, "warn").mockImplementation(() => {});
    const track = { key: "a", artwork: null, playing: true };
    vi.mocked(invoke).mockResolvedValueOnce(track).mockResolvedValueOnce(new ArrayBuffer(8)).mockResolvedValueOnce({ ...track, playing: false });
    const update = vi.fn();
    const watcher = watchSpotify(update);
    await vi.advanceTimersByTimeAsync(1250);
    expect(update).toHaveBeenCalledTimes(1);
    expect(warning).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ message: "Spotify returned an unreadable ArrayBuffer response." }));
    await vi.advanceTimersByTimeAsync(1250);
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ key: "a", playing: false }));
    watcher.stop();
    warning.mockRestore();
  });
  it("requests optional state only while enabled and publishes changes to it", async () => {
    vi.useFakeTimers();
    const track = { key: "a", artwork: null, playing: true, liked: false, shuffle: false, repeatMode: "off" };
    vi.mocked(invoke).mockResolvedValueOnce(track).mockResolvedValueOnce({ ...track, liked: true, shuffle: true, repeatMode: "track" });
    const update = vi.fn();
    let like = false;
    const watcher = watchSpotify(update, () => ({ like }));
    await vi.advanceTimersByTimeAsync(0);
    expect(invoke).toHaveBeenLastCalledWith("spotify_state", { knownTrack: null, features: { like: false } });
    like = true;
    await vi.advanceTimersByTimeAsync(1250);
    expect(invoke).toHaveBeenLastCalledWith("spotify_state", { knownTrack: null, features: { like: true } });
    expect(update).toHaveBeenLastCalledWith(expect.objectContaining({ liked: true, shuffle: true, repeatMode: "track" }));
    watcher.stop();
  });
  it("discards a pending state read after stop", async () => {
    let finish!: (value: unknown) => void;
    vi.mocked(invoke).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    const update = vi.fn();
    const watcher = watchSpotify(update);
    watcher.stop();
    finish({ key: "old", liked: true });
    await Promise.resolve();
    expect(update).not.toHaveBeenCalled();
  });
});
