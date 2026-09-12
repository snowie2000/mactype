import { AlertTriangle, Check, ChevronDown, FileCode2, FolderOpen, ListChecks, LogOut, Play, Power, PowerOff, RefreshCw, ServerCog, ShieldAlert, Trash2, UserPlus, Wrench } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { ExecutionStatus, ManualLaunchCandidate, SystemServiceAction } from "../app/model";
import { projectExecutionView } from "../app/executionViewModel";
import { operationErrorMessage } from "../app/operationError";
import { disableLegacyTrayAutostart, launchRegisteredTargets, launchTargetWithMactype, listManualLaunchCandidates, loadExecutionStatus, manageSystemService, pickExecutable, registerSessionTarget, removeSessionTarget, reportFrontendFailure, requestLegacyTrayExit, revealSystemService, setSessionAutostart, verifyInjectionWorkflowForCi } from "../app/tauri";
import { useI18n } from "../i18n/i18n";

export function ExecutionPage({ ciSmoke = false, onReady }: { ciSmoke?: boolean; onReady?: () => void }) {
  const { t } = useI18n();
  const [status, setStatus] = useState<ExecutionStatus | null>(null);
  const [target, setTarget] = useState("");
  const [argumentsText, setArgumentsText] = useState("");
  const [candidates, setCandidates] = useState<ReadonlyArray<ManualLaunchCandidate> | null>(null);
  const [candidateFilter, setCandidateFilter] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [serviceBusy, setServiceBusy] = useState<string | null>(null);
  const [legacyTrayBusy, setLegacyTrayBusy] = useState<"exit" | "disable-autostart" | null>(null);
  const [migrationConfirmationOpen, setMigrationConfirmationOpen] = useState(false);
  const migrationTriggerRef = useRef<HTMLButtonElement>(null);
  const migrationCancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (migrationConfirmationOpen) migrationCancelRef.current?.focus();
  }, [migrationConfirmationOpen]);

  const refresh = useCallback(async () => {
    try {
      const nextStatus = await loadExecutionStatus();
      setStatus(nextStatus);
      setError(null);
      if (ciSmoke) {
        if (!nextStatus.injectionReady || !nextStatus.activeProfile) {
          throw new Error("CI profile application did not produce an active injection runtime");
        }
        await verifyInjectionWorkflowForCi();
        onReady?.();
      }
    } catch (caught: unknown) {
      const message = caught instanceof Error ? caught.message : String(caught);
      setError(message);
      if (ciSmoke) void reportFrontendFailure("execution", message);
    }
  }, [ciSmoke, onReady]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggleAutostart = async (enabled: boolean) => {
    try {
      const actual = await setSessionAutostart(enabled);
      setStatus((current) => current ? { ...current, autoStart: actual } : current);
      setMessage(actual ? t("execution.autostartOn") : t("execution.autostartOff"));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const launch = async () => {
    try {
      const arguments_ = argumentsText.split(/\r?\n/).map((argument) => argument.trim()).filter(Boolean);
      const pid = await launchTargetWithMactype(target, arguments_);
      setMessage(t("execution.launched", { pid }));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const argumentsFromEditor = () => argumentsText.split(/\r?\n/).map((argument) => argument.trim()).filter(Boolean);

  const loadCandidates = useCallback(async () => {
    try {
      setCandidates(await listManualLaunchCandidates());
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const chooseTarget = async () => {
    try {
      const selected = await pickExecutable(t("execution.executableFilter"));
      if (selected) setTarget(selected);
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const register = async () => {
    try {
      const sessionTargets = await registerSessionTarget(target, argumentsFromEditor());
      setStatus((current) => current ? { ...current, sessionTargets } : current);
      setMessage(t("execution.registered"));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const remove = async (registeredTarget: string) => {
    try {
      const sessionTargets = await removeSessionTarget(registeredTarget);
      setStatus((current) => current ? { ...current, sessionTargets } : current);
      setMessage(t("execution.removed"));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const launchAll = async () => {
    try {
      const processes = await launchRegisteredTargets();
      setMessage(t("execution.launchedRegistered", { count: processes.length }));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const manageService = async (action: SystemServiceAction) => {
    setServiceBusy(action);
    const hadProfile = Boolean(status?.activeProfile);
    try {
      const nextStatus = await manageSystemService(action);
      setStatus(nextStatus);
      // "start" and "publish-profile" auto-apply the bundled default profile
      // when none was applied yet; name what happened instead of a generic note.
      const defaultApplied = !hadProfile && Boolean(nextStatus.activeProfile);
      const appliedName = nextStatus.activeProfile?.split(/[\\/]/).pop() ?? "";
      setMessage(
        action === "stop"
          ? t("execution.systemPaused")
          : action === "publish-profile"
            ? (defaultApplied
              ? t("execution.systemActivatedWithDefaultProfile", { name: appliedName })
              : t("execution.systemActivated"))
            : action === "migrate-from-legacy"
              ? t("execution.migrationComplete")
              : action === "remove-legacy"
                ? t("execution.legacyRemoved")
                : action === "start" && defaultApplied
                  ? t("execution.serviceStartedWithDefaultProfile", { name: appliedName })
                  : t("execution.serviceActionDone"),
      );
      setError(null);
    } catch (caught: unknown) {
      setError(operationErrorMessage(
        caught,
        t,
        action === "migrate-from-legacy" ? "execution.migrationFailed" : "execution.operationFailed",
      ));
      setMessage(null);
    } finally {
      setServiceBusy(null);
    }
  };

  const revealServiceLocation = async () => {
    try {
      await revealSystemService();
      setMessage(t("execution.serviceLocationOpened"));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setMessage(null);
    }
  };

  const exitLegacyTray = async () => {
    const process = status?.legacyTray.process;
    if (!process || process.state !== "trusted-current-session") return;
    setLegacyTrayBusy("exit");
    try {
      const nextStatus = await requestLegacyTrayExit({
        pid: process.pid,
        creationTime: process.creationTime,
        path: process.path,
      });
      setStatus(nextStatus);
      setMessage(t("execution.legacyTrayExited"));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setMessage(null);
    } finally {
      setLegacyTrayBusy(null);
    }
  };

  const disableLegacyTrayStartup = async () => {
    setLegacyTrayBusy("disable-autostart");
    try {
      const nextStatus = await disableLegacyTrayAutostart();
      setStatus(nextStatus);
      setMessage(t("execution.legacyTrayAutostartDisabled"));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setMessage(null);
    } finally {
      setLegacyTrayBusy(null);
    }
  };

  const restoreMigrationTriggerFocus = () => {
    window.requestAnimationFrame(() => migrationTriggerRef.current?.focus());
  };

  const closeMigrationConfirmation = () => {
    setMigrationConfirmationOpen(false);
    restoreMigrationTriggerFocus();
  };

  const confirmMigration = async () => {
    setMigrationConfirmationOpen(false);
    await manageService("migrate-from-legacy");
    restoreMigrationTriggerFocus();
  };

  const handleMigrationDialogKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMigrationConfirmation();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const first = focusable[0];
    const last = focusable.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const candidateFilterText = candidateFilter.trim().toLowerCase();
  const visibleCandidates = (candidates ?? []).filter((candidate) => !candidateFilterText
    || candidate.name.toLowerCase().includes(candidateFilterText)
    || candidate.path.toLowerCase().includes(candidateFilterText)
    || (candidate.windowTitle?.toLowerCase().includes(candidateFilterText) ?? false));

  const executionView = projectExecutionView(status, serviceBusy);
  const systemInjectionAction = executionView.systemInjectionAction;
  const service = executionView.status?.systemService;
  const legacyService = executionView.status?.legacyMacTray;
  const legacyTrayResolution = executionView.legacyTrayResolution;
  const serviceSummary = executionView.serviceSummary;
  const serviceStatusLine = executionView.serviceStatusLine;
  const profileIndicator = executionView.profileIndicator;
  const serviceStateText = serviceStatusLine
    ? [serviceStatusLine.installationKey, serviceStatusLine.runtimeKey, ...(serviceStatusLine.healthKey ? [serviceStatusLine.healthKey] : [])].map((key) => t(key)).join(" · ")
    : t("execution.checking");
  const servicePackageNotice = status?.serviceManagementPackage === "not-installed"
    ? {
        titleKey: "execution.servicePackageNotInstalledTitle" as const,
        descriptionKey: "execution.servicePackageNotInstalledDescription" as const,
      }
    : status?.serviceManagementPackage === "incomplete"
      ? {
          titleKey: "execution.servicePackageIncompleteTitle" as const,
          descriptionKey: "execution.servicePackageIncompleteDescription" as const,
        }
      : status?.serviceManagementPackage === "untrusted"
        ? {
            titleKey: "execution.servicePackageUntrustedTitle" as const,
            descriptionKey: "execution.servicePackageUntrustedDescription" as const,
          }
        : null;
  const activeProfileName = status?.activeProfile?.split(/[\\/]/).pop() ?? t("execution.profileNotApplied");

  const runSummaryAction = (command: SystemServiceAction) => {
    if (command === "migrate-from-legacy") {
      setMigrationConfirmationOpen(true);
      return;
    }
    void manageService(command);
  };

  return (
    <section className="page view-enter" aria-labelledby="execution-title">
      <header className="page-header">
        <div><h1 id="execution-title">{t("nav.execution")}</h1><p>{t("execution.subtitle")}</p></div>
        <div className="header-actions">
          <button className="button secondary" onClick={() => void refresh()} type="button"><RefreshCw aria-hidden="true" size={16} /> {t("execution.refresh")}</button>
        </div>
      </header>

      <section className="service-summary" data-state={serviceSummary.tone} data-service-summary>
        <dl className="service-summary-grid">
          <div><dt>{t("execution.summaryProfile")}</dt><dd><code title={status?.activeProfile ?? undefined}>{activeProfileName}</code></dd></div>
          <div><dt>{t("execution.summaryMode")}</dt><dd>{t(serviceSummary.modeKey)}</dd></div>
          <div>
            <dt>{t("execution.summaryStatus")}</dt>
            <dd>{serviceSummary.tone === "normal" ? <Check className="success" aria-hidden="true" size={18} /> : serviceSummary.tone === "neutral" ? <PowerOff className="neutral-status" aria-hidden="true" size={18} /> : <AlertTriangle className="warning" aria-hidden="true" size={18} />}{t(serviceSummary.statusKey)}</dd>
          </div>
        </dl>
        {legacyTrayResolution ? (
          <div className="legacy-tray-conflict" data-kind={legacyTrayResolution.kind} data-legacy-tray-conflict data-prominent-exception>
            <div className="legacy-tray-conflict-copy">
              <span className="legacy-tray-conflict-icon"><ShieldAlert aria-hidden="true" size={20} /></span>
              <div>
                <strong>{t(legacyTrayResolution.titleKey)}</strong>
                <p>{t(legacyTrayResolution.descriptionKey)}</p>
              </div>
            </div>
            <div className="legacy-tray-conflict-actions">
              <button className="button secondary" disabled={legacyTrayBusy !== null} onClick={() => void refresh()} type="button">
                <RefreshCw aria-hidden="true" size={16} /> {t("execution.legacyTrayCheckAgain")}
              </button>
              {legacyTrayResolution.canRequestExit && (
                <button className="button primary" disabled={legacyTrayBusy !== null} onClick={() => void exitLegacyTray()} type="button">
                  <LogOut aria-hidden="true" size={16} /> {t("execution.legacyTrayExit")}
                </button>
              )}
              {legacyTrayResolution.canDisableStartup && (
                <button className="button primary" disabled={legacyTrayBusy !== null} onClick={() => void disableLegacyTrayStartup()} type="button">
                  <PowerOff aria-hidden="true" size={16} /> {t("execution.legacyTrayDisableAutostart")}
                </button>
              )}
            </div>
          </div>
        ) : (
          <>
            {serviceSummary.notice && (
              <div className="service-summary-notice" data-kind={serviceSummary.notice.kind} data-prominent-exception>
                {serviceSummary.notice.kind === "repair" ? <Wrench aria-hidden="true" size={19} /> : <ShieldAlert aria-hidden="true" size={19} />}
                <div className="service-summary-notice-copy">
                  <strong>{t(serviceSummary.notice.titleKey)}</strong>
                  {serviceSummary.notice.descriptionKey && <p>{t(serviceSummary.notice.descriptionKey)}</p>}
                </div>
              </div>
            )}
            <div className="service-summary-actions">
              {serviceSummary.actions.map((action) => (
                <button
                  className={`button ${action.tone === "primary" ? "primary" : "secondary"}${action.tone === "danger" ? " danger" : ""}`}
                  disabled={!action.enabled}
                  key={action.command}
                  onClick={() => runSummaryAction(action.command)}
                  ref={action.command === "migrate-from-legacy" ? migrationTriggerRef : undefined}
                  type="button"
                >
                  {serviceBusy === action.command ? t("execution.serviceWorking") : t(action.labelKey)}
                </button>
              ))}
            </div>
          </>
        )}
      </section>

      <div className="service-rows">

      <details className="service-row" data-kind="system">
        <summary>
          <span className="service-row-icon"><ServerCog aria-hidden="true" size={19} /></span>
          <span className="service-row-copy"><h2>{t("execution.systemTitle")}</h2><p>{t("execution.systemDescription")}</p></span>
          <span className="service-row-state" data-tone={serviceSummary.tone}>{serviceStateText}</span>
          <ChevronDown aria-hidden="true" className="service-row-chevron" size={17} />
        </summary>
        <div className="service-row-body">
        <div className="open-service-card" data-service-backend="open-source">
        {servicePackageNotice && (
          <div className="service-package-notice" role="status" data-service-package={status?.serviceManagementPackage} data-prominent-exception>
            <span className="service-package-notice-icon"><ShieldAlert aria-hidden="true" size={20} /></span>
            <div>
              <strong>{t(servicePackageNotice.titleKey)}</strong>
              <p>{t(servicePackageNotice.descriptionKey)}</p>
            </div>
          </div>
        )}
        <div className="system-injection-control" data-active={systemInjectionAction.state === "active"} data-state={systemInjectionAction.state}>
          <div className="system-injection-state">
            <span className="system-injection-icon">{systemInjectionAction.intent === "stop" ? <Power aria-hidden="true" size={20} /> : <PowerOff aria-hidden="true" size={20} />}</span>
            <div>
              <span className="eyebrow">{t("execution.openServiceTitle")}</span>
              <strong>{t(systemInjectionAction.titleKey)}</strong>
              <p>{t(systemInjectionAction.descriptionKey)}</p>
            </div>
          </div>
          <button
            className={systemInjectionAction.intent === "stop" ? "button secondary system-injection-action" : "button primary system-injection-action"}
            disabled={!systemInjectionAction.enabled}
            onClick={() => void manageService(systemInjectionAction.command)}
            type="button"
          >
            {t(systemInjectionAction.labelKey)}
          </button>
        </div>
        <dl className="detail-list">
          <div><dt>{t("execution.openServiceStatus")}</dt><dd>{systemInjectionAction.state === "active" ? <Check className="success" size={17} /> : systemInjectionAction.state === "inactive" ? <PowerOff className="neutral-status" size={17} /> : <AlertTriangle className="warning" size={17} />}<span>{serviceStateText}</span></dd></div>
          <div><dt>{t("execution.profileGeneration")}</dt><dd>{profileIndicator.kind === "matched" ? <Check className="success" size={17} /> : profileIndicator.kind === "service-off" ? <PowerOff className="neutral-status" size={17} /> : <AlertTriangle className="warning" size={17} />}<span>{t(profileIndicator.labelKey)}</span></dd></div>
          <div><dt>{t("execution.appInit")}</dt><dd>{status?.registryModeDetected ? <ShieldAlert className="warning" size={17} /> : <Check className="success" size={17} />}<span>{status?.registryModeDetected ? t("execution.entryDetected") : t("profiles.disabled")}</span></dd></div>
        </dl>
        <div className="service-controls">
          <div>
            <strong>{t("execution.openServiceControlTitle")}</strong>
            <p>{t("execution.openServiceControlDescription")}</p>
            {executionView.serviceBinaryPath && (
              <div className="service-path">
                <code title={executionView.serviceBinaryPath}>{executionView.serviceBinaryPath}</code>
                <button className="button secondary" onClick={() => void revealServiceLocation()} type="button">
                  <FolderOpen aria-hidden="true" size={16} /> {t("execution.revealSystemService")}
                </button>
              </div>
            )}
            {service?.backend === "foreign" && <p className="warning-text">{t("execution.serviceForeign")}</p>}
            {service?.configurationDrift && <p className="warning-text">{t("execution.serviceConfigurationDriftDescription")}</p>}
          </div>
          <div className="service-actions">
            <button className="button secondary" disabled={!executionView.canInstall} onClick={() => void manageService("install")} type="button">{serviceBusy === "install" ? t("execution.serviceWorking") : t("execution.serviceInstall")}</button>
            <button className="button secondary" disabled={!executionView.canStart} onClick={() => void manageService("start")} type="button">{serviceBusy === "start" ? t("execution.serviceWorking") : t("execution.serviceStart")}</button>
            {executionView.serviceNeedsUpgrade && <button className="button secondary" disabled={!executionView.canUpgrade} onClick={() => void manageService("upgrade")} type="button">{serviceBusy === "upgrade" ? t("execution.serviceWorking") : t("execution.serviceUpgrade")}</button>}
            {executionView.serviceNeedsRepair && <button className="button secondary" disabled={!executionView.canRepair} onClick={() => void manageService("repair")} type="button">{serviceBusy === "repair" ? t("execution.serviceWorking") : t("execution.serviceRepair")}</button>}
            <button className="button secondary danger" disabled={!executionView.canRemove} onClick={() => void manageService("remove")} type="button">{serviceBusy === "remove" ? t("execution.serviceWorking") : t("execution.serviceRemove")}</button>
          </div>
        </div>
        </div>
        {legacyService && (
          <div className="service-controls legacy-service-controls" data-service-backend="legacy-mactray">
            <div>
              <strong>{t("execution.legacyServiceTitle")}</strong>
              <p>{t("execution.legacyServiceDescription")}</p>
              <span>{`${t(`execution.servicePresence.${legacyService.presence}`)} · ${t(`execution.serviceState.${legacyService.state}`)}`}</span>
              {legacyService.registryConflict && <p className="warning-text">{t("execution.serviceRegistryConflict")}</p>}
              {legacyService.presence === "foreign" && <p className="warning-text">{t("execution.legacyServiceForeignDescription")}</p>}
              {legacyService.presence === "inaccessible" && <p className="warning-text">{t("execution.legacyServiceUncertainDescription")}</p>}
            </div>
            <div className="service-actions">
              <button className="button secondary" disabled={!executionView.canMigrateLegacy} onClick={() => setMigrationConfirmationOpen(true)} type="button">{serviceBusy === "migrate-from-legacy" ? t("execution.serviceWorking") : t("execution.migrateLegacy")}</button>
              <button className="button secondary danger" disabled={!executionView.canRemoveLegacy} onClick={() => void manageService("remove-legacy")} type="button">{serviceBusy === "remove-legacy" ? t("execution.serviceWorking") : t("execution.removeLegacy")}</button>
            </div>
          </div>
        )}
        <div className="system-mode-note"><ShieldAlert aria-hidden="true" size={19} /><p>{status ? t("execution.systemNote") : t("execution.checking")}</p></div>
        </div>
      </details>

      <div className="service-row service-row-static" data-kind="autostart">
        <div className="service-row-head execution-option">
          <span className="service-row-icon"><Power aria-hidden="true" size={19} /></span>
          <span className="service-row-copy"><h2 id="autostart-title">{t("execution.autostartTitle")}</h2><p>{t("execution.autostartDescription")}</p></span>
          <label className="switch-control"><input aria-labelledby="autostart-title" checked={status?.autoStart ?? false} disabled={!status} onChange={(event) => void toggleAutostart(event.target.checked)} role="switch" type="checkbox" /><span aria-hidden="true">{status?.autoStart ? t("common.on") : t("common.off")}</span></label>
        </div>
      </div>

      <details className="service-row" data-kind="registered">
        <summary>
          <span className="service-row-icon"><ListChecks aria-hidden="true" size={19} /></span>
          <span className="service-row-copy"><h2>{t("execution.registeredTitle")}</h2><p>{t("execution.registeredDescription")}</p></span>
          <span className="service-row-state" data-tone={status?.sessionTargets.length ? "normal" : "neutral"}>{t("execution.registeredCount", { count: status?.sessionTargets.length ?? 0 })}</span>
          <ChevronDown aria-hidden="true" className="service-row-chevron" size={17} />
        </summary>
        <div className="service-row-body">
          <div className="registered-launchers">
            <div className="registered-heading"><button className="button secondary" disabled={!status?.injectionReady || !status.sessionTargets.length} onClick={() => void launchAll()} type="button"><Play aria-hidden="true" size={16} /> {t("execution.launchRegistered")}</button></div>
            {status?.sessionTargets.length ? <ul>{status.sessionTargets.map((entry) => <li key={entry.target}><code>{entry.target}</code><button aria-label={t("execution.removeTarget", { name: entry.target })} className="icon-button" onClick={() => void remove(entry.target)} type="button"><Trash2 aria-hidden="true" size={16} /></button></li>)}</ul> : <p className="empty-state">{t("execution.noRegistered")}</p>}
          </div>
        </div>
      </details>

      <details className="service-row" data-kind="manual" onToggle={(event) => { if (event.currentTarget.open && candidates === null) void loadCandidates(); }}>
        <summary>
          <span className="service-row-icon"><FileCode2 aria-hidden="true" size={19} /></span>
          <span className="service-row-copy"><h2>{t("execution.manualTitle")}</h2><p>{t("execution.manualDescription")}</p></span>
          <span className="service-row-state" data-tone="neutral">{target ? target.split(/[\\/]/).pop() : t("execution.noExecutableSelected")}</span>
          <ChevronDown aria-hidden="true" className="service-row-chevron" size={17} />
        </summary>
        <div className="service-row-body">
          <div className="process-picker">
            <div className="process-picker-heading">
              <strong>{t("execution.processListTitle")}</strong>
              <div className="process-picker-tools">
                <input aria-label={t("execution.processListFilter")} className="process-picker-filter" onChange={(event) => setCandidateFilter(event.target.value)} placeholder={t("execution.processListFilter")} type="search" value={candidateFilter} />
                <button className="button secondary" onClick={() => void loadCandidates()} type="button"><RefreshCw aria-hidden="true" size={16} /> {t("execution.processListRefresh")}</button>
              </div>
            </div>
            {visibleCandidates.length ? (
              <ul className="process-picker-list">
                {visibleCandidates.map((candidate) => (
                  <li key={candidate.pid}>
                    <label className="process-picker-row">
                      <input checked={target === candidate.path} name="manual-launch-candidate" onChange={() => setTarget(candidate.path)} type="radio" value={candidate.path} />
                      <strong>{candidate.name}</strong>
                      {candidate.windowTitle && <span className="process-picker-window">{candidate.windowTitle}</span>}
                      <span className="process-picker-pid">{t("execution.processPid", { pid: candidate.pid })}</span>
                    </label>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="empty-state">{t("execution.processListEmpty")}</p>
            )}
          </div>
          <div className="manual-launcher">
            <div className="target-picker">
              <span>{t("execution.browseFallback")}</span>
              <div className="target-selection" data-empty={!target}>
                <FileCode2 aria-hidden="true" size={22} />
                <div>
                  <strong>{target.split(/[\\/]/).pop() || t("execution.noExecutableSelected")}</strong>
                  {target && <code title={target}>{target}</code>}
                </div>
                <button className="button secondary" onClick={() => void chooseTarget()} type="button"><FolderOpen aria-hidden="true" size={17} /> {target ? t("execution.changeExecutable") : t("execution.chooseExecutable")}</button>
              </div>
            </div>
            <label><span>{t("execution.arguments")}</span><textarea onChange={(event) => setArgumentsText(event.target.value)} placeholder={t("execution.argumentsPlaceholder")} rows={3} value={argumentsText} /></label>
            <div className="manual-actions"><button className="button secondary" disabled={!status?.injectionReady || !target.trim()} onClick={() => void register()} type="button"><UserPlus aria-hidden="true" size={17} /> {t("execution.register")}</button><button className="button primary" disabled={!status?.manualLauncherAvailable || !target.trim()} onClick={() => void launch()} type="button"><Play aria-hidden="true" size={17} /> {t("execution.launch")}</button></div>
          </div>
        </div>
      </details>

      </div>

      {message && <p className="success-message">{message}</p>}
      {error && <p className="inline-error"><AlertTriangle aria-hidden="true" size={15} /> {error}</p>}
      {migrationConfirmationOpen && (
        <div className="confirmation-backdrop">
          <section
            aria-labelledby="migration-confirmation-title"
            aria-modal="true"
            className="migration-confirmation"
            onKeyDown={handleMigrationDialogKeyDown}
            role="dialog"
          >
            <div className="migration-confirmation-heading">
              <ShieldAlert aria-hidden="true" size={22} />
              <div>
                <h2 id="migration-confirmation-title">{t("execution.migrationConfirmTitle")}</h2>
                <p>{t("execution.migrationConfirmDescription")}</p>
              </div>
            </div>
            <ol>
              <li>{t("execution.migrationConfirmStrictCheck")}</li>
              <li>{t("execution.migrationConfirmBackup")}</li>
              <li>{t("execution.migrationConfirmSwitch")}</li>
              <li>{t("execution.migrationConfirmRollback")}</li>
            </ol>
            <div className="migration-confirmation-actions">
              <button className="button secondary" onClick={closeMigrationConfirmation} ref={migrationCancelRef} type="button">{t("execution.migrationCancel")}</button>
              <button className="button primary" disabled={!executionView.canMigrateLegacy} onClick={() => void confirmMigration()} type="button">{t("execution.migrationContinue")}</button>
            </div>
          </section>
        </div>
      )}
    </section>
  );
}
