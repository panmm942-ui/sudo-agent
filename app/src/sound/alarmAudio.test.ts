import { afterEach, describe, expect, it, vi } from "vitest";
import { createAlarmAudio } from "./alarmAudio";
import { sfx } from "./sfx";

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });
describe("gentle alarm start", () => {
  it("starts custom sound silently, ramps to chosen volume and cancels on stop", async () => {
    vi.useFakeTimers();
    const player={loop:false,volume:1,play:vi.fn().mockResolvedValue(undefined),pause:vi.fn()};
    vi.stubGlobal("Audio",vi.fn(function(){return player;}));
    const audio=createAlarmAudio();
    audio.play({tone:"custom",volume:80,alarmFade:true},"blob:alarm");
    expect(player.volume).toBe(0);
    await vi.advanceTimersByTimeAsync(5000);
    expect(player.volume).toBeGreaterThan(0);expect(player.volume).toBeLessThan(.8);
    await vi.advanceTimersByTimeAsync(5500);expect(player.volume).toBeCloseTo(.8);
    audio.stop();expect(player.pause).toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });
  it("applies fade to built-in and custom failure fallback; off retains chosen volume", async () => {
    vi.useFakeTimers();const play=vi.spyOn(sfx,"play").mockImplementation(()=>{});
    const builtIn={loop:false,volume:1,play:vi.fn().mockResolvedValue(undefined),pause:vi.fn()};
    const AudioMock=vi.fn(function(_url?:string){return builtIn;});vi.stubGlobal('Audio',AudioMock);
    const audio=createAlarmAudio();
    audio.play({tone:"bell",volume:60,alarmFade:true});
    expect(String(AudioMock.mock.calls[0][0])).toContain('bell.mp3');
    expect(builtIn.volume).toBe(0);
    await vi.advanceTimersByTimeAsync(5000);expect(builtIn.volume).toBeCloseTo(.3,1);
    const player={loop:false,volume:1,play:vi.fn().mockRejectedValue(Error()),pause:vi.fn()};
    vi.stubGlobal("Audio",vi.fn(function(){return player;}));
    audio.play({tone:"custom",volume:80,alarmFade:false},"blob:bad");
    expect(player.volume).toBe(.8);await vi.advanceTimersByTimeAsync(1);
    expect(play).toHaveBeenLastCalledWith("alarm",expect.objectContaining({tone:"chime",fadeSeconds:0}));
    audio.stop();
  });
});
