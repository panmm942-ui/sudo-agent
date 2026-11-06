import { beforeEach, describe, expect, it, vi } from "vitest";
import { describe as describeCode, loadWeather } from "./weather";

const store = new Map<string, string>();
beforeEach(() => {
  store.clear();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) });
});

const respond = (body: unknown) => Promise.resolve({ json: () => Promise.resolve(body) } as Response);

describe("weather", () => {
  it("geocodes the city, reads the current weather and caches it", async () => {
    const fetcher = vi.fn((url: string) => url.includes("geocoding")
      ? respond({ results: [{ latitude: 37.98, longitude: 23.72 }] })
      : respond({ current: { temperature_2m: 21.6, weather_code: 2, is_day: 1 } })) as unknown as typeof fetch;
    const first = await loadWeather("Athens", fetcher, 1000);
    expect(first).toMatchObject({ city: "Athens", temperature: 22, code: 2, isDay: true });
    await loadWeather("Athens", fetcher, 2000);
    expect(fetcher).toHaveBeenCalledTimes(2); // second call came from the 30-minute cache
  });
  it("returns nothing for unknown places or no city", async () => {
    const fetcher = vi.fn(() => respond({ results: [] })) as unknown as typeof fetch;
    expect(await loadWeather("Nowhereville", fetcher)).toBeNull();
    expect(await loadWeather(" ", fetcher)).toBeNull();
  });
  it("labels weather codes", () => {
    expect(describeCode(0).label).toBe("Clear");
    expect(describeCode(63).label).toBe("Rain");
    expect(describeCode(95).label).toBe("Thunderstorm");
  });
});
