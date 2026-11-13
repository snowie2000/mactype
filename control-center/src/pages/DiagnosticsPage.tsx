import { AlertTriangle, Check, Copy, Download, ExternalLink, FolderSearch, LoaderCircle } from "lucide-react";
import { EventSourceList, EventTimeline, EventViewOptions } from "../features/events/EventTimeline";
import { useEventLog } from "../features/events/useEventLog";
import type { InstallationStatus } from "../app/model";
import { useDiagnosticsModel } from "../features/diagnostics/useDiagnosticsModel";

interface DiagnosticsPageProps {
  status: InstallationStatus;
  onReconnect: () => Promise<InstallationStatus>;
  onRelocate: () => Promise<InstallationStatus>;
}

export function DiagnosticsPage({ status, onReconnect, onRelocate }: DiagnosticsPageProps) {
  const log = useEventLog();
  const model = useDiagnosticsModel({ status, onReconnect, onRelocate });
  const { t, operation, completed, error, run } = model;

  return (
    <section className="page view-enter" aria-labelledby="diagnostics-title">
      <header className="page-header">
        <div><h1 id="diagnostics-title">{t("nav.diagnostics")}</h1><p>{t("diagnostics.subtitle")}</p></div>
        <button aria-busy={operation === "export"} className="button primary" disabled={operation !== null} onClick={() => void run("export")} type="button">{operation === "export" ? <LoaderCircle aria-hidden="true" className="spin" size={17} /> : <Download aria-hidden="true" size={17} />} {t("diagnostics.export")}</button>
      </header>

      <section className="section-block" aria-labelledby="installation-title">
        <div className="section-heading installation-heading">
          <div><h2 id="installation-title">{t("overview.installation")}</h2><code>{status.root ?? t("overview.noRoot")}</code></div>
          <div className="header-actions">
            <button aria-busy={operation === "relocate"} className="button secondary" disabled={operation !== null} onClick={() => void run("relocate")} type="button">{operation === "relocate" ? <LoaderCircle aria-hidden="true" className="spin" size={17} /> : <FolderSearch aria-hidden="true" size={17} />}{t("overview.relocate")}</button>
            <button aria-busy={operation === "reconnect"} className="button secondary" disabled={operation !== null} onClick={() => void run("reconnect")} type="button">{operation === "reconnect" && <LoaderCircle aria-hidden="true" className="spin" size={17} />}{t("overview.reconnect")}</button>
          </div>
        </div>
        <dl className="detail-list">
          {model.findings.map((finding) => <div key={finding.key}><dt>{finding.label}</dt><dd>{finding.ok ? <Check className="success" aria-label={t("overview.checked")} size={17} /> : <AlertTriangle className="warning" aria-label={t("overview.attention")} size={17} />}<span>{finding.value}</span></dd></div>)}
        </dl>
      </section>

      <section className="section-block" aria-labelledby="components-title">
        <div className="section-heading"><h2 id="components-title">{t("diagnostics.components")}</h2><button aria-busy={operation === "copy"} className="icon-button" disabled={operation !== null} aria-label={t("diagnostics.copy")} onClick={() => void run("copy")} type="button">{operation === "copy" ? <LoaderCircle aria-hidden="true" className="spin" size={17} /> : <Copy aria-hidden="true" size={17} />}</button></div>
        <dl className="detail-list diagnostic-list">
          <div><dt>Control Center</dt><dd><Check className="success" size={17} /><code>0.1.0</code></dd></div>
          <div><dt>{t("diagnostics.core")}</dt><dd><Check className="success" size={17} /><code>{status.coreVersion ?? t("diagnostics.unknown")}</code></dd></div>
        </dl>
      </section>

      <section className="section-block" aria-labelledby="log-title">
        <div className="section-heading"><div><h2 id="log-title">{t("diagnostics.logs")}</h2><p>{t("diagnostics.logsDescription")}</p></div><EventViewOptions log={log} /></div>
        <div className="diagnostic-events" id="diagnostic-log-view" role="log" aria-label={t("diagnostics.logAria")}>
          <EventTimeline log={log} viewOptions={false} />
        </div>
        <details className="event-source-disclosure"><summary>{t("events.logFiles")}</summary><EventSourceList log={log} /></details>
        <div className="disclosure-actions" data-log-disclosure-actions>
          <button aria-busy={operation === "folder"} className="text-action" disabled={operation !== null} onClick={() => void run("folder")} type="button">{operation === "folder" ? <LoaderCircle aria-hidden="true" className="spin" size={15} /> : <ExternalLink aria-hidden="true" size={15} />}{t("diagnostics.openFolder")}</button>
        </div>
      </section>
      <div aria-live="polite">
        {completed && <p className="success-message" data-operation={completed.kind}><Check aria-hidden="true" size={16} /> <span>{completed.detail}</span></p>}
        {error && <p className="inline-error"><AlertTriangle aria-hidden="true" size={15} /> {error}</p>}
      </div>
    </section>
  );
}
