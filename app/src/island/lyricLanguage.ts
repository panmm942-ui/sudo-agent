const cache = new Map<string, string>();
export function targetLanguage(setting: string, browser = navigator.language): string {
  return (setting === "auto" ? browser : setting).split("-")[0].toLowerCase();
}
export async function romanize(text: string, signal?: AbortSignal): Promise<string> {
  signal?.throwIfAborted();
  const { transliterate } = await import("transliteration");
  signal?.throwIfAborted();
  // The generic table uses h/c for Russian х/ц. Use the familiar kh/ts
  // spelling; this remains transliteration rather than guessed pronunciation.
  const value = transliterate(text, { replace: [['Х', 'Kh'], ['х', 'kh'], ['Ц', 'Ts'], ['ц', 'ts']] });
  return value.trim() ? value : text;
}
export async function translateLine(text: string, language: string, signal: AbortSignal, get: typeof fetch = fetch): Promise<string> {
  signal.throwIfAborted();
  if (!text.trim()) return "";
  const key = `${language}:${text}`;
  if (cache.has(key)) return cache.get(key)!;
  if (new TextEncoder().encode(text).length > 480) throw new Error("This line is too long for the free translation service.");
  const query = new URLSearchParams({ q: text, langpair: `autodetect|${language}` });
  const response = await get(`https://api.mymemory.translated.net/get?${query}`, { signal });
  if (!response.ok) throw new Error("Translation service unavailable.");
  const data = await response.json();
  if (Number(data.responseStatus) !== 200 || typeof data.responseData?.translatedText !== "string") {
    throw new Error(data.quotaFinished ? "Free translation limit reached. Try again tomorrow." : "Translation is unavailable for this line.");
  }
  signal.throwIfAborted();
  const translated = data.responseData.translatedText.replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, "&");
  if (cache.size >= 500) cache.delete(cache.keys().next().value!);
  cache.set(key, translated);
  return translated;
}
