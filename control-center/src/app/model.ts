export type ViewId = "overview" | "files" | "profiles" | "execution" | "diagnostics";

export interface LaunchContext {
  view: ViewId;
  ciSmoke: boolean;
  trayStart: boolean;
  previewStudioSmoke?: boolean;
}

export interface InstallationStatus {
  state: "ready" | "incomplete" | "not-found";
  root: string | null;
  coreVersion: string | null;
  findings: ReadonlyArray<{ label: string; value: string; ok: boolean }>;
}

export interface DiagnosticEntry {
  time: string;
  area: string;
  message: string;
  severity: "info" | "warning" | "error";
}

export interface ExecutionStatus {
  trayAvailable: boolean;
  autoStart: boolean;
  manualLauncherAvailable: boolean;
  serviceManagementPackage: ServiceManagementPackageState;
  systemService: SystemServiceStatus;
  legacyMacTray: LegacyMacTrayStatus | null;
  legacyTray: LegacyTrayStatus;
  registryModeDetected: boolean;
  /** Backend-authoritative capability for publishing and applying a profile. */
  systemModesSupported: boolean;
  systemInjectionActive: boolean;
  injectionReady: boolean;
  activeProfile: string | null;
  expectedProfileDigest: string | null;
  sessionTargets: ReadonlyArray<SessionTarget>;
}

export interface StructuredServiceError {
  code: string;
  message: string;
  win32_error: number | null;
}

export type LegacyTrayProcessState =
  | { state: "absent" }
  | { state: "trusted-current-session"; pid: number; creationTime: string; path: string }
  | { state: "trusted-other-session"; sessionId: number; path: string }
  | { state: "untrusted-same-name"; sessionId: number | null; path: string | null }
  | { state: "unknown"; error: StructuredServiceError };

export type LegacyTrayStartupSource =
  | "current-user-run32"
  | "current-user-run64"
  | "local-machine-run32"
  | "local-machine-run64"
  | "current-user-startup";

export interface LegacyTrayStartupEntry {
  sourceKind: LegacyTrayStartupSource;
  displayName: string;
  targetPath: string;
}

export type LegacyTrayStartupState =
  | { state: "absent" }
  | { state: "detected"; entries: ReadonlyArray<LegacyTrayStartupEntry> }
  | { state: "untrusted"; entries: ReadonlyArray<LegacyTrayStartupEntry> }
  | { state: "unknown"; error: StructuredServiceError };

export type LegacyTrayConflictState = "clear" | "detected" | "unknown";

export interface LegacyTrayStatus {
  process: LegacyTrayProcessState;
  startup: LegacyTrayStartupState;
  conflict: LegacyTrayConflictState;
  canRequestExit: boolean;
  canDisableStartup: boolean;
}

export interface ExpectedLegacyTrayIdentity {
  pid: number;
  creationTime: string;
  path: string;
}

export interface SessionTarget {
  target: string;
  arguments: ReadonlyArray<string>;
}

export interface ManualLaunchCandidate {
  pid: number;
  name: string;
  path: string;
  windowTitle: string | null;
}

export interface AppliedProfile {
  sourceProfile: string;
  runtimeRoot: string;
}

export interface ProfileSnapshot {
  path: string;
  displayPath: string;
  location: "installation" | "personal" | "external";
  canSave: boolean;
  encoding: string;
  bom: string;
  lineEnding: string;
  originalHash: string;
  values: Record<string, number>;
  savedValues: Record<string, number>;
  dirtyKeys: ReadonlyArray<string>;
  canUndo: boolean;
  canRedo: boolean;
  individuals: ReadonlyArray<IndividualSetting>;
  lists: ProfileLists;
  advanced: AdvancedProfile;
}

export interface ProfileEntry {
  name: string;
  path: string;
  displayPath: string;
}

export type ServiceBackend = "open-source" | "legacy-mac-tray" | "foreign" | "none";
export type ServiceManagementPackageState = "ready" | "not-installed" | "incomplete" | "untrusted";
export type InstallationState = "absent" | "current" | "outdated" | "invalid" | "inaccessible" | "delete-pending";
export type ServiceRuntimeState = "stopped" | "start-pending" | "running" | "stop-pending" | "paused" | "unknown";
export type ServiceHealthState = "unknown" | "initializing" | "ready" | "degraded" | "failed";

export interface SystemServiceStatus {
  backend: ServiceBackend;
  installation: InstallationState;
  configurationDrift: boolean;
  runtime: ServiceRuntimeState;
  health: ServiceHealthState;
  binaryPath: string | null;
  win32Error: number | null;
  activeProfileDigest: string | null;
  canInstall: boolean;
  canRemove: boolean;
  canStart: boolean;
  canStop: boolean;
  canRepair: boolean;
  canUpgrade: boolean;
}

export interface LegacyMacTrayStatus {
  presence: "absent" | "owned" | "compatible-unquoted" | "foreign" | "delete-pending" | "inaccessible";
  state: ServiceRuntimeState | "continue-pending" | "pause-pending";
  binaryPath: string | null;
  win32Error: number | null;
  trustedBinaryAvailable: boolean;
  registryConflict: boolean;
  canRemove: boolean;
  canStop: boolean;
  migrationAvailable: boolean;
  migrationBackupAvailable: boolean;
  blocksActivation: boolean;
}

export type SystemServiceAction = "install" | "upgrade" | "repair" | "remove" | "start" | "stop" | "publish-profile" | "migrate-from-legacy" | "remove-legacy";

export interface LegacyProfileCandidate {
  name: string;
  path: string;
  source: "alternative-file";
}

export interface IndividualSetting {
  fontFace: string;
  values: Array<number | null>;
}

export interface ProfileLists {
  excludeFonts: ReadonlyArray<string>;
  includeFonts: ReadonlyArray<string>;
  excludeModules: ReadonlyArray<string>;
  includeModules: ReadonlyArray<string>;
  unloadDlls: ReadonlyArray<string>;
  excludeSubstitutionModules: ReadonlyArray<string>;
}

export interface ShadowSetting {
  offsetX: number;
  offsetY: number;
  darkAlpha: number;
  darkColor: number;
  lightAlpha: number;
  lightColor: number;
}

export interface AdvancedProfile {
  shadow: ShadowSetting | null;
  lcdFilterWeight: ReadonlyArray<number> | null;
  pixelLayout: ReadonlyArray<number> | null;
  fontSubstitutes: ReadonlyArray<string>;
}

export interface PreviewSample {
  text: string;
  fontFace: string;
  fontSizePt: number;
  widthPx: number;
  heightPx: number;
  dpi: number;
  foreground: string;
  background: string;
  /** Optional GDI style flags; the preview helper defaults both to false. */
  bold?: boolean;
  italic?: boolean;
}

/** Which helper renders a sample: MacType's renderer, or Windows' own GDI. */
export type PreviewEngine = "mactype" | "plain";

export interface PreviewRequest {
  profilePath: string;
  overrides: Record<string, number>;
  sample: PreviewSample;
  displayScale: number;
  engine?: PreviewEngine;
}

/** How the helper-owned native preview window lays out its canvas. */
export type NativePreviewMode = "sample" | "ladder" | "compare" | "listing";

/** Everything the native window needs; omitted fields keep the window's previous value. */
export interface NativePreviewOptions {
  displayMode?: NativePreviewMode;
  text?: string;
  listingText?: string;
  fontFace?: string;
  fontSizePt?: number;
  bold?: boolean;
  italic?: boolean;
  foreground?: string;
  background?: string;
  theme?: "light" | "dark";
  inverted?: boolean;
  zoom?: 1 | 2 | 4;
  sizes?: ReadonlyArray<number>;
  labels?: Readonly<Record<string, string>>;
  /** Colours and metrics of the Control Center window. */
  chrome?: NativePreviewChrome;
}

export interface NativePreviewChrome {
  skin: "classic";
  canvas: string;
  surface: string;
  surfaceSubtle: string;
  border: string;
  text: string;
  muted: string;
  accent: string;
  onAccent: string;
  radius: number;
  controlHeight: number;
  toolbarHeight: number;
  statusHeight: number;
  canvasRadius: number;
  canvasInset: number;
  monoStatus: boolean;
}

/** What the native window reports after a show or hide request. */
export interface NativePreviewState {
  visible: boolean;
  displayMode: NativePreviewMode;
  background: string;
  foreground?: string;
  inverted?: boolean;
  zoom?: number;
  fontFace?: string;
  fontSizePt?: number;
  bold?: boolean;
  italic?: boolean;
  topmost?: boolean;
  /** Skin id the window is drawing its chrome from; absent without chrome. */
  skin?: "classic";
}

export interface PreviewResult {
  requestId: number;
  imagePath: string;
  width: number;
  height: number;
  dpi: number;
  elapsedMs: number;
  coreVersion: number;
  engine?: PreviewEngine;
}

export type EventSeverity = "info" | "notice" | "warning" | "error";
export type EventArea = "service" | "setup" | "profile" | "preview" | "injection" | "control-center" | "tray";
export type EventSource = "service-host" | "service-setup" | "control-center";

/** One line of the unified event log; `code` is localised by the UI. */
export interface EventRecord {
  v: number;
  ts: number;
  severity: EventSeverity;
  area: EventArea;
  code: string;
  params: Record<string, string>;
  detail: string | null;
  source: EventSource;
}

export interface EventFilter {
  severities?: ReadonlyArray<EventSeverity>;
  areas?: ReadonlyArray<EventArea>;
  sinceUnixMs?: number;
}

export interface EventSourceStatus {
  source: EventSource;
  path: string;
  present: boolean;
  readable: boolean;
  bytes: number;
}

export interface EventLogSummary {
  total: number;
  warnings: number;
  errors: number;
  newestTs: number | null;
  sources: ReadonlyArray<EventSourceStatus>;
}

export const fallbackStatus: InstallationStatus = {
  state: "incomplete",
  root: "C:\\Program Files\\MacType",
  coreVersion: "1.2025.6.9",
  findings: [
    { label: "core32", value: "MacType.dll", ok: true },
    { label: "core64", value: "MacType64.dll", ok: true },
    { label: "preview", value: "waiting", ok: false },
  ],
};
