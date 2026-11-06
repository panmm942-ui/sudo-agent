/**
 * Home weather from Open-Meteo (free, no key). Only the city the user typed is sent; results are
 * cached for 30 minutes so Home never polls more often than that.
 */
export type Weather = { city: string; temperature: number; code: number; isDay: boolean; at: number };

const CACHE_KEY = "sudo.home.weather.v1";
const FRESH_MS = 30 * 60_000;

/** WMO weather code → short label and a simple glyph. */
export function describe(code: number, isDay = true): { label: string; glyph: string } {
  if (code === 0) return { label: isDay ? "Clear" : "Clear night", glyph: isDay ? "☀" : "☾" };
  if (code <= 2) return { label: "Partly cloudy", glyph: "⛅" };
  if (code === 3) return { label: "Cloudy", glyph: "☁" };
  if (code === 45 || code === 48) return { label: "Fog", glyph: "≋" };
  if (code >= 51 && code <= 57) return { label: "Drizzle", glyph: "☂" };
  if ((code >= 61 && code <= 67) || (code >= 80 && code <= 82)) return { label: "Rain", glyph: "☂" };
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return { label: "Snow", glyph: "❄" };
  if (code >= 95) return { label: "Thunderstorm", glyph: "ϟ" };
  return { label: "—", glyph: "·" };
}

function cached(city: string, now: number): Weather | null {
  try {
    const value = JSON.parse(localStorage.getItem(CACHE_KEY) ?? "null") as Weather | null;
    if (value && value.city === city && now - value.at < FRESH_MS && Number.isFinite(value.temperature)) return value;
  } catch { /* storage unavailable or old format */ }
  return null;
}

export async function loadWeather(city: string, fetcher: typeof fetch = fetch, now = Date.now()): Promise<Weather | null> {
  const name = city.trim();
  if (name.length < 2 || name.length > 80) return null;
  const hit = cached(name, now);
  if (hit) return hit;
  const place = await fetcher(`https://geocoding-api.open-meteo.com/v1/search?count=1&name=${encodeURIComponent(name)}`).then((r) => r.json());
  const first = place?.results?.[0];
  if (!first || !Number.isFinite(first.latitude) || !Number.isFinite(first.longitude)) return null;
  const data = await fetcher(`https://api.open-meteo.com/v1/forecast?latitude=${first.latitude}&longitude=${first.longitude}&current=temperature_2m,weather_code,is_day`).then((r) => r.json());
  const current = data?.current;
  if (!current || !Number.isFinite(current.temperature_2m) || !Number.isFinite(current.weather_code)) return null;
  const weather: Weather = { city: name, temperature: Math.round(current.temperature_2m), code: current.weather_code, isDay: current.is_day !== 0, at: now };
  try { localStorage.setItem(CACHE_KEY, JSON.stringify(weather)); } catch { /* caching is optional */ }
  return weather;
}
