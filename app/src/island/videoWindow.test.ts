import {describe,it,expect} from 'vitest';
import {validVideoInput} from './videoWindow';
describe('fullscreen media inputs',()=>{
  const base={quality:'best' as const,playing:true,pauseWithSong:true};
  it('permits artwork and lyrics without a video',()=>{
    expect(validVideoInput({...base,url:null,presentation:'artwork'})).toBe(true);
    expect(validVideoInput({...base,url:null,presentation:'lyrics'})).toBe(true);
    expect(validVideoInput({...base,url:null,presentation:'video'})).toBe(false);
  });
  it('keeps source restrictions in every presentation',()=>{
    expect(validVideoInput({...base,url:'https://evil.test/movie.mp4',presentation:'lyrics'})).toBe(false);
    expect(validVideoInput({...base,url:'https://canvaz.scdn.co/x.mp4',presentation:'video'})).toBe(true);
  });
});
