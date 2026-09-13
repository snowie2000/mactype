import { useState } from "react";
import type { InstallationStatus } from "../../app/model";
import { copyDiagnostics, exportDiagnostics, openLogFolder } from "../../app/tauri";
import { useI18n, type I18nValue } from "../../i18n/i18n";

export type DiagnosticsOperation = "export" | "copy" | "folder" | "relocate" | "reconnect";

export interface DiagnosticsModelOptions {
  status: InstallationStatus;
  onReconnect: () => Promise<InstallationStatus>;
  onRelocate: () => Promise<InstallationStatus>;
}

/* Installation findings are named the same way everywhere they appear: the
   diagnostics page, the status bars, and the overview rows. */
export function findingLabel(t: I18nValue["t"], label: string, value: string): string {
  if (value === "MacType.dll") return t("finding.core32");
  if (value === "MacType64.dll") return t("finding.core64");
  if (value === "MacLoader.exe") return t("finding.loader");
  if (label === "preview") return t("finding.preview");
  return label;
}

export function findingValue(t: I18nValue["t"], value: string): string {
  return value === "waiting" ? t("finding.waiting") : value === "connected" ? t("overview.checked") : value;
}

export function useDiagnosticsModel({ status, onReconnect, onRelocate }: DiagnosticsModelOptions) {
  const { t } = useI18n();
  const [operation, setOperation] = useState<DiagnosticsOperation | null>(null);
  const [completed, setCompleted] = useState<{ kind: DiagnosticsOperation; detail: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async (kind: DiagnosticsOperation) => {
    setOperation(kind);
    setCompleted(null);
    setError(null);
    try {
      if (kind === "export") setCompleted({ kind, detail: await exportDiagnostics() });
      if (kind === "copy") {
        await copyDiagnostics();
        setCompleted({ kind, detail: t("diagnostics.copy") });
      }
      if (kind === "folder") setCompleted({ kind, detail: await openLogFolder() });
      if (kind === "relocate") {
        await onRelocate();
        setCompleted({ kind, detail: t("overview.relocate") });
      }
      if (kind === "reconnect") {
        await onReconnect();
        setCompleted({ kind, detail: t("overview.reconnect") });
      }
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setOperation(null);
    }
  };

  const findings = status.findings.map((finding) => ({
    key: finding.label,
    label: findingLabel(t, finding.label, finding.value),
    value: findingValue(t, finding.value),
    ok: finding.ok,
  }));

  return {
    completed,
    error,
    findings,
    operation,
    run,
    status,
    t,
  };
}

export type DiagnosticsModel = ReturnType<typeof useDiagnosticsModel>;
