import type {
  ExecutionStatus,
  ExpectedLegacyTrayIdentity,
  LegacyMacTrayStatus,
  LegacyTrayProcessState,
  LegacyTrayStartupState,
  LegacyTrayStatus,
  ServiceManagementPackageState,
  ServiceRuntimeState,
  SystemServiceAction,
  SystemServiceStatus,
} from "../model";
import { projectServiceCapabilities } from "./serviceCapabilityPolicy";
export const expectedGalleryDigest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

type GalleryQuery = Pick<URLSearchParams, "get" | "has">;

const absentLegacyTrayProcess: LegacyTrayProcessState = { state: "absent" };
const absentLegacyTrayStartup: LegacyTrayStartupState = { state: "absent" };

function createLegacyTrayStatus(
  process: LegacyTrayProcessState,
  startup: LegacyTrayStartupState,
): LegacyTrayStatus {
  const conflict = process.state === "unknown" || startup.state === "unknown"
    ? "unknown"
    : process.state === "absent" && startup.state === "absent"
      ? "clear"
      : "detected";
  return {
    process,
    startup,
    conflict,
    canRequestExit: process.state === "trusted-current-session",
    canDisableStartup: startup.state === "detected" && startup.entries.length > 0,
  };
}

function galleryLegacyTrayStatus(query: GalleryQuery): LegacyTrayStatus {
  const processFixture = query.get("legacy-tray");
  const process: LegacyTrayProcessState = processFixture === "trusted-current"
    ? {
        state: "trusted-current-session",
        pid: 4243,
        creationTime: "638883072000000000",
        path: "C:\\Program Files\\MacType\\MacTray.exe",
      }
    : processFixture === "trusted-other"
      ? {
          state: "trusted-other-session",
          sessionId: 2,
          path: "C:\\Program Files\\MacType\\MacTray.exe",
        }
      : processFixture === "untrusted"
        ? {
            state: "untrusted-same-name",
            sessionId: 1,
            path: "C:\\Users\\Gallery\\Downloads\\MacTray.exe",
          }
        : processFixture === "unknown"
          ? {
              state: "unknown",
              error: {
                code: "legacy-tray-process-unavailable",
                message: "The MacTray process identity could not be verified.",
                win32_error: 5,
              },
            }
          : absentLegacyTrayProcess;
  const startupFixture = query.get("legacy-startup");
  const startup: LegacyTrayStartupState = startupFixture === "hkcu-run"
    ? {
        state: "detected",
        entries: [{
          sourceKind: "current-user-run64",
          displayName: "MacType",
          targetPath: "C:\\Program Files\\MacType\\MacTray.exe",
        }],
      }
    : startupFixture === "untrusted"
      ? {
          state: "untrusted",
          entries: [{
            sourceKind: "current-user-run64",
            displayName: "MacType",
            targetPath: "C:\\Users\\Gallery\\Downloads\\MacTray.exe",
          }],
        }
      : startupFixture === "unknown"
        ? {
            state: "unknown",
            error: {
              code: "legacy-tray-startup-unavailable",
              message: "The MacTray autostart configuration could not be verified.",
              win32_error: 5,
            },
          }
        : absentLegacyTrayStartup;
  return createLegacyTrayStatus(process, startup);
}

const galleryLegacyService: LegacyMacTrayStatus = {
  presence: "owned",
  state: "running",
  binaryPath: null,
  win32Error: null,
  trustedBinaryAvailable: true,
  registryConflict: false,
  canRemove: true,
  canStop: true,
  migrationAvailable: true,
  migrationBackupAvailable: false,
  blocksActivation: true,
};

const serviceRuntimeValues: ReadonlyArray<ServiceRuntimeState> = [
  "stopped",
  "start-pending",
  "running",
  "stop-pending",
  "paused",
  "unknown",
];

const legacyRuntimeValues: ReadonlyArray<LegacyMacTrayStatus["state"]> = [
  "stopped",
  "start-pending",
  "running",
  "stop-pending",
  "continue-pending",
  "pause-pending",
  "paused",
  "unknown",
];

export function galleryExecutionStatus(query: GalleryQuery): ExecutionStatus {
  const fixture = query.get("system-service") ?? "ready";
  const requestedPackage = query.get("service-package");
  const serviceManagementPackage: ServiceManagementPackageState =
    requestedPackage === "not-installed"
      || requestedPackage === "incomplete"
      || requestedPackage === "untrusted"
      ? requestedPackage
      : "ready";
  const appInitConflict = fixture === "legacy-conflict";
  const profileMismatch = fixture === "profile-mismatch";
  const ready = fixture === "ready" || appInitConflict;
  const requestedServiceRuntime = query.get("service-runtime") ?? fixture;
  const defaultServiceRuntime: ServiceRuntimeState = ready
    || fixture === "degraded"
    || fixture === "initializing"
    || fixture === "unknown-health"
    || profileMismatch
    ? "running"
    : fixture === "inaccessible-service" || fixture === "delete-pending"
      ? "unknown"
      : "stopped";
  const serviceRuntime = serviceRuntimeValues.find((runtime) => runtime === requestedServiceRuntime)
    ?? defaultServiceRuntime;
  const serviceBackend = fixture === "foreign-service"
    ? "foreign"
    : fixture === "inaccessible-service" || fixture === "migration-available"
      ? "none"
      : "open-source";
  const serviceInstallation = fixture === "foreign-service"
    ? "invalid"
    : fixture === "inaccessible-service"
      ? "inaccessible"
    : fixture === "delete-pending"
      ? "delete-pending"
    : fixture === "migration-available"
      ? "absent"
      : fixture === "outdated"
        ? "outdated"
        : "current";
  const activeProfile = query.has("profile-unapplied")
    ? null
    : query.has("legacy-applied")
      ? "Profiles\\Pretendard forever.ini"
      : "ini\\Default.ini";
  const legacyRequest = query.get("legacy");
  const legacyForeign = legacyRequest === "foreign";
  const legacyUncertain = legacyRequest === "inaccessible";
  const legacyRequested = legacyRequest === "migration-available"
    || legacyForeign
    || legacyUncertain
    || fixture === "legacy-conflict";
  const requestedLegacyState = query.get("legacy-state");
  const legacyState = legacyRuntimeValues.find((state) => state === requestedLegacyState) ?? "running";
  const legacyRetired = query.get("legacy-retired") === "1";
  const legacyTray = galleryLegacyTrayStatus(query);
  const legacyTrayClear = legacyTray.conflict === "clear";
  const liveServiceHealth = ready
    ? "ready"
    : fixture === "degraded"
      ? "degraded"
      : fixture === "initializing"
        ? "initializing"
        : fixture === "failed"
          ? "failed"
          : "unknown";
  const legacy: Pick<LegacyMacTrayStatus, "presence" | "state" | "trustedBinaryAvailable"> = {
    presence: !legacyRequested
      ? "absent"
      : legacyForeign
        ? "foreign"
        : legacyUncertain
          ? "inaccessible"
          : galleryLegacyService.presence,
    state: legacyUncertain ? "unknown" : legacyState,
    trustedBinaryAvailable: !legacyForeign && !legacyUncertain,
  };
  const { systemModesSupported, migrationAvailable, ...capabilities } = projectServiceCapabilities({
    runtime: serviceRuntime,
    installation: serviceInstallation,
    configurationDrift: false,
    backend: serviceBackend,
    registryModeDetected: appInitConflict,
    legacyTrayConflict: legacyTray.conflict,
    legacy,
    managementPackage: serviceManagementPackage,
  });

  return {
    trayAvailable: true,
    autoStart: false,
    manualLauncherAvailable: true,
    serviceManagementPackage,
    systemService: {
      backend: serviceBackend,
      installation: serviceInstallation,
      configurationDrift: false,
      runtime: serviceRuntime,
      // The backend reads live health and the active generation only from a
      // running service; a stopped one keeps at most a persisted degraded or
      // failed record.
      health: serviceRuntime === "running" || liveServiceHealth === "degraded" || liveServiceHealth === "failed"
        ? liveServiceHealth
        : "unknown",
      binaryPath: fixture === "migration-available" || fixture === "inaccessible-service"
        ? null
        : fixture === "foreign-service"
          ? "C:\\Program Files\\Unknown\\service.exe"
          : "C:\\Program Files\\MacType Control Center\\Service\\mactype-service.exe",
      win32Error: null,
      activeProfileDigest: serviceRuntime !== "running"
        ? null
        : profileMismatch
          ? "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
          : ready
            ? expectedGalleryDigest
            : null,
      ...capabilities,
    },
    legacyMacTray: legacyRequested ? {
      ...galleryLegacyService,
      ...legacy,
      win32Error: legacyUncertain ? 5 : null,
      registryConflict: appInitConflict,
      canRemove: false,
      canStop: !legacyForeign && !legacyUncertain && !appInitConflict && legacyState === "running",
      migrationAvailable: !legacyRetired && migrationAvailable,
      blocksActivation: !legacyRetired,
    } : null,
    legacyTray,
    registryModeDetected: appInitConflict,
    systemModesSupported,
    systemInjectionActive: legacyTrayClear && (query.has("raw-active") ? true : ready && !appInitConflict),
    injectionReady: !query.has("profile-runtime-missing"),
    activeProfile,
    expectedProfileDigest: activeProfile ? expectedGalleryDigest : null,
    runProfilePublication: !activeProfile || query.has("profile-runtime-missing")
      ? "unknown"
      : query.has("run-profile-pending") ? "pending" : "published",
    sessionTargets: [],
  };
}

function withGalleryLegacyTrayPolicy(
  current: ExecutionStatus,
  legacyTray: LegacyTrayStatus,
): ExecutionStatus {
  const service = current.systemService;
  const { systemModesSupported, migrationAvailable, ...capabilities } = projectServiceCapabilities({
    runtime: service.runtime,
    installation: service.installation,
    configurationDrift: service.configurationDrift,
    backend: service.backend,
    registryModeDetected: current.registryModeDetected,
    legacyTrayConflict: legacyTray.conflict,
    legacy: current.legacyMacTray ?? { presence: "absent", state: "stopped" },
    managementPackage: current.serviceManagementPackage,
  });
  const systemInjectionActive = systemModesSupported
    && legacyTray.conflict === "clear"
    && service.backend === "open-source"
    && service.installation === "current"
    && service.runtime === "running"
    && service.health === "ready"
    && Boolean(current.expectedProfileDigest)
    && service.activeProfileDigest === current.expectedProfileDigest;

  return {
    ...current,
    legacyTray,
    systemModesSupported,
    systemInjectionActive,
    systemService: {
      ...service,
      ...capabilities,
    },
    legacyMacTray: current.legacyMacTray
      ? {
          ...current.legacyMacTray,
          migrationAvailable: current.legacyMacTray.blocksActivation && migrationAvailable,
          canRemove: legacyTray.conflict === "clear" && current.legacyMacTray.canRemove,
        }
      : null,
  };
}

export function transitionGalleryLegacyTrayExit(
  current: ExecutionStatus,
  expectedIdentity: ExpectedLegacyTrayIdentity,
): ExecutionStatus {
  if (!current.legacyTray.canRequestExit
    || current.legacyTray.process.state !== "trusted-current-session") return current;
  const observed = current.legacyTray.process;
  if (observed.pid !== expectedIdentity.pid
    || observed.creationTime !== expectedIdentity.creationTime
    || observed.path !== expectedIdentity.path) return current;
  return withGalleryLegacyTrayPolicy(
    current,
    createLegacyTrayStatus(absentLegacyTrayProcess, current.legacyTray.startup),
  );
}

export function transitionGalleryLegacyTrayAutostartDisable(current: ExecutionStatus): ExecutionStatus {
  if (!current.legacyTray.canDisableStartup) return current;
  return withGalleryLegacyTrayPolicy(
    current,
    createLegacyTrayStatus(current.legacyTray.process, absentLegacyTrayStartup),
  );
}

function runningGalleryService(current: SystemServiceStatus): SystemServiceStatus {
  return {
    ...current,
    backend: "open-source",
    installation: "current",
    configurationDrift: false,
    runtime: "running",
    health: "ready",
    activeProfileDigest: expectedGalleryDigest,
  };
}

/* Designating a run profile moves the pointer; only a running service changes
   state, because it switches to the new profile at once. */
export function transitionGalleryRunProfile(
  current: ExecutionStatus,
  displayPath: string,
  live: boolean,
): ExecutionStatus {
  const next = live ? transitionGalleryExecutionStatus(current, "publish-profile") : current;
  return withGalleryLegacyTrayPolicy({
    ...next,
    activeProfile: displayPath,
    expectedProfileDigest: expectedGalleryDigest,
    runProfilePublication: "published",
  }, next.legacyTray);
}

export function transitionGalleryExecutionStatus(
  current: ExecutionStatus,
  action: SystemServiceAction,
): ExecutionStatus {
  if (action === "stop") {
    return withGalleryLegacyTrayPolicy({
      ...current,
      systemInjectionActive: false,
      systemService: {
        ...current.systemService,
        configurationDrift: false,
        runtime: "stopped",
        health: "unknown",
        activeProfileDigest: null,
      },
    }, current.legacyTray);
  }
  if (action === "remove") {
    return withGalleryLegacyTrayPolicy({
      ...current,
      systemInjectionActive: false,
      systemService: {
        ...current.systemService,
        configurationDrift: false,
        backend: "none",
        installation: "absent",
        runtime: "stopped",
        health: "unknown",
        activeProfileDigest: null,
      },
    }, current.legacyTray);
  }
  if (action === "remove-legacy") {
    return withGalleryLegacyTrayPolicy({ ...current, legacyMacTray: null }, current.legacyTray);
  }

  // Mirrors the backend contract: starting or publishing with no applied
  // profile applies the bundled default profile first.
  const defaultApplied = (action === "start" || action === "publish-profile") && !current.activeProfile;
  return withGalleryLegacyTrayPolicy({
    ...current,
    activeProfile: defaultApplied ? "ini\\Default.ini" : current.activeProfile,
    expectedProfileDigest: defaultApplied ? expectedGalleryDigest : current.expectedProfileDigest,
    runProfilePublication: defaultApplied ? "published" : current.runProfilePublication,
    systemInjectionActive: true,
    systemService: runningGalleryService(current.systemService),
    legacyMacTray: action === "migrate-from-legacy" && current.legacyMacTray
      ? {
          ...current.legacyMacTray,
          state: "stopped",
          canStop: false,
          canRemove: true,
          migrationBackupAvailable: true,
          blocksActivation: false,
        }
      : current.legacyMacTray,
  }, current.legacyTray);
}
