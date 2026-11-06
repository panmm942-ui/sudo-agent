export type IslandPage = 'home' | 'spotify' | 'sudo';

/**
 * Pages in scroll order: Home → Spotify (only while a song is available) → Sudo.
 * Two significant upward wheel steps on Home hide the island; no stop/restart gesture is required.
 */
export function createWheelNavigation(){
  let steps=0;let last=-Infinity;
  return {
    reset(){steps=0;last=-Infinity;},
    handle(delta:number,mode:number,time:number,page:IslandPage,hasTrack:boolean):IslandPage|'hide'|null {
      const pixels=delta*(mode===1?40:mode===2?240:1);
      if(Math.abs(pixels)<6)return null;
      if(pixels>0){
        steps=0;last=-Infinity;
        if(page==='home')return hasTrack?'spotify':'sudo';
        if(page==='spotify')return 'sudo';
        return null;
      }
      if(page==='sudo'){steps=0;last=time;return hasTrack?'spotify':'home';}
      if(page==='spotify'){steps=0;last=time;return 'home';}
      if(time-last>2000)steps=0;
      last=time;steps++;
      if(steps<2)return null;
      steps=0;return 'hide';
    }
  };
}
