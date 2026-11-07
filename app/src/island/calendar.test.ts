import { describe, expect, it } from "vitest";
import { parseCalendar } from "./calendar";

const wrap = (body: string, extra = "") => `BEGIN:VCALENDAR\r\nVERSION:2.0\r\n${extra}${body}END:VCALENDAR\r\n`;
const event = (lines: string) => `BEGIN:VEVENT\r\n${lines}END:VEVENT\r\n`;
const iso = (t: number) => new Date(t).toISOString();

describe("parseCalendar", () => {
  it("reads UTC and zoned events with folded lines and escapes", () => {
    const text = wrap(event("UID:a\r\nSUMMARY:Team\, sync\r\nDESCRIPTION:long\r\n  folded\r\nDTSTART:20261110T090000Z\r\nDTEND:20261110T093000Z\r\n") +
      event("UID:b\r\nSUMMARY:Dentist\r\nLOCATION:Athens\r\nDTSTART;TZID=Europe/Athens:20261110T120000\r\nDTEND;TZID=Europe/Athens:20261110T130000\r\n"));
    const events = parseCalendar(text, Date.UTC(2026, 10, 1), Date.UTC(2026, 11, 1));
    expect(events.map((e) => [e.title, iso(e.start), (e.end - e.start) / 60000])).toEqual([
      ["Team, sync", "2026-11-10T09:00:00.000Z", 30],
      ["Dentist", "2026-11-10T10:00:00.000Z", 60], // Athens is UTC+2 in November
    ]);
  });

  it("maps Outlook's Windows zone names and handles summer time", () => {
    const text = wrap(event("UID:c\r\nSUMMARY:Call\r\nDTSTART;TZID=GTB Standard Time:20260715T100000\r\nDURATION:PT45M\r\n"));
    const [e] = parseCalendar(text, Date.UTC(2026, 6, 1), Date.UTC(2026, 7, 1));
    expect(iso(e.start)).toBe("2026-07-15T07:00:00.000Z"); // UTC+3 in July
    expect(e.end - e.start).toBe(45 * 60000);
  });

  it("uses a VTIMEZONE when the zone name is unknown", () => {
    const tz = "BEGIN:VTIMEZONE\r\nTZID:Custom Zone\r\nBEGIN:STANDARD\r\nDTSTART:16011028T040000\r\nRRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\r\nTZOFFSETTO:+0200\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nDTSTART:16010325T030000\r\nRRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=3\r\nTZOFFSETTO:+0300\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n";
    const text = wrap(event("UID:d\r\nDTSTART;TZID=Custom Zone:20260715T100000\r\n") + event("UID:e\r\nDTSTART;TZID=Custom Zone:20261215T100000\r\n"), tz);
    const events = parseCalendar(text, Date.UTC(2026, 0, 1), Date.UTC(2027, 0, 1));
    expect(events.map((e) => iso(e.start))).toEqual(["2026-07-15T07:00:00.000Z", "2026-12-15T08:00:00.000Z"]);
  });

  it("expands weekly rules with exceptions and moved occurrences", () => {
    const text = wrap(
      event("UID:w\r\nSUMMARY:Standup\r\nDTSTART:20261102T080000Z\r\nDTEND:20261102T081500Z\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=6\r\nEXDATE:20261104T080000Z\r\n") +
      event("UID:w\r\nSUMMARY:Standup (moved)\r\nRECURRENCE-ID:20261109T080000Z\r\nDTSTART:20261109T100000Z\r\nDTEND:20261109T101500Z\r\n"));
    const events = parseCalendar(text, Date.UTC(2026, 10, 1), Date.UTC(2026, 11, 31));
    expect(events.map((e) => `${e.title} ${iso(e.start)}`)).toEqual([
      "Standup 2026-11-02T08:00:00.000Z",
      "Standup (moved) 2026-11-09T10:00:00.000Z",
      "Standup 2026-11-11T08:00:00.000Z",
      "Standup 2026-11-16T08:00:00.000Z",
      "Standup 2026-11-18T08:00:00.000Z",
    ]);
  });

  it("expands monthly nth-weekday and yearly rules, and all-day events", () => {
    const text = wrap(
      event("UID:m\r\nSUMMARY:Review\r\nDTSTART:20261110T120000Z\r\nRRULE:FREQ=MONTHLY;BYDAY=2TU;UNTIL=20270301T000000Z\r\n") +
      event("UID:y\r\nSUMMARY:Birthday\r\nDTSTART;VALUE=DATE:20200105\r\nDTEND;VALUE=DATE:20200106\r\nRRULE:FREQ=YEARLY\r\n"));
    const events = parseCalendar(text, Date.UTC(2026, 10, 1), Date.UTC(2027, 1, 28));
    const reviews = events.filter((e) => e.title === "Review").map((e) => iso(e.start).slice(0, 10));
    expect(reviews).toEqual(["2026-11-10", "2026-12-08", "2027-01-12", "2027-02-09"]);
    const birthday = events.find((e) => e.title === "Birthday")!;
    expect(birthday.allDay).toBe(true);
    expect(new Date(birthday.start).getDate()).toBe(5);
    expect(new Date(birthday.start).getFullYear()).toBe(2027);
  });

  it("skips cancelled events", () => {
    const text = wrap(event("UID:x\r\nSTATUS:CANCELLED\r\nDTSTART:20261110T090000Z\r\n"));
    expect(parseCalendar(text, 0, Date.UTC(2030, 0, 1))).toEqual([]);
  });
});
