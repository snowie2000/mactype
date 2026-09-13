import { defaultEventViewOptions, type EventViewOptions } from "./useEventLog";

const storageKey = "mactype-control-center.event-view";

export function loadEventViewOptions(): EventViewOptions {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(storageKey) ?? "null");
    if (!stored || typeof stored !== "object" || Array.isArray(stored)) return { ...defaultEventViewOptions };
    const values = stored as Record<string, unknown>;
    return {
      hideInjectionSummary: typeof values.hideInjectionSummary === "boolean" ? values.hideInjectionSummary : false,
      collapseRepeatedFailures: typeof values.collapseRepeatedFailures === "boolean" ? values.collapseRepeatedFailures : false,
      hideRoutine: typeof values.hideRoutine === "boolean" ? values.hideRoutine : false,
    };
  } catch {
    return { ...defaultEventViewOptions };
  }
}

export function saveEventViewOptions(options: EventViewOptions): void {
  try {
    window.localStorage.setItem(storageKey, JSON.stringify(options));
  } catch {
    // View options still apply for this session when storage is unavailable.
  }
}
