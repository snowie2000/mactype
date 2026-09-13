import { useCallback, useEffect, useMemo, useState } from "react";
import type { EventArea, EventLogSummary, EventRecord, EventSeverity } from "../../app/model";
import { listEvents, loadEventLogSummary, subscribeEventLog } from "../../app/tauri";

import { loadEventViewOptions, saveEventViewOptions } from "./eventViewPreference";

export type EventViewOptionId = "hideInjectionSummary" | "collapseRepeatedFailures" | "hideRoutine";
export interface EventViewOptions {
  hideInjectionSummary: boolean;
  collapseRepeatedFailures: boolean;
  hideRoutine: boolean;
}
export const defaultEventViewOptions: EventViewOptions = { hideInjectionSummary: false, collapseRepeatedFailures: false, hideRoutine: false };
export const routineEventCodes: ReadonlyArray<string> = ["app-started", "preview-helper-connected", "profile-verified"];

export const eventSeverities: ReadonlyArray<EventSeverity> = ["info", "notice", "warning", "error"];
export const eventAreas: ReadonlyArray<EventArea> = ["service", "setup", "profile", "preview", "injection", "control-center", "tray"];

export interface EventLogFilters {
  severities: ReadonlySet<EventSeverity>;
  areas: ReadonlySet<EventArea>;
  query: string;
}

const DEFAULT_LIMIT = 300;

/* The diagnostics timeline: every event within the limit, filtered locally so
   toggling a chip never waits on the backend, with live refresh. */
export function useEventLog() {
  const [events, setEvents] = useState<ReadonlyArray<EventRecord>>([]);
  const [summary, setSummary] = useState<EventLogSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [severities, setSeverities] = useState<ReadonlySet<EventSeverity>>(() => new Set(eventSeverities));
  const [areas, setAreas] = useState<ReadonlySet<EventArea>>(() => new Set(eventAreas));
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [view, setView] = useState(loadEventViewOptions);
  const setViewOption = (id: EventViewOptionId, value: boolean) => {
    const next = { ...view, [id]: value };
    setView(next);
    saveEventViewOptions(next);
  };

  const refresh = useCallback(() => {
    void Promise.all([listEvents(undefined, DEFAULT_LIMIT), loadEventLogSummary()])
      .then(([nextEvents, nextSummary]) => {
        setEvents(nextEvents);
        setSummary(nextSummary);
        setError(null);
      })
      .catch((caught: unknown) => setError(caught instanceof Error ? caught.message : String(caught)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
    return subscribeEventLog(refresh);
  }, [refresh]);

  const toggleSeverity = (severity: EventSeverity) => setSeverities((current) => {
    const next = new Set(current);
    if (next.has(severity)) next.delete(severity);
    else next.add(severity);
    return next;
  });
  const toggleArea = (area: EventArea) => setAreas((current) => {
    const next = new Set(current);
    if (next.has(area)) next.delete(area);
    else next.add(area);
    return next;
  });
  const resetFilters = () => {
    setSeverities(new Set(eventSeverities));
    setAreas(new Set(eventAreas));
    setQuery("");
  };

  const needle = query.trim().toLocaleLowerCase();
  const { visible, repeats } = useMemo(() => {
    const matching = events.filter((event) =>
      severities.has(event.severity)
      && areas.has(event.area)
      && (!needle || `${event.code} ${Object.values(event.params ?? {}).join(" ")} ${event.detail ?? ""}`.toLocaleLowerCase().includes(needle)))
      .filter((event) => !view.hideInjectionSummary || event.code !== "injection-summary")
      .filter((event) => !view.hideRoutine || !routineEventCodes.includes(event.code));
    const repeats = new Map<EventRecord, number>();
    if (!view.collapseRepeatedFailures) return { visible: matching, repeats };
    const failures = new Map<string, { newest: EventRecord; count: number }>();
    for (const event of matching) {
      if (event.code !== "injection-failed") continue;
      const params = event.params ?? {};
      const key = `${(params.process ?? "").toLocaleLowerCase()}|${params.reason ?? ""}`;
      const previous = failures.get(key);
      failures.set(key, {
        newest: previous && previous.newest.ts > event.ts ? previous.newest : event,
        count: (previous?.count ?? 0) + 1,
      });
    }
    for (const { newest, count } of failures.values()) repeats.set(newest, count);
    return { visible: matching.filter((event) => event.code !== "injection-failed" || repeats.has(event)), repeats };
  }, [areas, events, needle, severities, view]);
  const filtered = severities.size !== eventSeverities.length || areas.size !== eventAreas.length || needle.length > 0;
  const viewHidesEverything = events.length > 0 && !filtered && visible.length === 0;
  const repeatCount = (event: EventRecord) => repeats.get(event) ?? 1;
  const eventKey = (event: EventRecord) => `${event.source}:${event.ts}:${event.code}`;

  return {
    areas,
    error,
    eventKey,
    events,
    expanded,
    filtered,
    loading,
    query,
    refresh,
    resetFilters,
    setExpanded,
    setQuery,
    severities,
    summary,
    toggleArea,
    toggleSeverity,
    visible,
    view,
    setViewOption,
    repeatCount,
    viewHidesEverything,
  };
}

export type EventLogModel = ReturnType<typeof useEventLog>;
