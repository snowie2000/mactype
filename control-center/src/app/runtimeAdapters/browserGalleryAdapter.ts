import {
  fallbackStatus,
  type AppliedProfile,
  type ExecutionStatus,
  type InstallationStatus,
  type LegacyProfileCandidate,
  type LaunchContext,
  type ManualLaunchCandidate,
  type NativePreviewOptions,
  type NativePreviewState,
  type ProfileEntry,
  type PreviewRequest,
  type PreviewResult,
  type ProfileSnapshot,
  type EventFilter,
  type EventLogSummary,
  type EventRecord,
  type SessionTarget,
} from "../model";
import type { ControlCenterRuntimeAdapter } from "../runtimeAdapter";
import { normalizeEvents } from "../../features/events/normalizeEvent";
import {
  galleryExecutionStatus,
  transitionGalleryLegacyTrayAutostartDisable,
  transitionGalleryLegacyTrayExit,
  transitionGalleryExecutionStatus,
} from "./browserGalleryExecution";
import { createBrowserGalleryProfileState } from "./browserGalleryProfiles";

const galleryProfiles = createBrowserGalleryProfileState();
let galleryPreviewRequestId = 0;
let galleryExecutionState: { location: string; status: ExecutionStatus } | null = null;

function currentGalleryExecutionStatus(): ExecutionStatus {
  const location = window.location.href;
  if (!galleryExecutionState || galleryExecutionState.location !== location) {
    galleryExecutionState = {
      location,
      status: galleryExecutionStatus(new URLSearchParams(window.location.search)),
    };
  }
  return galleryExecutionState.status;
}

function updateGalleryExecutionStatus(status: ExecutionStatus): ExecutionStatus {
  galleryExecutionState = { location: window.location.href, status };
  return status;
}

function incrementGalleryCounter(key: string): void {
  const current = Number(window.sessionStorage.getItem(key) ?? "0");
  window.sessionStorage.setItem(key, String(current + 1));
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/** Deterministic SVG stand-in for the native preview renderer so gallery runs show stable thumbnails. */
function galleryPreviewImage(request: PreviewRequest): string {
  const { sample } = request;
  const fontSizePx = Math.max(8, Math.round(sample.fontSizePt * (sample.dpi / 72)));
  const lineHeight = Math.round(fontSizePx * 1.5);
  const inset = Math.round(fontSizePx * 0.75);
  const style = `${sample.bold ? ' font-weight="bold"' : ""}${sample.italic ? ' font-style="italic"' : ""}`;
  const text = sample.text
    .split("\n")
    .map((line, index) => `<text x="${inset}" y="${inset + lineHeight * (index + 1) - Math.round(fontSizePx * 0.35)}" fill="${sample.foreground}" font-family="${escapeXml(sample.fontFace)}, sans-serif" font-size="${fontSizePx}"${style}>${escapeXml(line)}</text>`)
    .join("");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${sample.widthPx}" height="${sample.heightPx}"><rect width="100%" height="100%" fill="${sample.background}"/>${text}</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/* Cross-window messages in the browser gallery stay inside one document. */
const galleryStudioBus = new EventTarget();

function galleryEvents(): ReadonlyArray<EventRecord> {
  const now = Date.now();
  const minute = 60_000;
  const query = new URLSearchParams(window.location.search);
  const events: EventRecord[] = [
    { v: 1, ts: now - 26 * 60 * minute, severity: "info", area: "control-center", code: "app-started", params: { version: "0.1.0" }, detail: null, source: "control-center" },
    { v: 1, ts: now - 25 * 60 * minute, severity: "info", area: "service", code: "service-installed", params: {}, detail: null, source: "control-center" },
    { v: 1, ts: now - 25 * 60 * minute + 8_000, severity: "info", area: "service", code: "service-started", params: { version: "0.1.0" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 24 * 60 * minute, severity: "info", area: "injection", code: "injection-summary", params: { injected: "14", failed: "0", skipped: "3" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 11 * 60 * minute, severity: "warning", area: "injection", code: "injection-failed", params: { process: "vgtray.exe", reason: "protected-process" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 10 * 60 * minute, severity: "warning", area: "injection", code: "injection-failed", params: { process: "vgtray.exe", reason: "protected-process" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 10 * 60 * minute + 30_000, severity: "warning", area: "injection", code: "injection-failed", params: { process: "firefox.exe", reason: "module-load-failed" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 9 * 60 * minute, severity: "warning", area: "injection", code: "injection-failed", params: { process: "vgtray.exe", reason: "protected-process" }, detail: "helper disposition: protected-process-light (PPL) refused module load; exact identity pid=4180 creation=133700000000000000", source: "service-host" },
    { v: 1, ts: now - 8 * 60 * minute, severity: "notice", area: "service", code: "service-health-changed", params: { state: "degraded", code: "observer-restarted" }, detail: "WMI process-creation subscription was re-established after a transient RPC failure (0x800706BA).", source: "service-host" },
    { v: 1, ts: now - 8 * 60 * minute + 30_000, severity: "info", area: "service", code: "service-health-changed", params: { state: "ready" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 3 * 60 * minute, severity: "error", area: "setup", code: "operation-failed", params: { operation: "upgrade", stage: "installation-preflight", rollback: "not-applicable" }, detail: "installation-preflight: the installed Control Center does not match the running executable\nfinalState=legacy=Absent/Stopped/win32=None; modern=Current/Running/Ready/win32=None; receipt=unavailable", source: "control-center" },
    { v: 1, ts: now - 55 * minute, severity: "info", area: "injection", code: "injection-summary", params: { injected: "12", failed: "1", skipped: "2" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 40 * minute, severity: "error", area: "injection", code: "helper-broker-failed", params: { architecture: "x64", code: "renderer-evidence-thread-failed" }, detail: "pid=26172 creation_time=134336993656899217 win32=Some(5)", source: "service-host" },
    { v: 1, ts: now - 35 * minute, severity: "info", area: "injection", code: "injection-summary", params: { injected: "8", failed: "0", skipped: "1" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 25 * minute, severity: "error", area: "control-center", code: "panic", params: { thread: "main", message: "cannot move state from Destroyed", location: "tao-0.35.3/src/platform_impl/windows/event_loop/runner.rs:371:25" }, detail: "0: gallery::event_loop::runner\n1: gallery::application::run", source: "control-center" },
    { v: 1, ts: now - 20 * minute, severity: "info", area: "injection", code: "injection-summary", params: { injected: "6", failed: "0", skipped: "0" }, detail: null, source: "service-host" },
    { v: 1, ts: now - 12 * minute, severity: "info", area: "preview", code: "preview-helper-connected", params: { architecture: "x86", coreVersion: "1.2025.6.9" }, detail: null, source: "control-center" },
    { v: 1, ts: now - 60_000, severity: "info", area: "profile", code: "profile-verified", params: { profile: "Default.ini" }, detail: null, source: "control-center" },
    /* The backend omits empty params and a missing detail on the wire; this
       line arrives exactly like that so the UI proves it copes. */
    { v: 1, ts: now - 45_000, severity: "info", area: "service", code: "service-started", source: "control-center" } as unknown as EventRecord,
    { v: 1, ts: now - 15_000, severity: "info", area: "profile", code: "profile-applied", params: { profile: "Default.ini" }, detail: null, source: "control-center" },
  ];
  if (query.has("events-empty")) return [];
  return normalizeEvents(events);
}

export const browserGalleryAdapter: ControlCenterRuntimeAdapter = {
  loadLaunchContext(): Promise<LaunchContext> {
    const query = new URLSearchParams(window.location.search);
    const requested = query.get("view");
    return Promise.resolve<LaunchContext>({
      view: requested === "files" || requested === "profiles" || requested === "execution" || requested === "diagnostics" ? requested : "overview",
      ciSmoke: query.has("ci-smoke"),
      trayStart: false,
    });
  },

  setApplicationLocale(): Promise<void> {
    return Promise.resolve();
  },

  loadExecutionStatus(): Promise<ExecutionStatus> {
    return Promise.resolve(currentGalleryExecutionStatus());
  },

  requestLegacyTrayExit(expectedIdentity): Promise<ExecutionStatus> {
    return Promise.resolve(updateGalleryExecutionStatus(
      transitionGalleryLegacyTrayExit(currentGalleryExecutionStatus(), expectedIdentity),
    ));
  },

  disableLegacyTrayAutostart(): Promise<ExecutionStatus> {
    return Promise.resolve(updateGalleryExecutionStatus(
      transitionGalleryLegacyTrayAutostartDisable(currentGalleryExecutionStatus()),
    ));
  },

  manageSystemService(action): Promise<ExecutionStatus> {
    const query = new URLSearchParams(window.location.search);
    if (query.get("service-fail") === action) {
      return Promise.reject(new Error(`control-center-internal-operation-failed:${action}`));
    }
    const current = currentGalleryExecutionStatus();
    const next = transitionGalleryExecutionStatus(current, action);
    const delay = Number(query.get("service-delay"));
    if (Number.isFinite(delay) && delay > 0) {
      return new Promise((resolve) => window.setTimeout(() => resolve(updateGalleryExecutionStatus(next)), delay));
    }
    return Promise.resolve(updateGalleryExecutionStatus(next));
  },

  revealSystemService(): Promise<void> {
    return Promise.resolve();
  },

  pickExecutable(): Promise<string | null> {
    return Promise.resolve("C:\\Windows\\System32\\notepad.exe");
  },

  pickIniProfile(): Promise<string | null> {
    return Promise.resolve("C:\\Users\\Gallery\\Downloads\\Community.ini");
  },

  pickIniExportPath(_filterName, defaultName): Promise<string | null> {
    return Promise.resolve(`C:\\Users\\Gallery\\Documents\\${defaultName}`);
  },

  loadInstalledFontFamilies(): Promise<ReadonlyArray<string>> {
    return Promise.resolve(["Segoe UI", "Arial", "Calibri", "Cambria", "Consolas", "맑은 고딕", "Microsoft YaHei UI", "Microsoft JhengHei UI", "Meiryo", "Tahoma"]);
  },

  setSessionAutostart(enabled: boolean): Promise<boolean> {
    return Promise.resolve(enabled);
  },

  launchTargetWithMactype(): Promise<number> {
    return Promise.resolve(4242);
  },

  scanInstallation(): Promise<InstallationStatus | null> {
    return Promise.resolve(null);
  },

  applyOpenProfile(): Promise<AppliedProfile> {
    const query = new URLSearchParams(window.location.search);
    if (query.get("service-fail") === "publish-profile") {
      return Promise.reject(new Error("control-center-internal-operation-failed:publish-profile"));
    }
    updateGalleryExecutionStatus(
      transitionGalleryExecutionStatus(currentGalleryExecutionStatus(), "publish-profile"),
    );
    return Promise.resolve<AppliedProfile>({ sourceProfile: galleryProfiles.current().displayPath, runtimeRoot: "C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\runtime\\generations\\gallery" });
  },

  registerSessionTarget(target: string, arguments_: ReadonlyArray<string>): Promise<ReadonlyArray<SessionTarget>> {
    return Promise.resolve<ReadonlyArray<SessionTarget>>([{ target, arguments: arguments_ }]);
  },

  removeSessionTarget(): Promise<ReadonlyArray<SessionTarget>> {
    return Promise.resolve([]);
  },

  launchRegisteredTargets(): Promise<ReadonlyArray<number>> {
    return Promise.resolve([4242]);
  },

  listManualLaunchCandidates(): Promise<ReadonlyArray<ManualLaunchCandidate>> {
    return Promise.resolve<ReadonlyArray<ManualLaunchCandidate>>([
      { pid: 5678, name: "code.exe", path: "C:\\Tools\\VSCode\\code.exe", windowTitle: "Visual Studio Code" },
      { pid: 4242, name: "notepad.exe", path: "C:\\Tools\\notepad.exe", windowTitle: "제목 없음 - 메모장" },
      { pid: 6110, name: "agent.exe", path: "C:\\Tools\\Agent\\agent.exe", windowTitle: null },
      { pid: 7130, name: "syncworker.exe", path: "C:\\Tools\\Sync\\syncworker.exe", windowTitle: null },
      { pid: 7240, name: "updater.exe", path: "C:\\Tools\\Updater\\updater.exe", windowTitle: null },
    ]);
  },

  rediscoverInstallation(): Promise<InstallationStatus> {
    return Promise.resolve<InstallationStatus>({ ...fallbackStatus, state: "ready" });
  },

  reconnectPreview(): Promise<InstallationStatus> {
    return Promise.resolve<InstallationStatus>({
      ...fallbackStatus,
      state: "ready",
      findings: fallbackStatus.findings.map((finding) => finding.label === "preview" ? { ...finding, value: "connected", ok: true } : finding),
    });
  },

  loadDiagnosticReport(): Promise<string> {
    return Promise.resolve(`MacType Control Center diagnostics\ncontrolCenterVersion=0.1.0\ncoreVersion=${fallbackStatus.coreVersion}\n`);
  },

  loadDiagnosticLogs(): Promise<ReadonlyArray<string>> {
    return Promise.resolve([
      "1784459527000 operation=migrate-from-legacy stage=verify open service readiness error=strict Ready timed out rollback=completed finalState=legacy=Running/Auto; modern=Absent",
    ]);
  },

  loadRecentActivity(): Promise<ReadonlyArray<EventRecord>> {
    return Promise.resolve(galleryEvents().filter((event) => (event.severity === "info" || event.severity === "notice") && event.area !== "setup").slice(-8));
  },

  listEvents(filter?: EventFilter, limit?: number): Promise<ReadonlyArray<EventRecord>> {
    const events = galleryEvents().filter((event) =>
      (!filter?.severities || filter.severities.includes(event.severity))
      && (!filter?.areas || filter.areas.includes(event.area))
      && (filter?.sinceUnixMs === undefined || event.ts >= filter.sinceUnixMs));
    return Promise.resolve(events.slice(-(limit ?? 200)));
  },

  loadEventLogSummary(): Promise<EventLogSummary> {
    const events = galleryEvents();
    const query = new URLSearchParams(window.location.search);
    return Promise.resolve({
      total: events.length,
      warnings: events.filter((event) => event.severity === "warning").length,
      errors: events.filter((event) => event.severity === "error").length,
      newestTs: events.at(-1)?.ts ?? null,
      sources: [
        { source: "control-center", path: "C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\logs\\control-center.log", present: true, readable: true, bytes: 18_432 },
        { source: "service-host", path: "C:\\ProgramData\\MacType\\ControlCenter\\logs\\service-host.log", present: true, readable: true, bytes: 61_204 },
        { source: "service-setup", path: "C:\\ProgramData\\MacType\\ControlCenter\\logs\\service-setup.log", present: !query.has("events-absent"), readable: !query.has("events-unreadable"), bytes: 2_310 },
      ],
    });
  },

  subscribeEventLog(): () => void {
    return () => undefined;
  },

  exportDiagnostics(): Promise<string> {
    return Promise.resolve("C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\logs\\diagnostics-gallery.txt");
  },

  copyDiagnostics(): Promise<void> {
    return Promise.resolve();
  },

  openLogFolder(): Promise<string> {
    return Promise.resolve("C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\logs");
  },

  openDefaultProfile(): Promise<ProfileSnapshot | null> {
    return Promise.resolve(galleryProfiles.openDefault());
  },

  currentProfile(): Promise<ProfileSnapshot | null> {
    if (new URLSearchParams(window.location.search).has("fresh")) return Promise.resolve(null);
    galleryProfiles.setCanSave(!new URLSearchParams(window.location.search).has("profile-read-only"));
    return Promise.resolve(galleryProfiles.current());
  },

  discoverLegacyProfile(): Promise<LegacyProfileCandidate | null> {
    if (new URLSearchParams(window.location.search).get("legacy-profile") === "external") {
      return Promise.resolve({ name: "External", path: "C:\\Users\\Gallery\\Downloads\\External.ini", source: "alternative-file" });
    }
    return Promise.resolve({ name: "Pretendard forever", path: "C:\\Program Files\\MacType\\ini\\pretendard forever.ini", source: "alternative-file" });
  },

  importProfile(path: string): Promise<ProfileSnapshot> {
    return Promise.resolve(galleryProfiles.import(path));
  },

  listProfiles(): Promise<ReadonlyArray<ProfileEntry>> {
    return Promise.resolve(galleryProfiles.list());
  },

  openProfile(path: string): Promise<ProfileSnapshot> {
    return Promise.resolve(galleryProfiles.open(path));
  },

  duplicateProfile(name: string): Promise<ProfileSnapshot> {
    return Promise.resolve(galleryProfiles.duplicate(name));
  },

  updateProfileSetting(settingId, value): Promise<ProfileSnapshot | null> {
    if (new URLSearchParams(window.location.search).get("profile-fail-setting") === settingId) {
      return Promise.reject(new Error("Gallery profile mutation failed."));
    }
    return Promise.resolve(galleryProfiles.updateSetting(settingId, value));
  },

  updateProfileIndividuals(entries): Promise<ProfileSnapshot | null> {
    return Promise.resolve(galleryProfiles.updateIndividuals(entries));
  },

  updateProfileList(kind, entries): Promise<ProfileSnapshot | null> {
    return Promise.resolve(galleryProfiles.updateList(kind, entries));
  },

  updateProfileAdvanced(advanced): Promise<ProfileSnapshot | null> {
    return Promise.resolve(galleryProfiles.updateAdvanced(advanced));
  },

  undoProfile(): Promise<ProfileSnapshot> {
    return Promise.resolve(galleryProfiles.undo());
  },

  redoProfile(): Promise<ProfileSnapshot> {
    return Promise.resolve(galleryProfiles.redo());
  },

  discardProfileChanges(): Promise<ProfileSnapshot> {
    return Promise.resolve(galleryProfiles.discard());
  },
  resetProfileDefaults(): Promise<ProfileSnapshot> {
    return Promise.resolve(galleryProfiles.resetDefaults());
  },

  exportProfile(path: string): Promise<string> {
    return Promise.resolve(path);
  },

  revealProfileFile(): Promise<string> {
    return Promise.resolve(galleryProfiles.current().path);
  },

  saveProfile(): Promise<ProfileSnapshot | null> {
    return Promise.resolve(galleryProfiles.save());
  },

  renderProfilePreview(request): Promise<PreviewResult | null> {
    const delay = Number(new URLSearchParams(window.location.search).get("preview-delay"));
    const delayed = Number.isFinite(delay) && delay > 0;
    const result: PreviewResult = {
      requestId: ++galleryPreviewRequestId,
      imagePath: galleryPreviewImage(request),
      width: request.sample.widthPx,
      height: request.sample.heightPx,
      dpi: request.sample.dpi,
      elapsedMs: delayed ? delay : 0,
      coreVersion: 0,
    };
    if (!delayed) return Promise.resolve(result);
    incrementGalleryCounter("gallery-preview-started");
    return new Promise((resolve) => window.setTimeout(() => resolve(result), delay));
  },

  setNativePreview(visible: boolean, options?: NativePreviewOptions): Promise<NativePreviewState> {
    const mode = options?.displayMode ?? "sample";
    window.sessionStorage.setItem("gallery-native-preview", visible ? mode : "hidden");
    window.sessionStorage.setItem("gallery-native-preview-background", visible ? options?.background ?? "" : "hidden");
    window.sessionStorage.setItem("gallery-native-preview-options", JSON.stringify(options ?? {}));
    return Promise.resolve({
      visible,
      displayMode: mode,
      background: options?.background ?? "#EEF1F4",
      foreground: options?.foreground ?? "#181D23",
      inverted: options?.inverted ?? false,
      zoom: options?.zoom ?? 1,
      fontFace: options?.fontFace ?? "Segoe UI",
      fontSizePt: options?.fontSizePt ?? 14,
      topmost: false,
    });
  },

  /* The gallery stands in for the window: a test dispatches
     `gallery-native-preview-state` with the state as `detail`. */
  subscribeNativePreview(listener: (state: NativePreviewState) => void): () => void {
    const handler = (event: Event) => {
      const state = (event as CustomEvent<NativePreviewState>).detail;
      if (!state.visible) window.sessionStorage.setItem("gallery-native-preview", "hidden");
      listener(state);
    };
    window.addEventListener("gallery-native-preview-state", handler);
    return () => window.removeEventListener("gallery-native-preview-state", handler);
  },

  openPreviewStudio(): Promise<void> {
    if (new URLSearchParams(window.location.search).has("studio-open-error")) return Promise.reject(new Error("Preview Studio could not open"));
    window.sessionStorage.setItem("gallery-preview-studio", "open");
    return Promise.resolve();
  },

  reportPreviewStudioReady: () => Promise.resolve(),

  closePreviewStudio(): Promise<void> {
    window.sessionStorage.setItem("gallery-preview-studio", "closed");
    return Promise.resolve();
  },

  pickPngExportPath(_filterName: string, defaultName: string): Promise<string | null> {
    return Promise.resolve(`C:\\Users\\Gallery\\Pictures\\${defaultName}`);
  },

  writePreviewExport(path: string): Promise<string> {
    window.sessionStorage.setItem("gallery-preview-export", path);
    return Promise.resolve(path);
  },

  emitStudioMessage(channel: string, payload: unknown): Promise<void> {
    galleryStudioBus.dispatchEvent(new CustomEvent(channel, { detail: payload }));
    return Promise.resolve();
  },

  subscribeStudioMessage<T>(channel: string, listener: (payload: T) => void): () => void {
    const handler = (event: Event) => listener((event as CustomEvent<T>).detail);
    galleryStudioBus.addEventListener(channel, handler);
    return () => galleryStudioBus.removeEventListener(channel, handler);
  },

  windowLabel(): string {
    return new URLSearchParams(window.location.search).get("window") === "preview-studio" ? "preview-studio" : "main";
  },

  previewImageUrl(path: string): string {
    return path;
  },

  loadPreviewDiagnostics(): Promise<ReadonlyArray<string>> {
    return Promise.resolve([]);
  },

  forcePreviewCrashForCi: () => {
    incrementGalleryCounter("gallery-preview-crashes");
    return Promise.resolve();
  },
  verifyProfileWorkflowForCi: () => Promise.resolve(),
  verifyInjectionWorkflowForCi: () => Promise.resolve(),
  verifyTrayModeForCi: () => Promise.resolve(),
  reportFrontendReady: (view) => {
    if (view === "profiles") incrementGalleryCounter("gallery-profile-ready");
    return Promise.resolve();
  },
  reportFrontendFailure: () => Promise.resolve(),
};
