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
  const managementPackageReady = serviceManagementPackage === "ready";
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
    : "stopped";
  const serviceRuntime = serviceRuntimeValues.find((runtime) => runtime === requestedServiceRuntime)
    ?? defaultServiceRuntime;
  const serviceStable = serviceRuntime === "running" || serviceRuntime === "stopped";
  const serviceBackend = fixture === "foreign-service"
    ? "foreign"
    : fixture === "inaccessible-service"
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
  const generalMutationAllowed = managementPackageReady
    && serviceBackend === "open-source"
    && serviceStable
    && !appInitConflict;
  const systemModesSupported = generalMutationAllowed
    && (serviceInstallation === "absent" || serviceInstallation === "current" || serviceInstallation === "outdated");
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
  const legacyStable = legacyState === "running" || legacyState === "stopped";
  const legacyRetired = query.get("legacy-retired") === "1";
  const legacyTray = galleryLegacyTrayStatus(query);
  const legacyTrayClear = legacyTray.conflict === "clear";
  const conflictFreeMutationAllowed = generalMutationAllowed && legacyTrayClear;
  const liveServiceHealth = ready
    ? "ready"
    : fixture === "degraded"
      ? "degraded"
      : fixture === "initializing"
        ? "initializing"
        : fixture === "failed"
          ? "failed"
          : "unknown";
  const conflictFreeSystemModesSupported = systemModesSupported && legacyTrayClear;

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
      canInstall: conflictFreeMutationAllowed && serviceInstallation === "absent",
      canRemove: conflictFreeMutationAllowed && (serviceInstallation === "current" || serviceInstallation === "outdated"),
      canStart: conflictFreeMutationAllowed && serviceInstallation === "current" && serviceRuntime === "stopped",
      canStop: managementPackageReady && serviceBackend === "open-source" && serviceRuntime === "running",
      canRepair: conflictFreeMutationAllowed && serviceInstallation === "current",
      canUpgrade: conflictFreeMutationAllowed && serviceInstallation === "outdated",
    },
    legacyMacTray: legacyRequested ? {
      ...galleryLegacyService,
      presence: legacyForeign
        ? "foreign"
        : legacyUncertain
          ? "inaccessible"
          : galleryLegacyService.presence,
      state: legacyUncertain ? "unknown" : legacyState,
      win32Error: legacyUncertain ? 5 : null,
      registryConflict: appInitConflict,
      trustedBinaryAvailable: !legacyForeign && !legacyUncertain,
      canRemove: false,
      canStop: !legacyForeign && !legacyUncertain && !appInitConflict && legacyState === "running",
      migrationAvailable: !legacyRetired
        && !legacyForeign
        && !legacyUncertain
        && conflictFreeSystemModesSupported
        && legacyStable,
      blocksActivation: !legacyRetired,
    } : null,
    legacyTray,
    registryModeDetected: appInitConflict,
    systemModesSupported: conflictFreeSystemModesSupported,
    systemInjectionActive: legacyTrayClear && (query.has("raw-active") ? true : ready && !appInitConflict),
    injectionReady: !query.has("profile-runtime-missing"),
    activeProfile,
    expectedProfileDigest: activeProfile ? expectedGalleryDigest : null,
    sessionTargets: [],
  };
}

function withGalleryLegacyTrayPolicy(
  current: ExecutionStatus,
  legacyTray: LegacyTrayStatus,
): ExecutionStatus {
  const service = current.systemService;
  const serviceStable = service.runtime === "running" || service.runtime === "stopped";
  const mutationAllowed = legacyTray.conflict === "clear"
    && current.serviceManagementPackage === "ready"
    && !current.registryModeDetected
    && service.backend === "open-source"
    && serviceStable;
  const systemModesSupported = mutationAllowed
    && (service.installation === "absent"
      || service.installation === "current"
      || service.installation === "outdated");
  const systemInjectionActive = systemModesSupported
    && service.runtime === "running"
    && service.health === "ready"
    && Boolean(current.expectedProfileDigest)
    && service.activeProfileDigest === current.expectedProfileDigest;
  const legacyStable = current.legacyMacTray?.state === "running"
    || current.legacyMacTray?.state === "stopped";

  return {
    ...current,
    legacyTray,
    systemModesSupported,
    systemInjectionActive,
    systemService: {
      ...service,
      configurationDrift: false,
      canInstall: mutationAllowed && service.installation === "absent",
      canRemove: mutationAllowed && (service.installation === "current" || service.installation === "outdated"),
      canStart: mutationAllowed && service.installation === "current" && service.runtime === "stopped",
      canStop: current.serviceManagementPackage === "ready"
        && service.backend === "open-source"
        && service.runtime === "running",
      canRepair: mutationAllowed && service.installation === "current",
      canUpgrade: mutationAllowed && service.installation === "outdated",
    },
    legacyMacTray: current.legacyMacTray
      ? {
          ...current.legacyMacTray,
          migrationAvailable: current.legacyMacTray.presence !== "foreign"
            && current.legacyMacTray.blocksActivation
            && systemModesSupported
            && legacyStable,
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
    canInstall: false,
    canRemove: true,
    canStart: false,
    canStop: true,
    canRepair: false,
    canUpgrade: false,
  };
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
        canStart: !current.registryModeDetected && current.systemService.installation === "current",
        canStop: false,
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
        installation: "absent",
        runtime: "stopped",
        health: "unknown",
        activeProfileDigest: null,
        canInstall: true,
        canRemove: false,
        canStart: false,
        canStop: false,
        canRepair: false,
        canUpgrade: false,
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
