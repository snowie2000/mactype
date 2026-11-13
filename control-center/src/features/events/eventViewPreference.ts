import { readPreference, writePreference } from "../../app/persistedPreference";
import { defaultEventViewOptions, type EventViewOptions } from "./useEventLog";

const storageKey = "mactype-control-center.event-view";

function parseEventViewOptions(raw: string): EventViewOptions | null {
  try {
    const stored: unknown = JSON.parse(raw);
    if (!stored || typeof stored !== "object" || Array.isArray(stored)) return null;
    const values = stored as Record<string, unknown>;
    return {
      hideInjectionSummary: typeof values.hideInjectionSummary === "boolean" ? values.hideInjectionSummary : false,
      collapseRepeatedFailures: typeof values.collapseRepeatedFailures === "boolean" ? values.collapseRepeatedFailures : false,
      hideRoutine: typeof values.hideRoutine === "boolean" ? values.hideRoutine : false,
    };
  } catch {
    return null;
  }
}

export function loadEventViewOptions(): EventViewOptions {
  return readPreference({
    key: storageKey,
    parse: parseEventViewOptions,
    fallback: () => ({ ...defaultEventViewOptions }),
  });
}

export function saveEventViewOptions(options: EventViewOptions): void {
  writePreference(storageKey, JSON.stringify(options));
}
