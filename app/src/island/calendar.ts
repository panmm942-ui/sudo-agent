/**
 * iCalendar (.ics) reading for subscribed calendars: events with their time zones, all-day events,
 * and repeating events (daily/weekly/monthly/yearly with the usual rule parts, exceptions and
 * moved occurrences), expanded within a time window.
 */
export type CalendarEvent = {
  uid: string;
  title: string;
  location: string;
  start: number;
  end: number;
  allDay: boolean;
  calendar?: string;
};

type Prop = { name: string; params: Record<string, string>; value: string };
type Wall = { y: number; m: number; d: number; h: number; mi: number; s: number };
type Block = { type: string; props: Prop[]; children: Block[] };

/** Windows time-zone names Outlook writes, for the zones people most often use. */
const WINDOWS_ZONES: Record<string, string> = {
  "GTB Standard Time": "Europe/Athens", "E. Europe Standard Time": "Europe/Chisinau", "FLE Standard Time": "Europe/Kiev",
  "W. Europe Standard Time": "Europe/Berlin", "Romance Standard Time": "Europe/Paris", "Central Europe Standard Time": "Europe/Budapest",
  "Central European Standard Time": "Europe/Warsaw", "GMT Standard Time": "Europe/London", "Greenwich Standard Time": "Atlantic/Reykjavik",
  "Turkey Standard Time": "Europe/Istanbul", "Russian Standard Time": "Europe/Moscow", "Eastern Standard Time": "America/New_York",
  "Central Standard Time": "America/Chicago", "Mountain Standard Time": "America/Denver", "Pacific Standard Time": "America/Los_Angeles",
  "US Mountain Standard Time": "America/Phoenix", "Atlantic Standard Time": "America/Halifax", "E. South America Standard Time": "America/Sao_Paulo",
  "India Standard Time": "Asia/Kolkata", "China Standard Time": "Asia/Shanghai", "Tokyo Standard Time": "Asia/Tokyo",
  "Korea Standard Time": "Asia/Seoul", "Singapore Standard Time": "Asia/Singapore", "AUS Eastern Standard Time": "Australia/Sydney",
  "Arabian Standard Time": "Asia/Dubai", "Israel Standard Time": "Asia/Jerusalem", "Egypt Standard Time": "Africa/Cairo",
  "South Africa Standard Time": "Africa/Johannesburg", "New Zealand Standard Time": "Pacific/Auckland", "UTC": "UTC",
};

function unfold(text: string): string[] {
  return text.replace(/\r\n/g, "\n").replace(/\n[ \t]/g, "").split("\n").filter(Boolean);
}

function parseLine(line: string): Prop | null {
  // NAME;PARAM=a;PARAM2="b:c":VALUE — the value starts at the first colon outside quotes.
  let quoted = false, split = -1;
  for (let i = 0; i < line.length; i++) {
    if (line[i] === '"') quoted = !quoted;
    else if (line[i] === ":" && !quoted) { split = i; break; }
  }
  if (split < 0) return null;
  const [name, ...rest] = line.slice(0, split).split(";");
  const params: Record<string, string> = {};
  for (const part of rest) {
    const eq = part.indexOf("=");
    if (eq > 0) params[part.slice(0, eq).toUpperCase()] = part.slice(eq + 1).replace(/^"|"$/g, "");
  }
  return { name: name.toUpperCase(), params, value: line.slice(split + 1) };
}

function blocks(lines: string[]): Block {
  const root: Block = { type: "ROOT", props: [], children: [] };
  const stack = [root];
  for (const line of lines) {
    const prop = parseLine(line);
    if (!prop) continue;
    const top = stack[stack.length - 1];
    if (prop.name === "BEGIN") { const block = { type: prop.value.toUpperCase(), props: [], children: [] }; top.children.push(block); stack.push(block); }
    else if (prop.name === "END") { if (stack.length > 1) stack.pop(); }
    else top.props.push(prop);
  }
  return root;
}

const unescape = (text: string) => text.replace(/\\n/gi, "\n").replace(/\\([,;\\])/g, "$1").trim();
const get = (b: Block, name: string) => b.props.find((p) => p.name === name);

function parseWall(value: string): (Wall & { utc: boolean; date: boolean }) | null {
  const m = /^(\d{4})(\d{2})(\d{2})(?:T(\d{2})(\d{2})(\d{2})?(Z)?)?$/.exec(value.trim());
  if (!m) return null;
  return { y: +m[1], m: +m[2], d: +m[3], h: +(m[4] ?? 0), mi: +(m[5] ?? 0), s: +(m[6] ?? 0), utc: !!m[7], date: m[4] === undefined };
}

const formatters = new Map<string, Intl.DateTimeFormat>();
/** Offset (ms) of `zone` at instant `at`: local wall clock minus UTC. */
function zoneOffset(at: number, zone: string): number {
  let f = formatters.get(zone);
  if (!f) {
    f = new Intl.DateTimeFormat("en-US", { timeZone: zone, hourCycle: "h23", year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit" });
    formatters.set(zone, f);
  }
  const parts: Record<string, number> = {};
  for (const p of f.formatToParts(new Date(at))) if (p.type !== "literal") parts[p.type] = +p.value;
  return Date.UTC(parts.year, parts.month - 1, parts.day, parts.hour % 24, parts.minute, parts.second) - Math.floor(at / 1000) * 1000;
}

function validZone(zone: string): boolean {
  try { new Intl.DateTimeFormat("en-US", { timeZone: zone }); return true; } catch { return false; }
}

/** A VTIMEZONE's standard/daylight offsets and switch rules, for zones Intl doesn't know. */
type Rule = { offset: number; month: number; nth: number; weekday: number; h: number; mi: number };
function vtimezone(block: Block): { standard: Rule; daylight?: Rule } | null {
  const rule = (b: Block | undefined): Rule | null => {
    if (!b) return null;
    const to = /^([+-])(\d{2})(\d{2})/.exec(get(b, "TZOFFSETTO")?.value ?? "");
    if (!to) return null;
    const offset = (to[1] === "-" ? -1 : 1) * (+to[2] * 60 + +to[3]) * 60_000;
    const start = parseWall(get(b, "DTSTART")?.value ?? "") ?? { y: 1970, m: 1, d: 1, h: 0, mi: 0, s: 0 };
    const rrule = Object.fromEntries((get(b, "RRULE")?.value ?? "").split(";").map((p) => p.split("=")));
    const byday = /^(-?\d)?(SU|MO|TU|WE|TH|FR|SA)$/.exec(rrule.BYDAY ?? "");
    return { offset, month: +(rrule.BYMONTH ?? start.m), nth: byday ? +(byday[1] ?? 1) : 0, weekday: byday ? DAYS.indexOf(byday[2]) : -1, h: start.h, mi: start.mi };
  };
  const standard = rule(block.children.find((c) => c.type === "STANDARD"));
  if (!standard) return null;
  return { standard, daylight: rule(block.children.find((c) => c.type === "DAYLIGHT")) ?? undefined };
}

const DAYS = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"];

/** Day of month of the nth (or, negative, nth-from-last) `weekday` in a month. */
function nthWeekday(y: number, m: number, weekday: number, nth: number): number {
  const days = new Date(Date.UTC(y, m, 0)).getUTCDate();
  if (nth > 0) { const first = new Date(Date.UTC(y, m - 1, 1)).getUTCDay(); return 1 + ((weekday - first + 7) % 7) + (nth - 1) * 7; }
  const last = new Date(Date.UTC(y, m - 1, days)).getUTCDay();
  return days - ((last - weekday + 7) % 7) + (nth + 1) * 7;
}

type Zone = { iana?: string; custom?: { standard: Rule; daylight?: Rule } };

function toInstant(w: Wall, zone: Zone | null): number {
  const guess = Date.UTC(w.y, w.m - 1, w.d, w.h, w.mi, w.s);
  if (zone?.iana) {
    let at = guess - zoneOffset(guess, zone.iana);
    at = guess - zoneOffset(at, zone.iana);
    return at;
  }
  if (zone?.custom) {
    const { standard, daylight } = zone.custom;
    if (!daylight || daylight.weekday < 0 || standard.weekday < 0) return guess - standard.offset;
    const switchAt = (r: Rule) => Date.UTC(w.y, r.month - 1, nthWeekday(w.y, r.month, r.weekday, r.nth), r.h, r.mi);
    const summer = switchAt(daylight), winter = switchAt(standard);
    const inDaylight = summer < winter ? guess >= summer && guess < winter : guess >= summer || guess < winter;
    return guess - (inDaylight ? daylight.offset : standard.offset);
  }
  // Floating time: the PC's own time zone.
  return new Date(w.y, w.m - 1, w.d, w.h, w.mi, w.s).getTime();
}

function resolveZone(tzid: string | undefined, zones: Map<string, Zone>): Zone | null {
  if (!tzid) return null;
  const known = zones.get(tzid);
  if (known) return known;
  const windows = WINDOWS_ZONES[tzid];
  if (windows) return { iana: windows };
  const cleaned = tzid.replace(/^\/[^/]+\/[^/]+\//, ""); // "/mozilla.org/20050126_1/Europe/Athens"
  if (validZone(cleaned)) return { iana: cleaned };
  return null;
}

function addDays(w: Wall, days: number): Wall {
  const t = new Date(Date.UTC(w.y, w.m - 1, w.d + days));
  return { ...w, y: t.getUTCFullYear(), m: t.getUTCMonth() + 1, d: t.getUTCDate() };
}
const weekdayOf = (w: Wall) => new Date(Date.UTC(w.y, w.m - 1, w.d)).getUTCDay();
const key = (w: Wall) => `${w.y}${String(w.m).padStart(2, "0")}${String(w.d).padStart(2, "0")}T${String(w.h).padStart(2, "0")}${String(w.mi).padStart(2, "0")}`;

/** Every start (wall time, event zone) of a repeating event, in order, until `limit` instants. */
function* occurrences(start: Wall, rule: string, until: number, toUtc: (w: Wall) => number): Generator<Wall> {
  const r = Object.fromEntries(rule.split(";").map((p) => p.split("=")).filter((p) => p.length === 2).map(([k, v]) => [k.toUpperCase(), v]));
  const freq = r.FREQ, interval = Math.max(1, +(r.INTERVAL ?? 1) || 1);
  const count = r.COUNT ? +r.COUNT : Infinity;
  const ruleUntil = r.UNTIL ? parseWall(r.UNTIL) : null;
  const end = Math.min(until, ruleUntil ? (ruleUntil.utc ? Date.UTC(ruleUntil.y, ruleUntil.m - 1, ruleUntil.d, ruleUntil.h, ruleUntil.mi, ruleUntil.s) : toUtc({ ...ruleUntil, h: ruleUntil.date ? 23 : ruleUntil.h, mi: ruleUntil.date ? 59 : ruleUntil.mi })) : Infinity);
  const byday = (r.BYDAY ?? "").split(",").filter(Boolean).map((d) => { const m = /^([+-]?\d{1,2})?(SU|MO|TU|WE|TH|FR|SA)$/.exec(d); return m ? { nth: m[1] ? +m[1] : 0, day: DAYS.indexOf(m[2]) } : null; }).filter((d): d is { nth: number; day: number } => !!d);
  const bymonthday = (r.BYMONTHDAY ?? "").split(",").filter(Boolean).map(Number);
  const bymonth = (r.BYMONTH ?? "").split(",").filter(Boolean).map(Number);
  let produced = 0, guard = 0;
  const emit = function* (w: Wall) {
    if (produced >= count) return;
    const at = toUtc(w);
    if (at < toUtc(start)) return;
    if (at > end) { produced = Infinity; return; }
    produced++;
    yield w;
  };
  if (freq === "DAILY") {
    for (let w = start; produced < count && guard++ < 5000; w = addDays(w, interval)) {
      if (byday.length && !byday.some((b) => b.day === weekdayOf(w))) continue;
      if (bymonth.length && !bymonth.includes(w.m)) continue;
      yield* emit(w);
    }
  } else if (freq === "WEEKLY") {
    const days = byday.length ? byday.map((b) => b.day).sort((a, b) => ((a - 1 + 7) % 7) - ((b - 1 + 7) % 7)) : [weekdayOf(start)];
    // Weeks start on Monday (WKST default).
    let monday = addDays(start, -((weekdayOf(start) + 6) % 7));
    while (produced < count && guard++ < 3000) {
      for (const day of days) yield* emit(addDays(monday, (day + 6) % 7));
      monday = addDays(monday, 7 * interval);
    }
  } else if (freq === "MONTHLY" || freq === "YEARLY") {
    let y = start.y, m = start.m;
    while (produced < count && guard++ < 1200) {
      const months = freq === "YEARLY" ? (bymonth.length ? bymonth : [start.m]) : [m];
      for (const month of months) {
        const days = new Date(Date.UTC(y, month, 0)).getUTCDate();
        let dates: number[];
        if (byday.length) dates = byday.flatMap((b) => b.nth ? [nthWeekday(y, month, b.day, b.nth)] : Array.from({ length: days }, (_, i) => i + 1).filter((d) => new Date(Date.UTC(y, month - 1, d)).getUTCDay() === b.day));
        else if (bymonthday.length) dates = bymonthday.map((d) => (d < 0 ? days + d + 1 : d));
        else dates = [start.d];
        for (const d of [...new Set(dates)].filter((d) => d >= 1 && d <= days).sort((a, b) => a - b)) yield* emit({ ...start, y, m: month, d });
      }
      if (freq === "YEARLY") y += interval;
      else { m += interval; while (m > 12) { m -= 12; y++; } }
    }
  }
}

/** Events between `from` and `to` (instants), sorted by start. */
export function parseCalendar(text: string, from: number, to: number, calendar?: string): CalendarEvent[] {
  const root = blocks(unfold(text));
  const vcal = root.children.find((b) => b.type === "VCALENDAR") ?? root;
  const zones = new Map<string, Zone>();
  for (const tz of vcal.children.filter((b) => b.type === "VTIMEZONE")) {
    const id = get(tz, "TZID")?.value;
    if (!id) continue;
    const named = resolveZone(id, new Map());
    const custom = vtimezone(tz);
    if (named) zones.set(id, named); else if (custom) zones.set(id, { custom });
  }
  const events = vcal.children.filter((b) => b.type === "VEVENT");
  // Moved or cancelled single occurrences: "uid|original start" → the replacement (or null).
  const overrides = new Map<string, Block | null>();
  for (const e of events) {
    const rid = get(e, "RECURRENCE-ID");
    const uid = get(e, "UID")?.value;
    if (rid && uid) { const w = parseWall(rid.value); if (w) overrides.set(`${uid}|${key(w)}`, get(e, "STATUS")?.value === "CANCELLED" ? null : e); }
  }
  const out: CalendarEvent[] = [];
  for (const e of events) {
    if (get(e, "STATUS")?.value === "CANCELLED") continue;
    const dtstart = get(e, "DTSTART");
    const startWall = dtstart ? parseWall(dtstart.value) : null;
    if (!dtstart || !startWall) continue;
    const allDay = startWall.date || dtstart.params.VALUE === "DATE";
    const zone = startWall.utc ? { iana: "UTC" } : resolveZone(dtstart.params.TZID, zones);
    const uid = get(e, "UID")?.value ?? `${key(startWall)}-${out.length}`;
    const title = unescape(get(e, "SUMMARY")?.value ?? "") || "Event";
    const location = unescape(get(e, "LOCATION")?.value ?? "");
    const dtend = get(e, "DTEND");
    const endWall = dtend ? parseWall(dtend.value) : null;
    // All-day events start at local midnight wherever the PC is.
    const toUtc = (w: Wall) => allDay ? new Date(w.y, w.m - 1, w.d).getTime() : toInstant(w, zone);
    const startAt = toUtc(startWall);
    let duration = allDay ? 86_400_000 : 0;
    if (endWall) {
      const endZone = endWall.utc ? { iana: "UTC" } : resolveZone(dtend!.params.TZID, zones) ?? zone;
      duration = Math.max(0, (allDay ? new Date(endWall.y, endWall.m - 1, endWall.d).getTime() : toInstant(endWall, endZone)) - startAt);
    } else {
      const d = /^P(?:(\d+)W)?(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?)?$/.exec(get(e, "DURATION")?.value ?? "");
      if (d) duration = (((+(d[1] ?? 0) * 7 + +(d[2] ?? 0)) * 24 + +(d[3] ?? 0)) * 60 + +(d[4] ?? 0)) * 60_000 + +(d[5] ?? 0) * 1000;
    }
    const isOverride = !!get(e, "RECURRENCE-ID");
    const rrule = get(e, "RRULE")?.value;
    const exdates = new Set(e.props.filter((p) => p.name === "EXDATE").flatMap((p) => p.value.split(",")).map(parseWall).filter((w): w is NonNullable<typeof w> => !!w).map(key));
    const push = (at: number) => { if (at + Math.max(duration, 1) > from && at < to) out.push({ uid, title, location, start: at, end: at + duration, allDay, calendar }); };
    if (!rrule || isOverride) { push(startAt); continue; }
    for (const w of occurrences(startWall, rrule, to, toUtc)) {
      const k = key(w);
      if (exdates.has(k) || overrides.has(`${uid}|${k}`)) continue;
      push(toUtc(w));
    }
  }
  return out.sort((a, b) => a.start - b.start);
}

/** "09:30", "Tomorrow 09:30", "Fri 09:30", or "All day". */
export function eventTime(event: CalendarEvent, now: number): string {
  const start = new Date(event.start), today = new Date(now);
  const sameDay = (a: Date, b: Date) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
  const clock = event.allDay ? "All day" : start.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (sameDay(start, today)) return clock;
  const tomorrow = new Date(today); tomorrow.setDate(today.getDate() + 1);
  if (sameDay(start, tomorrow)) return `Tomorrow ${clock}`;
  return `${start.toLocaleDateString([], { weekday: "short" })} ${clock}`;
}
