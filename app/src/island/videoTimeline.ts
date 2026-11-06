import type {VideoSyncClock} from './videoSync';

/** Preserve the first paused epoch so a view opened later derives the same
 * free-running clip phase as an already-open view. Windows may refresh its
 * observation timestamp without moving the paused song position. */
export function createVideoTimeline(){
  let previous:VideoSyncClock|undefined;
  return (track:VideoSyncClock|null|undefined,pauseWithSong:boolean):VideoSyncClock|undefined=>{
    if(!track){previous=undefined;return undefined;}
    const stablePause=!pauseWithSong&&!track.playing&&previous?.pauseWithSong===false&&!previous.playing&&
      previous.key===track.key&&previous.position===track.position;
    const clock={key:track.key,position:track.position,positionUpdatedAt:stablePause?previous!.positionUpdatedAt:track.positionUpdatedAt,
      duration:track.duration,playing:track.playing,pauseWithSong};
    previous=clock;return clock;
  };
}
