/** The 11-character video id from a YouTube link (watch, youtu.be, shorts, embed, live) or a bare id. */
export function youtubeId(text: string): string | null {
  const value = text.trim();
  if (/^[A-Za-z0-9_-]{11}$/.test(value)) return value;
  let url: URL;
  try { url = new URL(/^https?:\/\//i.test(value) ? value : `https://${value}`); } catch { return null; }
  const host = url.hostname.replace(/^(www|m|music)\./, "");
  let id: string | null = null;
  if (host === "youtu.be") id = url.pathname.slice(1).split("/")[0];
  else if (host === "youtube.com" || host === "youtube-nocookie.com") {
    id = url.searchParams.get("v") ?? (/^\/(?:shorts|embed|live|v)\/([^/?#]+)/.exec(url.pathname)?.[1] ?? null);
  }
  return id && /^[A-Za-z0-9_-]{11}$/.test(id) ? id : null;
}

/** What the floating player reports (null when it's closed). */
export type YoutubeState = { id: string; title: string; author: string; playing: boolean } | null;
