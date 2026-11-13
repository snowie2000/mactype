import { useEffect, useMemo, useState } from "react";
import { projectExecutionView, type ExecutionViewModel } from "../../app/executionViewModel";
import type { EventRecord, ExecutionStatus } from "../../app/model";
import { runtime } from "../../app/runtimeAdapter";
import { useI18n } from "../../i18n/i18n";
import { eventTitle } from "../events/eventText";

export type OverviewState = ExecutionViewModel["overviewState"];

export function timeText(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(new Date(timestamp));
}

export function clockText(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false }).format(new Date(timestamp));
}

/* The overview reads the execution status once and the recent activity feed,
   and projects both into the service summary on the overview page. */
export function useOverviewModel() {
  const { locale, t } = useI18n();
  const [execution, setExecution] = useState<ExecutionStatus | null>(null);
  const [activities, setActivities] = useState<ReadonlyArray<EventRecord>>([]);
  const [expanded, setExpanded] = useState(false);
  const [folderMessage, setFolderMessage] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void runtime().loadExecutionStatus().then((nextExecution) => {
      if (active) setExecution(nextExecution);
    }).catch(() => undefined);
    const refresh = () => {
      void runtime().loadRecentActivity().then((nextActivities) => {
        if (active) setActivities(nextActivities.slice(-5));
      }).catch(() => undefined);
    };
    refresh();
    const unsubscribe = runtime().subscribeEventLog(refresh);
    return () => { active = false; unsubscribe(); };
  }, []);

  const view = useMemo(() => projectExecutionView(execution, null), [execution]);
  const state = view.overviewState;
  const newestFirst = useMemo(() => [...activities].reverse(), [activities]);
  const latestApplied = newestFirst.find((entry) => entry.code === "profile-applied");
  const activityMessage = (entry: EventRecord) => eventTitle(t, locale, entry);
  const openFolder = async () => {
    setFolderMessage(null);
    try {
      setFolderMessage(await runtime().openLogFolder());
    } catch {
      setFolderMessage(t("overview.logFolderFailed"));
    }
  };
  const activeProfile = execution?.activeProfile ?? null;
  const activeProfileName = view.activeProfileDisplay.name;
  const lastAppliedText = latestApplied ? t("overview.todayAt", { time: timeText(latestApplied.ts, locale) }) : t("overview.noLastApplied");

  return {
    activeProfile,
    activeProfileName,
    activities,
    activityMessage,
    execution,
    expanded,
    folderMessage,
    lastAppliedText,
    latestApplied,
    locale,
    newestFirst,
    openFolder,
    setExpanded,
    state,
    t,
    view,
  };
}

export type OverviewModel = ReturnType<typeof useOverviewModel>;
