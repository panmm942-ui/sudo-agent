<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { emit, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { youtubeId } from "../../island/youtube";

  /**
   * The floating YouTube mini-player window: YouTube's embedded player (privacy-enhanced mode),
   * a strip at the top to move the window, and a grip to resize it. The island controls it.
   */
  let id = $state(youtubeId(new URLSearchParams(location.search).get("v") ?? "") ?? "");
  let frame = $state<HTMLIFrameElement>();
  let info = { title: "", author: "", playing: false };

  const source = $derived(id ? `https://www.youtube-nocookie.com/embed/${id}?autoplay=1&enablejsapi=1&playsinline=1&rel=0&origin=${encodeURIComponent(location.origin)}` : "");

  function post(message: object): void { frame?.contentWindow?.postMessage(JSON.stringify(message), "*"); }
  const command = (func: string) => post({ event: "command", func, args: [] });
  const report = () => void emit("youtube:state", { id, ...info });

  onMount(() => {
    const onMessage = (event: MessageEvent) => {
      if (!/^https:\/\/www\.youtube(-nocookie)?\.com$/.test(event.origin) || typeof event.data !== "string") return;
      let data: { event?: string; info?: { playerState?: number; videoData?: { title?: string; author?: string } } };
      try { data = JSON.parse(event.data); } catch { return; }
      const next = data.info;
      if (!next) return;
      let changed = false;
      if (typeof next.playerState === "number") { const playing = next.playerState === 1 || next.playerState === 3; changed ||= playing !== info.playing; info.playing = playing; }
      if (next.videoData?.title && next.videoData.title !== info.title) { info.title = next.videoData.title; info.author = next.videoData.author ?? ""; changed = true; }
      if (changed) report();
    };
    window.addEventListener("message", onMessage);
    const offs: Promise<() => void>[] = [
      listen<string>("youtube:load", (e) => { const next = youtubeId(e.payload); if (next) { id = next; info = { title: "", author: "", playing: false }; } }),
      // The island's YouTube panel just opened and asks what is playing.
      listen("youtube:hello", () => { if (info.title) report(); }),
      listen<string>("youtube:control", (e) => {
        if (e.payload === "play") command("playVideo");
        else if (e.payload === "pause") command("pauseVideo");
        else if (e.payload === "toggle") command(info.playing ? "pauseVideo" : "playVideo");
        else if (e.payload === "mute") command("mute");
        else if (e.payload === "unmute") command("unMute");
      }),
    ];
    return () => { window.removeEventListener("message", onMessage); for (const off of offs) void off.then((u) => u()); };
  });

  // The embed only sends its state after it is asked to.
  function loaded(): void { post({ event: "listening", id: 1, channel: "widget" }); }
  const close = () => void invoke("youtube_control", { action: "close" });
  const resize = (event: PointerEvent) => { if (event.button === 0) void getCurrentWindow().startResizeDragging("SouthEast"); };
</script>

<main>
  {#if source}<iframe bind:this={frame} src={source} title="YouTube video" allow="autoplay; encrypted-media; picture-in-picture; fullscreen" allowfullscreen referrerpolicy="strict-origin-when-cross-origin" onload={loaded}></iframe>{/if}
  <header data-tauri-drag-region>
    <span data-tauri-drag-region>Sudo · YouTube</span>
    <button type="button" aria-label="Close" onclick={close}>✕</button>
  </header>
  <button type="button" class="grip" aria-label="Resize" onpointerdown={resize}></button>
</main>

<style>
  :global(html, body) { margin: 0; height: 100%; background: #000; overflow: hidden; color: #fff; font: 12px/1 system-ui, sans-serif; }
  main { position: fixed; inset: 0; }
  iframe { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; }
  header { position: absolute; left: 0; right: 0; top: 0; height: 26px; display: flex; align-items: center; justify-content: space-between; padding: 0 4px 0 10px; background: linear-gradient(#000c, #0000); opacity: 0; transition: opacity .15s; cursor: grab; }
  header:hover { opacity: 1; }
  header span { opacity: .85; }
  header button { width: 22px; height: 22px; border: 0; border-radius: 50%; background: #fff2; color: #fff; cursor: pointer; }
  header button:hover { background: #e5484d; }
  .grip { position: absolute; right: 0; bottom: 0; width: 14px; height: 14px; padding: 0; border: 0; background: linear-gradient(135deg, transparent 50%, #fff6 50%); cursor: se-resize; opacity: 0; }
  .grip:hover { opacity: 1; }
</style>
