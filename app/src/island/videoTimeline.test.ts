import {describe,it,expect} from 'vitest';
import {createVideoTimeline} from './videoTimeline';
describe('shared video timeline',()=>{
  it('retains the paused epoch across native polls so late views share the same phase',()=>{
    const update=createVideoTimeline(),song={key:'song',position:12,positionUpdatedAt:1000,playing:false};
    expect(update(song,false)?.positionUpdatedAt).toBe(1000);
    expect(update({...song,positionUpdatedAt:8000},false)?.positionUpdatedAt).toBe(1000);
    expect(update({...song,position:20,positionUpdatedAt:9000},false)?.positionUpdatedAt).toBe(9000);
    expect(update({...song,key:'next',positionUpdatedAt:10000},false)?.positionUpdatedAt).toBe(10000);
  });
  it('resets on playback, preference changes and disappearance',()=>{
    const update=createVideoTimeline(),song={key:'song',position:1,positionUpdatedAt:1000,playing:false};
    update(song,false);
    expect(update({...song,playing:true,positionUpdatedAt:2000},false)?.positionUpdatedAt).toBe(2000);
    expect(update({...song,positionUpdatedAt:3000},true)?.positionUpdatedAt).toBe(3000);
    expect(update(null,false)).toBeUndefined();
    expect(update({...song,positionUpdatedAt:4000},false)?.positionUpdatedAt).toBe(4000);
  });
});
