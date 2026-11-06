/** Single-person companion. Capabilities are never stored in plaintext or put in URLs. */
const DAY = 86_400_000;
const BODY_LIMIT = 512 * 1024;
const TEN_MINUTES = 600_000;
type Note = { text: string; updatedAt: number };
type Focus = { remaining: number; deadline: number | null; minutes: number; updatedAt: number };
type Alarm = { id: string; label: string; at: number; time: string; utcOffset: string };
type Schedule = { generatedAt: number; coverageUntil: number; timeZone: string; alarms: Alarm[] };
type Data = {
  note: Note; focus: Focus; inbox: { id: string; text: string; createdAt: number }[];
  battery: { percent: number; charging: boolean; updatedAt: number } | null;
  schedule: Schedule | null; lastDesktopAt: number | null; lastImportedAt: number | null; importedCount: number | null;
  phoneHash: string | null; pairing: { hash: string; expiresAt: number } | null; ownerPrefix: string;
  served: { revision: string; count: number; at: number; phoneHash: string }[];
  limit: { at: number; requests: number; claims: number };
};
interface Storage {
  get<T>(key: string): Promise<T | undefined>;
  put(key: string, value: unknown): Promise<unknown>;
  transaction<T>(fn: (storage: Storage) => Promise<T>): Promise<T>;
}
interface State { storage: Storage }
interface Env {
  DESKTOP_KEY: string;
  COMPANION?: { idFromName(name: string): unknown; get(id: unknown): { fetch(request: Request): Promise<Response> } };
  ASSETS?: { fetch(request: Request): Promise<Response> };
}
class ApiError extends Error { constructor(public status: number, message: string) { super(message); } }
function fail(status: number, message: string): never { throw new ApiError(status, message); }
const security = {
  'Cache-Control': 'no-store', 'Referrer-Policy': 'no-referrer', 'X-Content-Type-Options': 'nosniff',
  'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; manifest-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
  'Permissions-Policy': 'camera=(), microphone=(), geolocation=()',
  'Strict-Transport-Security': 'max-age=31536000',
};
function json(value: unknown, status = 200): Response { return new Response(status === 204 ? null : JSON.stringify(value), { status, headers: { ...security, 'Content-Type': 'application/json; charset=utf-8' } }); }
function errorResponse(error: unknown): Response { return error instanceof ApiError ? json({ error: error.message }, error.status) : json({ error: 'Companion service unavailable.' }, 503); }
function object(value: unknown): Record<string, any> { if (!value || typeof value !== 'object' || Array.isArray(value)) fail(400, 'Expected a JSON object.'); return value as Record<string, any>; }
function exact(value: Record<string, any>, allowed: string[]) { if (Object.keys(value).some(key => !allowed.includes(key))) fail(400, 'Unexpected field.'); }
function integer(value: unknown, min: number, max: number): value is number { return typeof value === 'number' && Number.isSafeInteger(value) && value >= min && value <= max; }
function text(value: unknown, max = 2000, empty = true): string { if (typeof value !== 'string' || [...value].length > max || (!empty && !value.trim()) || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(value)) fail(400, 'Invalid text.'); return value; }
function random(): string { return [...crypto.getRandomValues(new Uint8Array(32))].map(v => v.toString(16).padStart(2, '0')).join(''); }
async function hash(value: string): Promise<string> { return [...new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value)))].map(v => v.toString(16).padStart(2, '0')).join(''); }
function equal(a: string, b: string): boolean { if (a.length !== b.length) return false; let difference = 0; for (let i = 0; i < a.length; i++) difference |= a.charCodeAt(i) ^ b.charCodeAt(i); return difference === 0; }
function validRequest(request: Request): URL {
  const url = new URL(request.url);
  if (request.headers.has('Origin') && request.headers.get('Origin') !== url.origin) fail(403, 'Browser origin is not allowed.');
  if (url.search || url.username || url.password) fail(400, 'Query parameters are not supported.');
  if (url.pathname.length > 100) fail(404, 'Unknown endpoint.');
  return url;
}
async function body(request: Request): Promise<Record<string, any>> {
  if (!request.headers.get('Content-Type')?.toLowerCase().startsWith('application/json')) fail(400, 'Use application/json.');
  const declared = request.headers.get('Content-Length');
  if (declared && (!/^\d+$/u.test(declared) || Number(declared) > BODY_LIMIT)) fail(413, 'Request body is too large.');
  if (!request.body) fail(400, 'JSON body is required.');
  const reader = request.body.getReader(); const chunks: Uint8Array[] = []; let length = 0;
  while (true) { const next = await reader.read(); if (next.done) break; length += next.value.byteLength; if (length > BODY_LIMIT) { await reader.cancel(); fail(413, 'Request body is too large.'); } chunks.push(next.value); }
  const bytes = new Uint8Array(length); let position = 0; for (const chunk of chunks) { bytes.set(chunk, position); position += chunk.length; }
  let value: unknown; try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); } catch { fail(400, 'Invalid JSON.'); }
  return object(value);
}
function timestamp(value: unknown, now: number): number { if (!integer(value, Math.max(0, now - 8 * DAY), now + 5000)) fail(400, 'Clock is implausible. Check the desktop date and time.'); return value; }
function utcOffset(timeZone: string, at: number): string {
  const raw = new Intl.DateTimeFormat('en-US', { timeZone, timeZoneName: 'longOffset' }).formatToParts(at).find(part => part.type === 'timeZoneName')?.value;
  if (raw === 'GMT') return '+00:00';
  if (!raw || !/^GMT[+-]\d{2}:\d{2}$/u.test(raw)) fail(400, 'Unsupported timezone.');
  return raw.slice(3);
}
function offsetMinutes(value: string): number { const result = /^([+-])(\d{2}):(\d{2})$/u.exec(value); if (!result || +result[2] > 14 || +result[3] > 59 || (+result[2] === 14 && +result[3] !== 0)) fail(400, 'Invalid UTC offset.'); return (+result[2] * 60 + +result[3]) * (result[1] === '-' ? -1 : 1); }
function validateSchedule(value: unknown, now: number): Schedule {
  const s = object(value); exact(s, ['generatedAt', 'coverageUntil', 'timeZone', 'alarms']);
  timestamp(s.generatedAt, now);
  if (s.generatedAt < now - 300_000 || !integer(s.coverageUntil, now + DAY, s.generatedAt + 8 * DAY)) fail(400, 'A fresh schedule with at least 24 hours of coverage is required.');
  if (typeof s.timeZone !== 'string' || s.timeZone.length > 100) fail(400, 'Invalid timezone.');
  try { utcOffset(s.timeZone, now); } catch { fail(400, 'Invalid timezone.'); }
  if (!Array.isArray(s.alarms) || s.alarms.length > 2000) fail(400, 'Too many alarms.');
  const ids = new Set<string>();
  for (const raw of s.alarms) {
    const a = object(raw); exact(a, ['id', 'label', 'at', 'time', 'utcOffset']);
    text(a.id, 160, false); text(a.label, 180, false);
    if (ids.has(a.id) || !integer(a.at, s.generatedAt - DAY, s.coverageUntil) || a.at % 60_000 !== 0 || typeof a.utcOffset !== 'string') fail(400, 'Invalid alarm entry.');
    ids.add(a.id); const minutes = offsetMinutes(a.utcOffset); const wall = ((a.at / 60_000 + minutes) % 1440 + 1440) % 1440;
    const time = `${String(Math.floor(wall / 60)).padStart(2, '0')}:${String(wall % 60).padStart(2, '0')}`;
    if (a.time !== time || a.utcOffset !== utcOffset(s.timeZone, a.at)) fail(400, 'Alarm wall time does not match its timezone.');
  }
  return s as Schedule;
}
function validateNote(value: unknown, now: number): Note { const note = object(value); exact(note, ['text', 'updatedAt']); return { text: text(note.text), updatedAt: timestamp(note.updatedAt, now) }; }
function validateFocus(value: unknown, now: number): Focus {
  const f = object(value); exact(f, ['remaining', 'deadline', 'minutes', 'updatedAt']);
  const updatedAt = timestamp(f.updatedAt, now);
  if (!integer(f.minutes, 1, 180) || typeof f.remaining !== 'number' || !Number.isFinite(f.remaining) || f.remaining < 0 || f.remaining > 10_800_000 || (f.deadline !== null && !integer(f.deadline, now - 8 * DAY, now + 180 * 60_000 + 5000))) fail(400, 'Invalid focus timer.');
  return { remaining: f.remaining, deadline: f.deadline, minutes: f.minutes, updatedAt };
}
function publicState(data: Data) { return { note: data.note, focus: data.focus, inbox: data.inbox, battery: data.battery, lastDesktopAt: data.lastDesktopAt, coverageUntil: data.schedule?.coverageUntil ?? null, sourceUpdatedAt: data.schedule?.generatedAt ?? null, phonePaired: !!data.phoneHash, lastImportedAt: data.lastImportedAt, importedCount: data.importedCount }; }

export class CompanionState {
  constructor(private state: State, private env: Env) {}
  async fetch(request: Request): Promise<Response> {
    try {
      const url = validRequest(request);
      if (!/^[a-f0-9]{64}$/u.test(this.env.DESKTOP_KEY ?? '')) fail(503, 'Desktop secret is not configured.');
      const routes: Record<string, string> = { '/v1/state': 'GET', '/v1/alarms': 'GET', '/v1/desktop/sync': 'POST', '/v1/pair/new': 'POST', '/v1/pair/claim': 'POST', '/v1/revoke': 'POST', '/v1/ack': 'POST', '/v1/action': 'POST' };
      if (!routes[url.pathname]) fail(404, 'Unknown endpoint.');
      if (routes[url.pathname] !== request.method) fail(405, 'Method is not allowed.');
      const input = request.method === 'POST' ? await body(request) : {};
      if (request.method === 'GET' && request.body) fail(400, 'GET body is not supported.');
      // Every read-modify-write (including pairing, rate limits and ACK) is atomic.
      return await this.state.storage.transaction(async storage => {
        const now = Date.now();
        const data: Data = await storage.get<Data>('state') ?? {
          note: { text: '', updatedAt: 0 }, focus: { remaining: 0, deadline: null, minutes: 25, updatedAt: 0 }, inbox: [], battery: null,
          schedule: null, lastDesktopAt: null, lastImportedAt: null, importedCount: null, phoneHash: null, pairing: null,
          ownerPrefix: `Sudo ${(await hash(this.env.DESKTOP_KEY)).slice(0, 8)} · `, served: [], limit: { at: now, requests: 0, claims: 0 },
        };
        if (now - data.limit.at >= 60_000) data.limit = { at: now, requests: 0, claims: 0 };
        if (++data.limit.requests > 240 || (url.pathname === '/v1/pair/claim' && ++data.limit.claims > 12)) { await storage.put('state', data); return json({ error: 'Too many requests. Try again in a minute.' }, 429); }
        // Persist rate accounting for rejected claims too. The following operations mutate only after validation.
        await storage.put('state', data);
        try {
        const auth = request.headers.get('Authorization') ?? '';
        const token = /^Bearer ([a-f0-9]{64})$/u.exec(auth)?.[1];
        const role = token && equal(token, this.env.DESKTOP_KEY) ? 'desktop' : token && data.phoneHash && equal(await hash(token), data.phoneHash) ? 'phone' : null;
        const desktop = () => { if (role !== 'desktop') fail(403, 'Desktop capability required.'); };
        const phone = () => { if (role !== 'phone') fail(403, 'Phone capability required.'); };
        if (url.pathname !== '/v1/pair/claim' && !role) fail(401, 'Valid capability required.');
        let response: Response;
        switch (url.pathname) {
          case '/v1/state': response = json(publicState(data)); break;
          case '/v1/pair/new': {
            desktop(); exact(input, []); const code = random(); const expiresAt = now + TEN_MINUTES;
            data.pairing = { hash: await hash(code), expiresAt };
            response = json({ pairUrl: `${url.origin}/#pair=${code}`, expiresAt }); break;
          }
          case '/v1/pair/claim': {
            exact(input, ['code']);
            if (typeof input.code !== 'string' || !/^[a-f0-9]{64}$/u.test(input.code) || !data.pairing || data.pairing.expiresAt <= now || !equal(await hash(input.code), data.pairing.hash)) fail(403, 'Pairing code is invalid or expired.');
            const phoneKey = random(); data.phoneHash = await hash(phoneKey); data.pairing = null; data.served = []; data.battery = null; data.lastImportedAt = null; data.importedCount = null;
            response = json({ phoneKey }); break;
          }
          case '/v1/revoke': desktop(); exact(input, []); data.phoneHash = null; data.pairing = null; data.served = []; data.battery = null; data.lastImportedAt = null; data.importedCount = null; response = json(publicState(data)); break;
          case '/v1/desktop/sync': {
            desktop(); exact(input, ['schedule', 'note', 'focus']);
            const schedule = validateSchedule(input.schedule, now);
            const note = input.note === undefined ? undefined : validateNote(input.note, now);
            const focus = input.focus === undefined ? undefined : validateFocus(input.focus, now);
            if (!data.schedule || schedule.generatedAt >= data.schedule.generatedAt) data.schedule = schedule;
            if (note && note.updatedAt > data.note.updatedAt) data.note = note;
            if (focus && focus.updatedAt > data.focus.updatedAt) data.focus = focus;
            data.lastDesktopAt = now; response = json(publicState(data)); break;
          }
          case '/v1/action': {
            switch (input.type) {
              case 'note': exact(input, ['type', 'text']); data.note = { text: text(input.text), updatedAt: Math.max(now, data.note.updatedAt + 1) }; break;
              case 'focus': {
                exact(input, ['type', 'action', 'minutes']);
                if (!['start', 'pause', 'reset'].includes(input.action) || (input.minutes !== undefined && !integer(input.minutes, 1, 180))) fail(400, 'Invalid focus action.');
                const f = data.focus; const minutes = input.minutes ?? f.minutes;
                const current = f.deadline === null ? f.remaining : Math.max(0, f.deadline - now);
                const remaining = input.action === 'reset' ? minutes * 60_000 : input.action === 'start' ? (input.minutes !== undefined || current <= 0 ? minutes * 60_000 : current) : current;
                data.focus = { minutes, remaining, deadline: input.action === 'start' ? now + remaining : null, updatedAt: Math.max(now, f.updatedAt + 1) }; break;
              }
              case 'send': {
                exact(input, ['type', 'text']); const value = text(input.text, 2000, false); if (data.inbox.length >= 50) fail(409, 'Inbox is full. Dismiss an item on desktop.');
                data.inbox.push({ id: random(), text: value, createdAt: now }); break;
              }
              case 'dismiss': exact(input, ['type', 'id']); text(input.id, 64, false); data.inbox = data.inbox.filter(item => item.id !== input.id); break;
              case 'battery': phone(); exact(input, ['type', 'percent', 'charging']); if (!integer(input.percent, 0, 100) || typeof input.charging !== 'boolean') fail(400, 'Invalid battery report.'); data.battery = { percent: input.percent, charging: input.charging, updatedAt: now }; break;
              default: fail(400, 'Unknown action.');
            }
            if (role === 'desktop') data.lastDesktopAt = now;
            response = json(publicState(data)); break;
          }
          case '/v1/alarms': {
            phone(); const s = data.schedule;
            if (!s || s.coverageUntil < now + DAY || s.generatedAt > now + 5000) fail(503, 'Schedule coverage is unavailable. Open Sudo on the PC to refresh it.');
            const currentOffset = utcOffset(s.timeZone, now);
            if (s.alarms.some(a => a.at > now && a.at <= now + DAY && a.utcOffset !== currentOffset)) fail(409, 'UTC offset changes in the next 24 hours. Refresh after the clock change.');
            const alarms = s.alarms.filter(a => a.at > now + 60_000 && a.at <= now + DAY).sort((a, b) => a.at - b.at).map(a => ({ label: data.ownerPrefix + a.label, time: a.time, at: a.at }));
            const manifest = { schema: 'sudo.iphone-alarms.v1', ownerPrefix: data.ownerPrefix, utcOffset: currentOffset, timeZone: s.timeZone, revision: random(), generatedAt: now, validUntil: now + 90_000, sourceUpdatedAt: s.generatedAt, coverageUntil: s.coverageUntil, alarmCount: alarms.length, alarms, ackUrl: url.origin + '/v1/ack' };
            data.served = data.served.filter(served => now - served.at <= TEN_MINUTES).slice(-15);
            data.served.push({ revision: manifest.revision, count: alarms.length, at: now, phoneHash: data.phoneHash! });
            response = json(manifest); break;
          }
          case '/v1/ack': {
            phone(); exact(input, ['revision', 'createdCount']);
            if (typeof input.revision !== 'string' || !integer(input.createdCount, 0, 2000) || !data.served.some(served => served.phoneHash === data.phoneHash && served.revision === input.revision && served.count === input.createdCount && now - served.at <= TEN_MINUTES && now >= served.at)) fail(409, 'ACK does not match a recently served import.');
            data.lastImportedAt = now; data.importedCount = input.createdCount; response = json(null, 204); break;
          }
          default: fail(404, 'Unknown endpoint.');
        }
        await storage.put('state', data);
        return response;
        } catch (error) {
          // Returning an error response commits rate accounting; throwing would roll it back.
          return errorResponse(error);
        }
      });
    } catch (error) { return errorResponse(error); }
  }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (url.pathname === '/v1' || url.pathname.startsWith('/v1/')) {
        validRequest(request);
        if (!env.COMPANION) fail(503, 'Storage is not configured.');
        return await env.COMPANION.get(env.COMPANION.idFromName('personal')).fetch(request);
      }
      if (!env.ASSETS) fail(503, 'Phone app is unavailable.');
      const response = await env.ASSETS.fetch(request); const headers = new Headers(response.headers);
      for (const [key, value] of Object.entries(security)) headers.set(key, value);
      return new Response(response.body, { status: response.status, headers });
    } catch (error) { return errorResponse(error); }
  },
};
