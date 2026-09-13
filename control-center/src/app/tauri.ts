import type { Locale } from "../i18n/i18n";
import type {
  AdvancedProfile,
  AppliedProfile,
  ExecutionStatus,
  ExpectedLegacyTrayIdentity,
  IndividualSetting,
  InstallationStatus,
  LegacyProfileCandidate,
  SystemServiceAction,
  LaunchContext,
  ManualLaunchCandidate,
  NativePreviewOptions,
  NativePreviewState,
  PreviewRequest,
  PreviewResult,
  ProfileEntry,
  ProfileSnapshot,
  EventFilter,
  EventLogSummary,
  EventRecord,
  SessionTarget,
  ViewId,
} from "./model";
import { getRuntimeAdapter } from "./runtimeAdapter";

export async function loadLaunchContext(): Promise<LaunchContext> {
  return getRuntimeAdapter().loadLaunchContext();
}

export async function setApplicationLocale(locale: Locale): Promise<void> {
  return getRuntimeAdapter().setApplicationLocale(locale);
}

export async function loadExecutionStatus(): Promise<ExecutionStatus> {
  return getRuntimeAdapter().loadExecutionStatus();
}

export async function requestLegacyTrayExit(expectedIdentity: ExpectedLegacyTrayIdentity): Promise<ExecutionStatus> {
  return getRuntimeAdapter().requestLegacyTrayExit(expectedIdentity);
}

export async function disableLegacyTrayAutostart(): Promise<ExecutionStatus> {
  return getRuntimeAdapter().disableLegacyTrayAutostart();
}

export async function pickExecutable(filterName: string): Promise<string | null> {
  return getRuntimeAdapter().pickExecutable(filterName);
}

export async function manageSystemService(action: SystemServiceAction): Promise<ExecutionStatus> {
  return getRuntimeAdapter().manageSystemService(action);
}

export async function revealSystemService(): Promise<void> {
  return getRuntimeAdapter().revealSystemService();
}

export async function pickIniProfile(filterName: string): Promise<string | null> {
  return getRuntimeAdapter().pickIniProfile(filterName);
}

export async function pickIniExportPath(filterName: string, defaultName: string): Promise<string | null> {
  return getRuntimeAdapter().pickIniExportPath(filterName, defaultName);
}

export async function loadInstalledFontFamilies(): Promise<ReadonlyArray<string>> {
  return getRuntimeAdapter().loadInstalledFontFamilies();
}

export async function setSessionAutostart(enabled: boolean): Promise<boolean> {
  return getRuntimeAdapter().setSessionAutostart(enabled);
}

export async function launchTargetWithMactype(target: string, arguments_: ReadonlyArray<string>): Promise<number> {
  return getRuntimeAdapter().launchTargetWithMactype(target, arguments_);
}

export async function scanInstallation(): Promise<InstallationStatus | null> {
  return getRuntimeAdapter().scanInstallation();
}

export async function applyOpenProfile(): Promise<AppliedProfile> {
  return getRuntimeAdapter().applyOpenProfile();
}

export async function registerSessionTarget(target: string, arguments_: ReadonlyArray<string>): Promise<ReadonlyArray<SessionTarget>> {
  return getRuntimeAdapter().registerSessionTarget(target, arguments_);
}

export async function removeSessionTarget(target: string): Promise<ReadonlyArray<SessionTarget>> {
  return getRuntimeAdapter().removeSessionTarget(target);
}

export async function launchRegisteredTargets(): Promise<ReadonlyArray<number>> {
  return getRuntimeAdapter().launchRegisteredTargets();
}

export async function listManualLaunchCandidates(): Promise<ReadonlyArray<ManualLaunchCandidate>> {
  return getRuntimeAdapter().listManualLaunchCandidates();
}

export async function rediscoverInstallation(): Promise<InstallationStatus> {
  return getRuntimeAdapter().rediscoverInstallation();
}

export async function reconnectPreview(): Promise<InstallationStatus> {
  return getRuntimeAdapter().reconnectPreview();
}

export async function loadDiagnosticReport(): Promise<string> {
  return getRuntimeAdapter().loadDiagnosticReport();
}

export async function loadDiagnosticLogs(): Promise<ReadonlyArray<string>> {
  return getRuntimeAdapter().loadDiagnosticLogs();
}

export async function loadRecentActivity(): Promise<ReadonlyArray<EventRecord>> {
  return getRuntimeAdapter().loadRecentActivity();
}

export async function listEvents(filter?: EventFilter, limit?: number): Promise<ReadonlyArray<EventRecord>> {
  return getRuntimeAdapter().listEvents(filter, limit);
}

export async function loadEventLogSummary(): Promise<EventLogSummary> {
  return getRuntimeAdapter().loadEventLogSummary();
}

export function subscribeEventLog(listener: () => void): () => void {
  return getRuntimeAdapter().subscribeEventLog(listener);
}

export async function exportDiagnostics(): Promise<string> {
  return getRuntimeAdapter().exportDiagnostics();
}

export async function copyDiagnostics(): Promise<void> {
  return getRuntimeAdapter().copyDiagnostics();
}

export async function openLogFolder(): Promise<string> {
  return getRuntimeAdapter().openLogFolder();
}

export async function openDefaultProfile(): Promise<ProfileSnapshot | null> {
  return getRuntimeAdapter().openDefaultProfile();
}

export async function currentProfile(): Promise<ProfileSnapshot | null> {
  return getRuntimeAdapter().currentProfile();
}

export async function discoverLegacyProfile(): Promise<LegacyProfileCandidate | null> {
  return getRuntimeAdapter().discoverLegacyProfile();
}

export async function importProfile(path: string): Promise<ProfileSnapshot> {
  return getRuntimeAdapter().importProfile(path);
}

export async function listProfiles(): Promise<ReadonlyArray<ProfileEntry>> {
  return getRuntimeAdapter().listProfiles();
}

export async function openProfile(path: string): Promise<ProfileSnapshot> {
  return getRuntimeAdapter().openProfile(path);
}

export async function duplicateProfile(name: string): Promise<ProfileSnapshot> {
  return getRuntimeAdapter().duplicateProfile(name);
}

export async function updateProfileSetting(settingId: string, value: number): Promise<ProfileSnapshot | null> {
  return getRuntimeAdapter().updateProfileSetting(settingId, value);
}

export async function updateProfileIndividuals(entries: ReadonlyArray<IndividualSetting>): Promise<ProfileSnapshot | null> {
  return getRuntimeAdapter().updateProfileIndividuals(entries);
}

export async function updateProfileList(kind: string, entries: ReadonlyArray<string>): Promise<ProfileSnapshot | null> {
  return getRuntimeAdapter().updateProfileList(kind, entries);
}

export async function updateProfileAdvanced(advanced: AdvancedProfile): Promise<ProfileSnapshot | null> {
  return getRuntimeAdapter().updateProfileAdvanced(advanced);
}

export async function undoProfile(): Promise<ProfileSnapshot> {
  return getRuntimeAdapter().undoProfile();
}

export async function redoProfile(): Promise<ProfileSnapshot> {
  return getRuntimeAdapter().redoProfile();
}

export async function resetProfileDefaults(): Promise<ProfileSnapshot> {
  return getRuntimeAdapter().resetProfileDefaults();
}

export async function discardProfileChanges(): Promise<ProfileSnapshot> {
  return getRuntimeAdapter().discardProfileChanges();
}

export async function exportProfile(path: string): Promise<string> {
  return getRuntimeAdapter().exportProfile(path);
}

export async function revealProfileFile(): Promise<string> {
  return getRuntimeAdapter().revealProfileFile();
}

export async function saveProfile(): Promise<ProfileSnapshot | null> {
  return getRuntimeAdapter().saveProfile();
}

export async function renderProfilePreview(request: PreviewRequest): Promise<PreviewResult | null> {
  return getRuntimeAdapter().renderProfilePreview(request);
}

export async function setNativePreview(visible: boolean, options?: NativePreviewOptions): Promise<NativePreviewState> {
  return getRuntimeAdapter().setNativePreview(visible, options);
}

export function subscribeNativePreview(listener: (state: NativePreviewState) => void): () => void {
  return getRuntimeAdapter().subscribeNativePreview(listener);
}

export function previewImageUrl(path: string): string {
  return getRuntimeAdapter().previewImageUrl(path);
}

export async function loadPreviewDiagnostics(): Promise<ReadonlyArray<string>> {
  return getRuntimeAdapter().loadPreviewDiagnostics();
}

export async function forcePreviewCrashForCi(): Promise<void> {
  return getRuntimeAdapter().forcePreviewCrashForCi();
}

export async function verifyProfileWorkflowForCi(): Promise<void> {
  return getRuntimeAdapter().verifyProfileWorkflowForCi();
}

export async function verifyInjectionWorkflowForCi(): Promise<void> {
  return getRuntimeAdapter().verifyInjectionWorkflowForCi();
}

export async function verifyTrayModeForCi(): Promise<void> {
  return getRuntimeAdapter().verifyTrayModeForCi();
}

export async function reportFrontendReady(view: ViewId): Promise<void> {
  return getRuntimeAdapter().reportFrontendReady(view);
}

export async function reportFrontendFailure(view: ViewId, message: string): Promise<void> {
  return getRuntimeAdapter().reportFrontendFailure(view, message);
}
