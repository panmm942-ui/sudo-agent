export type FrameTiming={active:boolean;fps:number;meanMs:number;worstMs:number;frames:number};
export class AnimationTiming {
 private last:number|null=null; private published=0;private elapsed=0;private worst=0;private frames=0;
 record(time:number,active:boolean):FrameTiming|null {
  if(!active){if(this.last===null)return null;this.last=null;return this.snapshot(false);}
  if(this.last===null){this.elapsed=0;this.worst=0;this.frames=0;this.last=time;this.published=time;return this.snapshot(true);}
  const dt=time-this.last;this.last=time;
  if(dt>0){this.elapsed+=dt;this.worst=Math.max(this.worst,dt);this.frames++;}
  if(time-this.published<125)return null;
  this.published=time;return this.snapshot(true);
 }
 private snapshot(active:boolean):FrameTiming{return {active,fps:this.elapsed?this.frames*1000/this.elapsed:0,meanMs:this.frames?this.elapsed/this.frames:0,worstMs:this.worst,frames:this.frames};}
}
