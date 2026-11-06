import {describe,expect,it} from 'vitest';
import {createWheelNavigation} from './wheel';
describe('two Home scroll steps',()=>{
  it('does not require a pause after Spotify to Home',()=>{
    const w=createWheelNavigation();
    expect(w.handle(-100,0,0,'spotify',true)).toBe('home');
    expect(w.handle(-100,0,40,'home',true)).toBeNull();
    expect(w.handle(-100,0,80,'home',true)).toBe('hide');
  });
  it('needs exactly two notches on Home and resets on direction change',()=>{
    const w=createWheelNavigation();
    expect(w.handle(-120,0,0,'home',false)).toBeNull();
    expect(w.handle(-120,0,50,'home',false)).toBe('hide');
    expect(w.handle(-120,0,100,'home',false)).toBeNull();
    w.handle(120,0,150,'home',false);
    expect(w.handle(-120,0,200,'home',false)).toBeNull();
  });
  it('normalizes line wheels and ignores tiny noise',()=>{
    const w=createWheelNavigation();
    expect(w.handle(-2,0,0,'home',true)).toBeNull();
    expect(w.handle(-3,1,10,'home',true)).toBeNull();
    expect(w.handle(-3,1,20,'home',true)).toBe('hide');
  });
  it('counts two smaller mouse-wheel steps without imposing a distance threshold',()=>{
    const w=createWheelNavigation();
    expect(w.handle(-40,0,0,'home',false)).toBeNull();
    expect(w.handle(-40,0,100,'home',false)).toBe('hide');
  });
});
describe('three pages: Home, Spotify, Sudo',()=>{
  it('scrolls down Home → Spotify → Sudo and up again',()=>{
    const w=createWheelNavigation();
    expect(w.handle(100,0,0,'home',true)).toBe('spotify');
    expect(w.handle(100,0,10,'spotify',true)).toBe('sudo');
    expect(w.handle(100,0,20,'sudo',true)).toBeNull();
    expect(w.handle(-100,0,30,'sudo',true)).toBe('spotify');
    expect(w.handle(-100,0,40,'spotify',true)).toBe('home');
  });
  it('skips Spotify when no song is available',()=>{
    const w=createWheelNavigation();
    expect(w.handle(100,0,0,'home',false)).toBe('sudo');
    expect(w.handle(-100,0,10,'sudo',false)).toBe('home');
  });
});
