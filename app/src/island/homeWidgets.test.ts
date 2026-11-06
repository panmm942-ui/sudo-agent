import { describe, expect, it } from 'vitest';
import { focusRemaining, pauseFocus, startFocus, loadFocus } from './homeWidgets';
import { loadPreferences } from './preferences';
describe('Home widgets', () => {
  it('validates widget selection and transparency without changing older defaults', () => {
    const load = (v: unknown) => loadPreferences({getItem:()=>JSON.stringify(v)});
    expect(load({}).homeWidgets).toEqual(['clock','reminders']);
    expect(load({homeWidgets:['note','bad','note','focus'],islandTransparency:999})).toMatchObject({homeWidgets:['note','focus'],islandTransparency:100});
    expect(load({homeWidgets:[]} ).homeWidgets).toEqual([]);
  });
  it('uses a deadline across tab changes and pauses without losing time', () => {
    const running=startFocus({remaining:60_000,deadline:null},1000);
    expect(focusRemaining(running,21_000)).toBe(40_000);
    const paused=pauseFocus(running,21_000);
    expect(focusRemaining(paused,90_000)).toBe(40_000);
    expect(focusRemaining(startFocus(paused,90_000),130_000)).toBe(0);
  });
  it('restores valid timers and rejects malformed persisted state', () => {
    expect(loadFocus('{"remaining":30000,"deadline":12345}')).toEqual({remaining:30000,deadline:12345});
    expect(loadFocus('{"remaining":-5,"deadline":"bad"}')).toEqual({remaining:1500000,deadline:null});
  });
});
