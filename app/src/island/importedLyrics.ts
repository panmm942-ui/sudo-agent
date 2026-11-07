/** Lyrics the user imported for a song (an .lrc or plain .txt file), kept on this PC by song. */
const KEY = "sudo.lyrics.imported.v1";
const MAX_SONGS = 300;
const MAX_CHARS = 150_000;

type Store = Record<string, { text: string; at: number }>;

function read(storage: Pick<Storage, "getItem">): Store {
  try { const value = JSON.parse(storage.getItem(KEY) ?? "{}"); return value && typeof value === "object" && !Array.isArray(value) ? value : {}; } catch { return {}; }
}

export function importedLyrics(trackKey: string, storage: Pick<Storage, "getItem"> = localStorage): string | null {
  const entry = read(storage)[trackKey];
  return entry && typeof entry.text === "string" ? entry.text : null;
}

/** Saves (or, with null, forgets) a song's imported lyrics; the oldest songs make room. */
export function saveImportedLyrics(trackKey: string, text: string | null, storage: Pick<Storage, "getItem" | "setItem"> = localStorage): void {
  const store = read(storage);
  if (text === null) delete store[trackKey];
  else store[trackKey] = { text: text.slice(0, MAX_CHARS), at: Date.now() };
  const keys = Object.keys(store).sort((a, b) => store[b].at - store[a].at);
  for (const old of keys.slice(MAX_SONGS)) delete store[old];
  storage.setItem(KEY, JSON.stringify(store));
}
