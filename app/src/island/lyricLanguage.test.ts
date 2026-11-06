import { describe, expect, it, vi } from "vitest";
import { targetLanguage, translateLine, romanize } from "./lyricLanguage";
describe("lyric language options", () => {
  it('uses readable Russian kh and ts without changing the original punctuation',async()=>{
    expect(await romanize('Хочу тебя целовала, мой мармеладный!')).toBe('Khochu tebya tselovala, moy marmeladnyy!');
  });
  it('does not publish romanization after its reader is cancelled',async()=>{
    const controller=new AbortController();
    const request=romanize('Целовала',controller.signal);
    controller.abort();
    await expect(request).rejects.toMatchObject({name:'AbortError'});
  });
  it('does not return a cached translation to a cancelled reader',async()=>{
    const get=vi.fn(async()=>({ok:true,json:async()=>({responseStatus:200,responseData:{translatedText:'Cached translation'}})}) as Response);
    await translateLine('Cancellation guarded lyric','fr',new AbortController().signal,get);
    const controller=new AbortController();controller.abort();
    await expect(translateLine('Cancellation guarded lyric','fr',controller.signal,get)).rejects.toMatchObject({name:'AbortError'});
  });
  it("uses the user's locale unless overridden, and romanizes without a service", async () => {
    expect(targetLanguage("auto","el-GR")).toBe("el");
    expect(targetLanguage("fr","el-GR")).toBe("fr");
    expect(await romanize("Καλησπέρα")).toBe("Kalispera");
  });
  it("caches a translated line and surfaces provider quota errors", async () => {
    const get=vi.fn().mockResolvedValue({ok:true,json:async()=>({responseStatus:200,responseData:{translatedText:"Test translation"}})});
    const signal=new AbortController().signal;
    expect(await translateLine("Unique test lyric","el",signal,get)).toBe("Test translation");
    await translateLine("Unique test lyric","el",signal,get);expect(get).toHaveBeenCalledTimes(1);
    get.mockResolvedValue({ok:true,json:async()=>({responseStatus:429,quotaFinished:true})});
    await expect(translateLine("Other test lyric","el",signal,get)).rejects.toThrow(/limit/);
  });
});
