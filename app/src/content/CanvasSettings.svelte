<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { CanvasConnection } from '../island/canvas';
  let { enabled, onchange, onconnectionchange, lookupError = '',showToggle=true,connectionTitle='Canvas connection' }: {
    showToggle?:boolean;connectionTitle?:string;
    enabled: boolean;
    onchange: (enabled: boolean) => void;
    onconnectionchange?: () => void;
    lookupError?: string;
  } = $props();
  let connection = $state<CanvasConnection>({ stored: false, kind: null });
  let kind = $state<'cookie' | 'token'>('cookie');
  let value = $state('');
  let busy = $state(false);
  let message = $state('');
  let mounted = false;
  onMount(() => {
    mounted = true;
    void invoke<CanvasConnection>('canvas_status').then(status => { if (mounted) connection = status; }).catch(() => { if (mounted) message = 'Canvas connections are available in the Windows app.'; });
    return () => { mounted = false; value = ''; };
  });
  async function connect() {
    busy = true; message = '';
    const credential = value.trim(); value = '';
    try {
      await invoke('canvas_connect', { kind, value: credential });
      if (!mounted) return;
      connection = { stored: true, kind };
      message = 'Credential saved securely. Spotify checks it when the expanded player requests a Canvas.';
      onconnectionchange?.();
    } catch (error) { if (mounted) message = typeof error === 'string' ? error : 'Could not save this connection.'; }
    finally { if (mounted) busy = false; }
  }
  async function disconnect() {
    busy = true; message = ''; value = '';
    try {
      await invoke('canvas_disconnect');
      if (!mounted) return;
      connection = { stored: false, kind: null }; message = 'Canvas connection removed.'; onconnectionchange?.();
    } catch (error) { if (mounted) message = typeof error === 'string' ? error : 'Could not remove this connection.'; }
    finally { if (mounted) busy = false; }
  }
</script>

<div class="canvas-settings">
  {#if showToggle}<label class="row switch"><span><strong>Spotify Canvas background</strong><small>Short, silent loops supplied for some songs. Missing videos use the song color.</small></span><input role="switch" type="checkbox" checked={enabled} onchange={event => onchange(event.currentTarget.checked)} /></label>{/if}
  <details>
    <summary>{connectionTitle} <span>{connection.stored ? 'Credential saved' : 'Not connected'}</span></summary>
    <p>Optional Spotify connection for Canvas backgrounds and fallback Shuffle/Repeat controls. Canvas loops are separate from full music videos.</p>
    <p>Playback fallback requires Spotify Premium and a connection with read/modify playback permission. Paste a credential from your own Spotify session. Sudo stores it in Windows Credential Manager; it never reads your browser cookies. Treat the credential like a password.</p>
    <label class="field"><span>Credential type</span><select bind:value={kind} disabled={busy}><option value="cookie">Spotify sp_dc cookie</option><option value="token">Short-lived Spotify access token</option></select></label>
    <p class="hint">{kind === 'cookie' ? 'Cookie exchange is best effort: Spotify rotates its unofficial authentication challenge. Public challenge data is retrieved from GitHub without your cookie. If Spotify rejects it, try an access token.' : 'Access tokens expire and need to be replaced manually. Use a token from your own signed-in Spotify session; paste only the value, without “Bearer”.'}</p>
    <form onsubmit={event => { event.preventDefault(); if (!busy && value.trim()) void connect(); }}>
      <label class="field"><span>{kind === 'cookie' ? 'sp_dc cookie value' : 'Spotify access token'}</span><input type="password" autocomplete="off" spellcheck="false" autocapitalize="off" maxlength="2200" bind:value placeholder="Paste value here" disabled={busy} /></label>
      <div class="actions"><button type="submit" class="pill-btn" disabled={busy || !value.trim()}>{busy ? 'Saving…' : connection.stored ? 'Replace credential' : 'Save connection'}</button>{#if connection.stored}<button type="button" class="pill-btn" disabled={busy} onclick={() => void disconnect()}>Disconnect</button>{/if}</div>
    </form>
    <p class="hint">{connection.stored ? `${connection.kind === 'token' ? 'Access token' : 'Cookie'} saved. A saved credential does not yet confirm Canvas access.` : 'No Spotify Canvas credential is stored.'} Credentials are sent only to Spotify. No hosted Canvas proxy or background server is used.</p>
    {#if message}<p role="status">{message}</p>{/if}
    {#if lookupError}<p role="status">{lookupError}</p>{/if}
  </details>
</div>

<style>
  .canvas-settings{font:var(--type-body)}.row{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:10px 0;min-height:44px;border-bottom:1px solid #ffffff12}.row>span{min-width:0}.row strong{font-weight:560}.row small{display:block;font:var(--type-caption);opacity:.75;margin-top:3px}.switch input{appearance:none;width:34px;height:20px;flex:none;margin:0;border:0;border-radius:20px;background:oklch(.55 .01 255);position:relative;cursor:pointer}.switch input::before{content:"";position:absolute;width:14px;height:14px;top:3px;left:3px;border-radius:50%;background:var(--text-primary);transition:transform 160ms ease}.switch input:checked{background:var(--accent)}.switch input:checked::before{transform:translateX(14px);background:var(--on-accent)}details{padding:10px 0;border-bottom:1px solid #ffffff12}summary{cursor:pointer;font-weight:560}summary span{float:right;font:var(--type-caption);opacity:.72}p{font:var(--type-caption);line-height:1.5;opacity:.82;margin:10px 0}.hint{opacity:.72}.field{display:flex;align-items:center;justify-content:space-between;gap:10px;margin:12px 0;font:var(--type-caption)}.field span{flex:1}select,input[type=password]{min-width:0;width:185px;max-width:60%;border:1px solid #ffffff25;background:var(--surface-control);border-radius:9px;color:inherit;color-scheme:dark;padding:7px;font:var(--type-caption)}option{background:#24262d;color:#f5f5f5}.actions{display:flex;gap:8px;flex-wrap:wrap}button:disabled{opacity:.45;cursor:default}@media(prefers-reduced-motion:reduce){.switch input::before{transition:none}}
</style>
