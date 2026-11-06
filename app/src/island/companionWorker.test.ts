import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import worker, { CompanionState } from '../../../companion/src/worker';

const NOW = Date.UTC(2026, 10, 4, 12);
const DAY = 86_400_000;
const KEY = 'a'.repeat(64);
const ORIGIN = 'https://sudo-personal-companion.workers.dev';
class MemoryStorage {
  data = new Map<string, unknown>();
  queue: Promise<unknown> = Promise.resolve();
  async get<T>(key: string): Promise<T | undefined> { return structuredClone(this.data.get(key)) as T | undefined; }
  async put(key: string, value: unknown) { this.data.set(key, structuredClone(value)); }
  async transaction<T>(fn: (storage: MemoryStorage) => Promise<T>): Promise<T> {
    const result = this.queue.then(async () => {
      const previous = structuredClone(this.data);
      try { return await fn(this); } catch (error) { this.data = previous; throw error; }
    });
    this.queue = result.catch(() => undefined);
    return result;
  }
}
function harness() {
  const storage = new MemoryStorage();
  const state = new CompanionState({ storage }, { DESKTOP_KEY: KEY });
  const env = { DESKTOP_KEY: KEY, COMPANION: { idFromName: () => 'one', get: () => state }, ASSETS: { fetch: async () => new Response('<html>Phone</html>', { headers: { 'Content-Type': 'text/html' } }) } };
  const request = (path: string, key: string | null = KEY, body?: unknown, origin?: string, method?: string) => worker.fetch(new Request(ORIGIN + path, {
    method: method ?? (body === undefined ? 'GET' : 'POST'),
    headers: { ...(key ? { Authorization: `Bearer ${key}` } : {}), ...(body === undefined ? {} : { 'Content-Type': 'application/json' }), ...(origin ? { Origin: origin } : {}) },
    body: body === undefined ? undefined : JSON.stringify(body),
  }), env);
  const pair = async () => {
    const response = await request('/v1/pair/new', KEY, {});
    const { pairUrl } = await response.json() as any;
    const code = new URL(pairUrl).hash.slice(6);
    return { code, claim: () => request('/v1/pair/claim', null, { code }) };
  };
  const phone = async () => (await (await (await pair()).claim()).json() as any).phoneKey as string;
  const schedule = (alarms = [{ id: 'lunch', label: 'Lunch', at: NOW + 3_600_000, time: '13:00', utcOffset: '+00:00' }]) => ({ generatedAt: NOW, coverageUntil: NOW + 7 * DAY, timeZone: 'UTC', alarms });
  return { request, pair, phone, schedule, storage };
}
beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(NOW); });
afterEach(() => vi.useRealTimers());

describe('online companion API', () => {
  it('requires capabilities, rejects foreign browser origins, and never caches API data', async () => {
    const h = harness();
    expect((await h.request('/v1/state', null)).status).toBe(401);
    expect((await h.request('/v1/state', KEY, undefined, 'https://evil.test')).status).toBe(403);
    const response = await h.request('/v1/state', KEY, undefined, ORIGIN);
    expect(response.status).toBe(200);
    expect(response.headers.get('cache-control')).toBe('no-store');
    expect(response.headers.get('access-control-allow-origin')).toBeNull();
  });
  it('claims an expiring one-use code, replaces old capabilities, and revokes pending and active keys', async () => {
    const h = harness(); const first = await h.pair();
    const key = (await (await first.claim()).json() as any).phoneKey;
    expect(key).toMatch(/^[a-f0-9]{64}$/);
    expect((await first.claim()).status).toBe(403);
    expect((await h.request('/v1/pair/new', key, {})).status).toBe(403);
    const expired = await h.pair(); vi.setSystemTime(NOW + 600_001);
    expect((await expired.claim()).status).toBe(403);
    const replacement = await h.phone();
    expect((await h.request('/v1/state', key)).status).toBe(401);
    expect((await h.request('/v1/state', replacement)).status).toBe(200);
    const pending = await h.pair(); await h.request('/v1/revoke', KEY, {});
    expect((await h.request('/v1/state', replacement)).status).toBe(401);
    expect((await pending.claim()).status).toBe(403);
  });
  it('exposes only public state and persists only hashed phone and pairing capabilities', async () => {
    const h = harness(); const pending = await h.pair(); const key = (await (await pending.claim()).json() as any).phoneKey;
    await h.request('/v1/desktop/sync', KEY, { schedule: h.schedule() });
    const value = await (await h.request('/v1/state', key)).json() as any;
    expect(Object.keys(value).sort()).toEqual(['battery','coverageUntil','focus','importedCount','inbox','lastDesktopAt','lastImportedAt','note','phonePaired','sourceUpdatedAt'].sort());
    const stored = JSON.stringify([...h.storage.data.values()]);
    expect(stored).not.toContain(key); expect(stored).not.toContain(pending.code); expect(stored).not.toContain(KEY);
    expect(value.sourceUpdatedAt).toBe(NOW); expect(value.phonePaired).toBe(true);
  });
  it('serves a fresh next-day manifest from sleeping desktop coverage, with stable owner prefix and exact ACK', async () => {
    const h = harness(); const phone = await h.phone();
    await h.request('/v1/desktop/sync', KEY, { schedule: h.schedule([
      { id:'near',label:'Near',at:NOW+60_000,time:'12:01',utcOffset:'+00:00' },
      { id:'later',label:'Later',at:NOW+2*DAY+3_600_000,time:'13:00',utcOffset:'+00:00' },
    ]) });
    vi.setSystemTime(NOW + 2 * DAY);
    expect((await h.request('/v1/alarms', KEY)).status).toBe(403);
    const manifest = await (await h.request('/v1/alarms', phone)).json() as any;
    expect(manifest).toMatchObject({ schema:'sudo.iphone-alarms.v1',alarmCount:1,sourceUpdatedAt:NOW,generatedAt:NOW+2*DAY,ackUrl:ORIGIN+'/v1/ack',utcOffset:'+00:00' });
    expect(manifest.alarms[0].label).toBe(manifest.ownerPrefix + 'Later');
    expect((await h.request('/v1/ack',phone,{revision:manifest.revision,createdCount:4})).status).toBe(409);
    expect((await h.request('/v1/ack',phone,{revision:manifest.revision,createdCount:1})).status).toBe(204);
    const state = await (await h.request('/v1/state',KEY)).json() as any;
    expect(state.importedCount).toBe(1); expect(state.lastImportedAt).toBe(NOW+2*DAY);
    const next = await (await h.request('/v1/alarms',phone)).json() as any;
    expect(next.ownerPrefix).toBe(manifest.ownerPrefix);
    vi.setSystemTime(NOW+2*DAY+600_001);
    expect((await h.request('/v1/ack',phone,{revision:manifest.revision,createdCount:1})).status).toBe(409);
  });
  it('refuses unavailable coverage and offset changes rather than silently importing an empty schedule', async () => {
    const h = harness(); const phone = await h.phone();
    expect((await h.request('/v1/alarms',phone)).status).toBe(503);
    await h.request('/v1/desktop/sync',KEY,{schedule:h.schedule([])});
    expect((await (await h.request('/v1/alarms',phone)).json() as any).alarmCount).toBe(0);
    vi.setSystemTime(NOW+7*DAY);
    expect((await h.request('/v1/alarms',phone)).status).toBe(503);
  });
  it('validates schedule clocks, timezone wall times, horizon, duplicate IDs and upload bounds', async () => {
    const h = harness();
    for (const schedule of [
      {...h.schedule(),generatedAt:NOW+600_000}, {...h.schedule(),coverageUntil:NOW+9*DAY},
      h.schedule([{id:'a',label:'A',at:NOW+3_600_000,time:'14:00',utcOffset:'+00:00'}]),
      h.schedule([{id:'a',label:'A',at:NOW+3_600_000,time:'14:00',utcOffset:'+01:00'}]),
      h.schedule([h.schedule().alarms[0],h.schedule().alarms[0]]),
    ]) expect((await h.request('/v1/desktop/sync',KEY,{schedule})).status).toBe(400);
    expect((await h.request('/v1/desktop/sync',KEY,{schedule:h.schedule(),pad:'x'.repeat(524_288)})).status).toBe(413);
  });
  it('merges note and focus revisions without stale desktop overwrites or implausible clocks', async () => {
    const h = harness(); const phone = await h.phone();
    const first = await (await h.request('/v1/action',phone,{type:'note',text:'From phone'})).json() as any;
    const state = await (await h.request('/v1/desktop/sync',KEY,{schedule:h.schedule(),note:{text:'Old',updatedAt:NOW-1000}})).json() as any;
    expect(state.note).toEqual(first.note);
    expect((await h.request('/v1/desktop/sync',KEY,{schedule:h.schedule(),note:{text:'Future',updatedAt:NOW+600_000}})).status).toBe(400);
    const started = await (await h.request('/v1/action',phone,{type:'focus',action:'start',minutes:5})).json() as any;
    expect(started.focus.deadline).toBe(NOW+300_000); expect(started.focus.remaining).toBe(300_000);
    vi.setSystemTime(NOW+90_000);
    const paused = await (await h.request('/v1/action',phone,{type:'focus',action:'pause'})).json() as any;
    expect(paused.focus.remaining).toBe(210_000); expect(paused.focus.deadline).toBeNull();
    expect((await h.request('/v1/action',phone,{type:'focus',action:'start',minutes:181})).status).toBe(400);
  });
  it('bounds the inbox, supports dismissal, and accepts validated battery reports from phone only', async () => {
    const h = harness(); const phone = await h.phone();
    expect((await h.request('/v1/action',KEY,{type:'battery',percent:70,charging:true})).status).toBe(403);
    expect((await h.request('/v1/action',phone,{type:'battery',percent:101,charging:true})).status).toBe(400);
    let response = await h.request('/v1/action',phone,{type:'battery',percent:70,charging:true});
    expect((await response.json() as any).battery).toEqual({percent:70,charging:true,updatedAt:NOW});
    expect((await h.request('/v1/action',phone,{type:'send',text:'x'.repeat(2001)})).status).toBe(400);
    for(let i=0;i<50;i++) response=await h.request('/v1/action',phone,{type:'send',text:`Item ${i}`});
    const full = await response.json() as any;
    expect(full.inbox).toHaveLength(50);
    expect((await h.request('/v1/action',phone,{type:'send',text:'Overflow'})).status).toBe(409);
    await h.request('/v1/action',KEY,{type:'dismiss',id:full.inbox[0].id});
    expect((await (await h.request('/v1/state',KEY)).json() as any).inbox).toHaveLength(49);
    expect((await h.request('/v1/action',phone,{type:'execute',text:'calc.exe'})).status).toBe(400);
  });
  it('serves secured assets but never falls back to HTML for unknown API endpoints', async () => {
    const h=harness(); const asset=await h.request('/',null);
    expect(asset.headers.get('content-security-policy')).toContain("script-src 'self'");
    expect(asset.headers.get('referrer-policy')).toBe('no-referrer');
    expect((await h.request('/v1/missing',KEY)).status).toBe(404);
    expect((await h.request('/v1/state?token=bad',KEY)).status).toBe(400);
  });
  it('limits rejected pairing guesses even when storage rolls back throwing transactions', async () => {
    const h=harness();
    for(let i=0;i<12;i++) expect((await h.request('/v1/pair/claim',null,{code:'0'.repeat(64)})).status).toBe(403);
    expect((await h.request('/v1/pair/claim',null,{code:'0'.repeat(64)})).status).toBe(429);
    vi.setSystemTime(NOW+60_000);
    expect((await h.request('/v1/pair/claim',null,{code:'0'.repeat(64)})).status).toBe(403);
  });
  it('rejects a next-day alarm with a daylight-saving offset different from the current offset', async () => {
    const h=harness(); const phone=await h.phone();
    const beforeChange=Date.UTC(2026,9,24,12); vi.setSystemTime(beforeChange);
    const schedule={generatedAt:beforeChange,coverageUntil:beforeChange+7*DAY,timeZone:'Europe/Bucharest',alarms:[{id:'dst',label:'After DST',at:Date.UTC(2026,9,25,2),time:'04:00',utcOffset:'+02:00'}]};
    expect((await h.request('/v1/desktop/sync',KEY,{schedule})).status).toBe(200);
    expect((await h.request('/v1/alarms',phone)).status).toBe(409);
    vi.setSystemTime(Date.UTC(2026,9,25,1));
    expect((await (await h.request('/v1/alarms',phone)).json() as any).utcOffset).toBe('+02:00');
  });
  it('enforces desktop and phone roles and preserves concurrent inbox writes atomically', async () => {
    const h=harness(); const phone=await h.phone();
    expect((await h.request('/v1/desktop/sync',phone,{schedule:h.schedule()})).status).toBe(403);
    expect((await h.request('/v1/revoke',phone,{})).status).toBe(403);
    expect((await h.request('/v1/ack',KEY,{revision:'a',createdCount:0})).status).toBe(403);
    await Promise.all(Array.from({length:20},(_,i)=>h.request('/v1/action',phone,{type:'send',text:`Concurrent ${i}`})));
    const state=await (await h.request('/v1/state',KEY)).json() as any;
    expect(state.inbox).toHaveLength(20);
    expect(new Set(state.inbox.map((item:any)=>item.text)).size).toBe(20);
  });
});
