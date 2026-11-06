import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import Spotify from './Spotify.svelte';
import type { SpotifyTrack } from '../island/spotify';

const track:SpotifyTrack={key:'current',title:'Current song',artist:'Current artist',artwork:'https://i.scdn.co/image/current',playing:true,canToggle:true,canNext:true,canPrevious:true};
describe('Spotify content modes',()=>{
  it('replaces the ordinary player with an in-island search view',()=>{
    const html=render(Spotify,{props:{track,oncontrol:async()=>true,searchOpen:true,onsearchchange:()=>{},onlyrics:()=>{},onvideo:()=>{}}}).body;
    expect(html).toContain('aria-label="Song search"');
    expect(html).toContain('Back to player');
    expect(html).not.toContain('aria-label="Playback controls"');
    expect(html).not.toContain('Current song');
    expect(html).not.toContain('Show lyrics');
    expect(html).not.toContain('LRCLIB');
  });
  it('keeps an accessible pin surface behind the controls (the cover itself never pins)',()=>{
    const html=render(Spotify,{props:{track,oncontrol:async()=>true,artworkVideo:true,onpin:()=>{}}}).body;
    expect(html).toContain('aria-label="Keep Spotify open"');
    expect(html).toMatch(/class="pin-surface(?:\s|")/);
    expect(html).not.toMatch(/class="artwork-button[^"]*"[^>]*aria-pressed/);
    expect(html).toContain('aria-label="Playback controls"');
  });
});
