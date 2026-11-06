/* Sudo's minimal current-player reader. No Spicetify, theme or CLI dependency.
 * Reads the existing React platform provider and only resolves two live APIs.
 * It does not patch Spotify's bundles or export its authorization credentials.
 * Self-healing: Spotify can replace its player object while running (after sleep, a network
 * reconnect or an internal reload). The reader then finds the current one again, instead of
 * holding on to a stale player that no longer reports a song (which silenced Sudo for good).
 */
(function sudoSpotifyClient(){
  'use strict';
  if(globalThis.__sudoSpotifyClient)return;
  const listeners={songchange:new Set(),onplaypause:new Set()};
  const live={player:null,authorization:null};
  let previousUri,previousPaused;
  function find(){
    const seen=new Set(),stack=[];
    for(const root of document.querySelectorAll('#root,#main')){
      const key=Object.keys(root).find(name=>name.startsWith('__reactContainer$')||name.startsWith('__reactFiber$'));
      if(key){const fiber=root[key];stack.push(fiber?.stateNode?.current??fiber);}
    }
    while(stack.length&&seen.size<5000){
      const fiber=stack.pop();if(!fiber||seen.has(fiber))continue;seen.add(fiber);
      const platform=fiber.memoizedProps?.platform;
      if(typeof platform?.getRegistry==='function'){
        const registry=platform.getRegistry();
        if(registry?._map instanceof Map&&typeof registry.resolve==='function'){
          const resolve=name=>{const key=[...registry._map.keys()].find(key=>typeof key==='symbol'&&key.description===name);return key?registry.resolve(key):null;};
          const player=resolve('PlayerAPI'),authorization=resolve('AuthorizationAPI');
          if(typeof player?.getState==='function'&&authorization)return {player,authorization};
        }
      }
      // Visit the upper provider chain before the large sibling UI trees.
      stack.push(fiber.sibling,fiber.child);
    }
    return null;
  }
  function attach(found){
    if(found.player===live.player&&found.authorization===live.authorization)return;
    live.player=found.player;live.authorization=found.authorization;
    const state=live.player.getState();previousUri=state?.item?.uri;previousPaused=state?.isPaused;
    if(typeof live.player._events?.addListener==='function')live.player._events.addListener('update',()=>{
      queueMicrotask(()=>{
        if(found.player!==live.player)return; // an older player we replaced
        const current=live.player.getState();
        const song=current?.item?.uri!==previousUri,paused=current?.isPaused!==previousPaused;
        previousUri=current?.item?.uri;previousPaused=current?.isPaused;
        if(song)for(const callback of listeners.songchange)callback();
        if(paused)for(const callback of listeners.onplaypause)callback();
      });
    });
  }
  const state=()=>{try{return live.player?.getState()??null;}catch{return null;}};
  const Player={
    get data(){return state();},
    getDuration:()=>Number(state()?.duration)||0,
    getProgress:()=>{const s=state();if(!s)return 0;const elapsed=s.isPaused||s.isBuffering?0:Math.max(0,Date.now()-s.timestamp);return Math.min(Number(s.duration)||0,Math.max(0,(Number(s.positionAsOfTimestamp)||0)+elapsed));},
    isPlaying:()=>!state()?.isPaused,
    addEventListener:(name,callback)=>{if(typeof callback==='function')listeners[name]?.add(callback);},
  };
  const Platform={get AuthorizationAPI(){return live.authorization;}};
  function discover(){const found=find();if(!found)return false;attach(found);globalThis.__sudoSpotifyClient??={Player,Platform};return true;}
  if(!discover()){const timer=setInterval(()=>{try{if(discover())clearInterval(timer);}catch{/* Wait for Spotify's provider to finish mounting. */}},1000);}
  // Every 5 s: a player that no longer reports a song may have been replaced; look again.
  setInterval(()=>{try{if(live.player&&!state()?.item)discover();}catch{/* Keep the current player. */}},5000);
})();
