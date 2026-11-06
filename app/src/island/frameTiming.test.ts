import {expect,it} from 'vitest';
import {AnimationTiming} from './frameTiming';
it('includes dropped draw intervals and retains worst gap',()=>{const m=new AnimationTiming();m.record(0,true);m.record(14,true);m.record(56,true);m.record(70,true);const r=m.record(84,false)!;expect(r.frames).toBe(3);expect(r.worstMs).toBe(42);expect(r.fps).toBeCloseTo(3000/70);});
it('does not count idle time',()=>{const m=new AnimationTiming();m.record(0,true);m.record(14,true);m.record(20,false);expect(m.record(5000,false)).toBeNull();m.record(6000,true);m.record(6014,true);expect(m.record(6020,false)?.meanMs).toBe(14);});
