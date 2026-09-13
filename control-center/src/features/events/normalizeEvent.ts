import type { EventRecord } from "../../app/model";

/* The backend omits empty `params` and a missing `detail` from the wire
   record, and an older or foreign line may lack more than that. The UI only
   ever sees a complete record, so a missing field can never take a page
   down. */
export function normalizeEvent(raw: Partial<EventRecord> & { ts?: number; code?: string }): EventRecord {
  return {
    v: typeof raw.v === "number" ? raw.v : 1,
    ts: typeof raw.ts === "number" ? raw.ts : 0,
    severity: raw.severity ?? "info",
    area: raw.area ?? "control-center",
    code: typeof raw.code === "string" && raw.code ? raw.code : "unknown-event",
    params: raw.params && typeof raw.params === "object" ? raw.params : {},
    detail: typeof raw.detail === "string" ? raw.detail : null,
    source: raw.source ?? "control-center",
  };
}

export function normalizeEvents(raw: unknown): EventRecord[] {
  if (!Array.isArray(raw)) return [];
  return raw.map((entry) => normalizeEvent((entry ?? {}) as Partial<EventRecord>));
}
