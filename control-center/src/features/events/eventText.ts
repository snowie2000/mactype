import type { EventArea, EventRecord, EventSeverity, EventSource } from "../../app/model";
import { catalogs, type I18nValue, type Locale, type MessageKey } from "../../i18n/i18n";

/* Event codes are localised through `event.<code>` keys. A code the catalog
   does not know yet (an older or newer backend) falls back to the code
   itself with its parameters, so nothing is ever hidden. */
export function eventTitle(t: I18nValue["t"], locale: Locale, event: EventRecord): string {
  const key = `event.${event.code}` as MessageKey;
  const eventParams = event.params ?? {};
  if (key in catalogs[locale]) return t(key, localizedParams(t, locale, event));
  const params = Object.entries(eventParams).map(([name, value]) => `${name}=${value}`).join(" ");
  return params ? `${event.code} (${params})` : event.code;
}

/* Closed-vocabulary state and reason parameters are localised only when the
   catalog knows them; unknown backend values retain their original spelling. */
function localizedParams(t: I18nValue["t"], locale: Locale, event: EventRecord): Record<string, string> {
  const params = { ...event.params };
  const vocabulary = { state: "state", reason: "reason", ...(event.code === "helper-broker-failed" ? { code: "reason" } : {}) };
  for (const [name, prefix] of Object.entries(vocabulary)) {
    const key = `event.${prefix}.${params[name] ?? ""}` as MessageKey;
    if (params[name] !== undefined && key in catalogs[locale]) params[name] = t(key);
  }
  return params;
}

export function severityLabel(t: I18nValue["t"], severity: EventSeverity): string {
  return t(`event.severity.${severity}` as MessageKey);
}

export function areaLabel(t: I18nValue["t"], area: EventArea): string {
  return t(`event.area.${area}` as MessageKey);
}

export function sourceLabel(t: I18nValue["t"], source: EventSource): string {
  return t(`event.source.${source}` as MessageKey);
}

export function eventDate(ts: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(new Date(ts));
}

export function eventTime(ts: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }).format(new Date(ts));
}

export function eventClock(ts: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(new Date(ts));
}

/* Groups a chronological list by calendar day, newest day first, newest
   event first inside a day. */
export function groupEventsByDay(events: ReadonlyArray<EventRecord>, locale: string): ReadonlyArray<{ day: string; events: ReadonlyArray<EventRecord> }> {
  const groups = new Map<string, EventRecord[]>();
  for (const event of [...events].reverse()) {
    const day = eventDate(event.ts, locale);
    const bucket = groups.get(day);
    if (bucket) bucket.push(event);
    else groups.set(day, [event]);
  }
  return [...groups.entries()].map(([day, list]) => ({ day, events: list }));
}
