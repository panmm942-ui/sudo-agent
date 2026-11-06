import { describe, expect, it } from "vitest";
import { parseLrc, lyricWindow, playbackSeconds } from "./lyricTiming";
describe("synchronized three-line lyrics", () => {
  it('preserves explicit enhanced LRC word timing and strips timing from displayed text',()=>{
    const [line]=parseLrc('[offset:100]\n[00:05]<00:05>One <00:05.50>moment<00:06>');
    expect(line.text).toBe('One moment');expect(line.words).toEqual([{time:4.9,end:5.4,text:'One '},{time:5.4,end:5.9,text:'moment'}]);
    expect(parseLrc('[00:05]One moment')[0].words).toBeUndefined();
    expect(parseLrc('[00:05]<00:05>One <00:04>moment<00:06>')[0].words).toBeUndefined();
  });
  it.each([
    {language:'Greek',stamp:'00:37.55',expected:37.3,text:'Δοκιμή'},
    {language:'Russian',stamp:'00:15.51',expected:15.26,text:'Проверка'},
    {language:'English',stamp:'00:13.13',expected:12.88,text:'Sample'},
    {language:'French',stamp:'00:16.07',expected:15.82,text:'Échantillon'},
    {language:'Japanese',stamp:'00:01.52',expected:1.27,text:'テスト'},
  ])('advances a positive LRC metadata offset without changing $language text',({stamp,expected,text})=>{
    const [line]=parseLrc(`[offset:+250]\n[${stamp}]${text}`);
    expect(line.time).toBeCloseTo(expected,8);expect(line.text).toBe(text);
    expect(lyricWindow([line],expected+.001)[1]).toBe(line);
    expect(lyricWindow([line],expected-.001)[1]).toBeUndefined();
  });
  it('delays a negative metadata offset and clamps an advanced intro at zero',()=>{
    expect(parseLrc('[offset:-500]\n[00:01.20]Sample')[0].time).toBe(1.7);
    expect(parseLrc('[offset:+1500]\n[00:01.20]Sample')[0].time).toBe(0);
  });
  it("parses repeated timestamps, sorting, and offset metadata", () => {
    const lines = parseLrc('[offset:100]\n[00:03.00]Third\n[00:01.20][00:02.40]Repeat');
    expect(lines).toHaveLength(3);
    [1.1,2.3,2.9].forEach((time,index)=>expect(lines[index].time).toBeCloseTo(time,8));
    expect(lyricWindow(lines,2.6).map(l=>l?.text)).toEqual(['Repeat','Repeat','Third']);
  });
  it("does not pretend a line is current before it starts, and follows seeks", () => {
    const lines=parseLrc('[00:05]First\n[00:10]Second');
    expect(lyricWindow(lines,0).map(l=>l?.text)).toEqual([undefined,undefined,'First']);
    expect(lyricWindow(lines,11).map(l=>l?.text)).toEqual(['First','Second',undefined]);
    expect(lyricWindow(lines,6)[1]?.text).toBe('First');
  });
  it("interpolates playback only while playing and clamps to duration", () => {
    expect(playbackSeconds({position:10,positionUpdatedAt:1000,playing:true,duration:20},3500)).toBe(12.5);
    expect(playbackSeconds({position:10,positionUpdatedAt:1000,playing:false},3500)).toBe(10);
    expect(playbackSeconds({position:19,positionUpdatedAt:1000,playing:true,duration:20},3500)).toBe(20);
  });
});
import {estimateWords,wordTokens} from './lyricTiming';
describe('estimated word timing',()=>{
  it('tokenizes so tokens rejoin to the exact display text',()=>{
    for(const text of ['Снова от чужих сообщений, мне ни жарко','"Hello," she said ','テスト です','Pyanyy Gottem bez dushi'])expect(wordTokens(text).join('')).toBe(text);
    expect(wordTokens('Снова от чужих')).toEqual(['Снова ','от ','чужих']);
  });
  it('spreads words across the line, ordered and before the next line',()=>{
    const words=estimateWords('Снова от чужих сообщений мне ни жарко, ни холодно',13.64,19.96)!;
    expect(words.map(w=>w.text).join('')).toBe('Снова от чужих сообщений мне ни жарко, ни холодно');
    expect(words[0].time).toBe(13.64);
    words.forEach((w,i)=>{expect(w.end).toBeGreaterThan(w.time);if(i)expect(w.time).toBeCloseTo(words[i-1].end,8);});
    expect(words.at(-1)!.end).toBeLessThan(19.96);
  });
  it('fits a short gap and ignores blank lines',()=>{
    const words=estimateWords('quick words here',10,10.5)!;
    expect(words.at(-1)!.end).toBeLessThanOrEqual(10.38+1e-9);
    expect(estimateWords('   ',1,2)).toBeUndefined();
  });
});
