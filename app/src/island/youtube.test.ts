import { describe, expect, it } from "vitest";
import { youtubeId } from "./youtube";

describe("youtubeId", () => {
  it("reads every common link form", () => {
    for (const link of ["dQw4w9WgXcQ", "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1s", "youtu.be/dQw4w9WgXcQ", "https://m.youtube.com/shorts/dQw4w9WgXcQ",
      "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1", "https://music.youtube.com/watch?v=dQw4w9WgXcQ"]) expect(youtubeId(link)).toBe("dQw4w9WgXcQ");
  });
  it("ignores search text and other sites", () => {
    expect(youtubeId("lofi beats")).toBeNull();
    expect(youtubeId("https://vimeo.com/123")).toBeNull();
  });
});
