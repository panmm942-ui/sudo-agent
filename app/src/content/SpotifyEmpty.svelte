<script lang="ts">
  import {openSpotify} from '../island/spotify';
  import spotifyIcon from '../assets/spotify-icon.svg';
  let {error='',onretry}:{error?:string;onretry?:()=>void}=$props();
  let busy=$state(false),message=$state('');
  async function open(){
    if(busy)return;busy=true;message='';
    try{if(!await openSpotify())message='Windows could not open Spotify. Check that the desktop app is installed.';else onretry?.();}
    catch(cause){message=typeof cause==='string'?cause:'Spotify could not open. Try opening the desktop app directly.';}
    finally{busy=false;}
  }
</script>

<section class="empty-player" aria-label="Spotify connection status">
  <img src={spotifyIcon} width="28" height="28" alt="" />
  <h2>Spotify is here.</h2>
  <p>Open the desktop app and play a song.<br />Your music will appear automatically.</p>
  <div class="actions"><button type="button" class="pill-btn" disabled={busy} onclick={open}>{busy?'Opening…':'Open Spotify'}</button><button type="button" class="retry" onclick={()=>{message='';onretry?.();}}>Retry</button></div>
  {#if message||error}<p class="status" role="status">{message||error}</p>{/if}
</section>

<style>
  .empty-player{height:100%;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;padding:20px 66px 22px 28px;text-align:center;}
  img{opacity:.9;flex:none;}h2{margin:0;font:600 20px/25px var(--font-sans);letter-spacing:var(--tracking-tight);}p{margin:0;color:var(--text-secondary);font:var(--type-caption);line-height:17px;}.actions{display:flex;align-items:center;gap:14px;margin-top:3px;}.pill-btn{padding:7px 14px;font:var(--type-label);}.retry{border:0;background:transparent;color:var(--text-primary);font:var(--type-caption);cursor:pointer;padding:8px;}.status{position:absolute;bottom:5px;left:28px;right:64px;overflow:hidden;white-space:nowrap;text-overflow:ellipsis;font-size:10px;line-height:13px;}
</style>
