import type { ExecutionStatus, LegacyMacTrayStatus, LegacyTrayStatus, SystemServiceAction, SystemServiceStatus } from "./model";
import type { MessageKey } from "../i18n/i18n";

type SystemInjectionState = "loading" | "active" | "running-unverified" | "running-profile-mismatch" | "running-appinit-conflict" | "running-legacy-tray-conflict" | "legacy-service-migrate" | "inactive" | "unavailable";

export type LegacyTrayResolutionKind =
  | "exit-current-process"
  | "other-session"
  | "untrusted-process"
  | "unknown-process"
  | "disable-autostart"
  | "untrusted-autostart"
  | "unknown-autostart";

export interface LegacyTrayResolution {
  kind: LegacyTrayResolutionKind;
  titleKey: MessageKey;
  descriptionKey: MessageKey;
  canRequestExit: boolean;
  canDisableStartup: boolean;
}

export interface SystemInjectionPrimaryAction {
  intent: "activate" | "stop";
  command: "publish-profile" | "stop";
  enabled: boolean;
  state: SystemInjectionState;
  titleKey: MessageKey;
  descriptionKey: MessageKey;
  labelKey: MessageKey;
}

export interface ServiceSummaryAction {
  command: SystemServiceAction;
  enabled: boolean;
  labelKey: MessageKey;
  tone: "primary" | "secondary" | "danger";
}

export type ServiceSummaryNoticeKind =
  | "appinit-conflict"
  | "configuration-drift"
  | "foreign-service"
  | "legacy-service"
  | "legacy-service-foreign"
  | "legacy-service-uncertain"
  | "migration"
  | "profile-mismatch"
  | "repair"
  | "removal-pending"
  | "upgrade";

export interface ServiceSummaryNotice {
  kind: ServiceSummaryNoticeKind;
  titleKey: MessageKey;
  descriptionKey?: MessageKey;
}

export interface ServiceSummary {
  modeKey: MessageKey;
  statusKey: MessageKey;
  tone: "normal" | "neutral" | "attention" | "critical";
  notice: ServiceSummaryNotice | null;
  actions: ReadonlyArray<ServiceSummaryAction>;
}

export interface ServiceStatusLine {
  installationKey: MessageKey;
  runtimeKey: MessageKey;
  healthKey: MessageKey | null;
}

export type ProfileIndicatorKind = "matched" | "mismatch" | "unverified" | "service-off";

export interface ProfileIndicator {
  kind: ProfileIndicatorKind;
  labelKey: MessageKey;
}

export interface ExecutionViewModel {
  status: ExecutionStatus | null;
  profileMatches: boolean;
  profileIndicator: ProfileIndicator;
  serviceStatusLine: ServiceStatusLine | null;
  serviceNeedsUpgrade: boolean;
  serviceNeedsRepair: boolean;
  serviceBinaryPath: string | null;
  serviceSummary: ServiceSummary;
  systemInjectionAction: SystemInjectionPrimaryAction;
  legacyTrayResolution: LegacyTrayResolution | null;
  canInstall: boolean;
  canStart: boolean;
  canUpgrade: boolean;
  canRepair: boolean;
  canRemove: boolean;
  canMigrateLegacy: boolean;
  canRemoveLegacy: boolean;
}

function projectServiceStatusLine(service: SystemServiceStatus): ServiceStatusLine {
  // A stopped service reports no live health. Only a persisted failure or
  // degradation record is worth repeating; anything else would read as a
  // present-tense diagnosis of a service the user deliberately turned off.
  const healthKey: MessageKey | null = service.runtime === "stopped"
    ? service.health === "failed"
      ? "execution.health.failed"
      : service.health === "degraded"
        ? "execution.health.degradedStopped"
        : null
    : `execution.health.${service.health}` as MessageKey;
  return {
    installationKey: `execution.installation.${service.installation}` as MessageKey,
    runtimeKey: `execution.serviceState.${service.runtime}` as MessageKey,
    healthKey,
  };
}

function projectProfileIndicator(
  status: ExecutionStatus | null,
  profileMatches: boolean,
): ProfileIndicator {
  const service = status?.systemService;
  if (service?.runtime === "stopped") {
    // No generation is active while the service is off; reporting a mismatch
    // would blame a profile for a state the user chose.
    return { kind: "service-off", labelKey: "execution.profileServiceOff" };
  }
  if (profileMatches) {
    return { kind: "matched", labelKey: "execution.profileMatched" };
  }
  const mismatch = Boolean(
    status?.expectedProfileDigest
      && service?.activeProfileDigest
      && service.activeProfileDigest !== status.expectedProfileDigest,
  );
  return mismatch
    ? { kind: "mismatch", labelKey: "execution.profileMismatch" }
    : { kind: "unverified", labelKey: "execution.profileUnverified" };
}

function projectServiceSummary(
  status: ExecutionStatus | null,
  serviceBusy: string | null,
  canInstall: boolean,
  canStart: boolean,
  canUpgrade: boolean,
  canRepair: boolean,
  canRemove: boolean,
  canMigrateLegacy: boolean,
): ServiceSummary {
  const service = status?.systemService;
  const idle = serviceBusy === null;
  const legacyTrayConflict = Boolean(status && status.legacyTray.conflict !== "clear");
  const legacyBlocksActivation = legacyServiceBlocksActivation(status?.legacyMacTray);
  const profileMismatch = Boolean(
    status?.expectedProfileDigest
      && service?.activeProfileDigest
      && service.activeProfileDigest !== status.expectedProfileDigest,
  );
  const foreignService = service?.backend === "foreign" || service?.installation === "invalid";
  const inaccessibleService = service?.installation === "inaccessible";
  const driftedService = service?.backend === "open-source" && service.configurationDrift === true;
  const serviceRemovalPending = service?.installation === "delete-pending";
  const modeKey: MessageKey = status?.registryModeDetected
    ? "execution.modeAppInit"
    : legacyBlocksActivation
      ? "execution.modeLegacy"
      : service?.backend === "open-source" && service.installation !== "absent"
        ? "execution.modeNative"
        : "execution.modeNone";

  let statusKey: MessageKey = "execution.checking";
  let tone: ServiceSummary["tone"] = "normal";
  if (service?.health === "failed") {
    statusKey = "execution.statusRepair";
    tone = "critical";
  } else if (
    status?.registryModeDetected
    || legacyTrayConflict
    || legacyBlocksActivation
    || profileMismatch
    || foreignService
    || inaccessibleService
    || driftedService
    || serviceRemovalPending
    || service?.installation === "outdated"
  ) {
    statusKey = "execution.statusAttention";
    tone = "attention";
  } else if (service?.runtime === "running") {
    // A degraded report can be caused by one or two process-local misses. Keep
    // that diagnostic in Details instead of turning the page summary into an alarm.
    statusKey = "execution.statusRunning";
  } else if (service?.runtime === "stopped") {
    statusKey = "execution.serviceState.stopped";
    tone = "neutral";
  } else if (service) {
    statusKey = `execution.serviceState.${service.runtime}` as MessageKey;
  }

  let notice: ServiceSummaryNotice | null = null;
  if (!legacyTrayConflict && status?.registryModeDetected) {
    notice = service?.runtime === "running"
      ? {
          kind: "appinit-conflict",
          titleKey: "execution.systemAppInitConflictTitle",
          descriptionKey: "execution.systemAppInitConflictDescription",
        }
      : {
          kind: "appinit-conflict",
          titleKey: "execution.serviceRegistryConflict",
        };
  } else if (!legacyTrayConflict && legacyBlocksActivation) {
    const legacy = status?.legacyMacTray;
    if (legacy?.presence === "foreign") {
      notice = {
        kind: "legacy-service-foreign",
        titleKey: "execution.legacyServiceForeignTitle",
        descriptionKey: "execution.legacyServiceForeignDescription",
      };
    } else if (legacy?.presence === "inaccessible") {
      notice = {
        kind: "legacy-service-uncertain",
        titleKey: "execution.legacyServiceUncertainTitle",
        descriptionKey: "execution.legacyServiceUncertainDescription",
      };
    } else {
      notice = canMigrateLegacy
        ? {
            kind: "migration",
            titleKey: "execution.legacyDetected",
            descriptionKey: "execution.systemLegacyServiceMigrateDescription",
          }
        : {
            kind: "legacy-service",
            titleKey: "execution.systemLegacyServiceMigrateTitle",
            descriptionKey: "execution.systemLegacyServiceMigrateDescription",
          };
    }
  } else if (!legacyTrayConflict && (foreignService || inaccessibleService)) {
    notice = {
      kind: "foreign-service",
      titleKey: inaccessibleService ? "execution.installation.inaccessible" : "execution.serviceForeign",
    };
  } else if (!legacyTrayConflict && driftedService) {
    notice = {
      kind: "configuration-drift",
      titleKey: "execution.serviceConfigurationDriftTitle",
      descriptionKey: "execution.serviceConfigurationDriftDescription",
    };
  } else if (!legacyTrayConflict && serviceRemovalPending) {
    notice = {
      kind: "removal-pending",
      titleKey: "execution.installation.delete-pending",
    };
  } else if (!legacyTrayConflict && service?.health === "failed") {
    notice = {
      kind: "repair",
      titleKey: "execution.repairRequired",
    };
  } else if (!legacyTrayConflict && profileMismatch) {
    notice = {
      kind: "profile-mismatch",
      titleKey: "execution.systemProfileMismatchTitle",
      descriptionKey: "execution.systemProfileMismatchDescription",
    };
  } else if (!legacyTrayConflict && service?.installation === "outdated") {
    notice = {
      kind: "upgrade",
      titleKey: "execution.installation.outdated",
    };
  }

  const action = (
    command: SystemServiceAction,
    labelKey: MessageKey,
    actionEnabled: boolean,
    actionTone: ServiceSummaryAction["tone"] = "primary",
  ): ServiceSummaryAction => ({ command, enabled: idle && actionEnabled, labelKey, tone: actionTone });

  let actions: ReadonlyArray<ServiceSummaryAction> = [];
  if (legacyTrayConflict || foreignService || inaccessibleService || serviceRemovalPending) {
    actions = [];
  } else if (canMigrateLegacy) {
    actions = [action("migrate-from-legacy", "execution.migrateLegacy", true)];
  } else if (status?.registryModeDetected || legacyBlocksActivation) {
    if (service?.runtime === "running") {
      actions = [action("stop", "execution.serviceStop", service.canStop, "secondary")];
    }
  } else if (driftedService) {
    actions = service.installation === "outdated"
      ? [action("upgrade", "execution.serviceUpgrade", canUpgrade)]
      : [action("repair", "execution.serviceRepair", canRepair)];
    if (service.runtime === "running") {
      actions = [...actions, action("stop", "execution.serviceStop", service.canStop, "secondary")];
    }
  } else if (service?.health === "failed") {
    if (canRepair) actions = [action("repair", "execution.serviceRepair", true)];
  } else if (service?.installation === "outdated") {
    actions = [action("upgrade", "execution.serviceUpgrade", canUpgrade)];
  } else if (service?.runtime === "running") {
    actions = [action("stop", "execution.serviceStop", service.canStop, "secondary")];
  } else if (service?.runtime === "stopped" && service.installation === "current") {
    // With no alternative service to fall back to, stopping is a genuine fork:
    // resume the service or remove it. Both choices belong at the same level.
    actions = [
      action("start", "execution.serviceStart", canStart),
      action("remove", "execution.serviceRemove", canRemove, "danger"),
    ];
  } else if (service?.installation === "absent") {
    actions = [action("install", "execution.serviceInstall", canInstall)];
  }

  return { modeKey, statusKey, tone, notice, actions };
}

const systemInjectionCopy: Record<SystemInjectionState, { titleKey: MessageKey; descriptionKey: MessageKey }> = {
  loading: {
    titleKey: "execution.checking",
    descriptionKey: "execution.checking",
  },
  active: {
    titleKey: "execution.systemActiveTitle",
    descriptionKey: "execution.systemActiveDescription",
  },
  "running-unverified": {
    titleKey: "execution.systemRunningUnverifiedTitle",
    descriptionKey: "execution.systemRunningUnverifiedDescription",
  },
  "running-profile-mismatch": {
    titleKey: "execution.systemProfileMismatchTitle",
    descriptionKey: "execution.systemProfileMismatchDescription",
  },
  "running-appinit-conflict": {
    titleKey: "execution.systemAppInitConflictTitle",
    descriptionKey: "execution.systemAppInitConflictDescription",
  },
  "running-legacy-tray-conflict": {
    titleKey: "execution.systemLegacyTrayConflictTitle",
    descriptionKey: "execution.systemLegacyTrayConflictDescription",
  },
  "legacy-service-migrate": {
    titleKey: "execution.systemLegacyServiceMigrateTitle",
    descriptionKey: "execution.systemLegacyServiceMigrateDescription",
  },
  inactive: {
    titleKey: "execution.systemInactiveTitle",
    descriptionKey: "execution.systemInactiveDescription",
  },
  unavailable: {
    titleKey: "execution.systemUnavailableTitle",
    descriptionKey: "execution.systemUnavailableDescription",
  },
};

function legacyServiceCopy(
  legacy: LegacyMacTrayStatus | null | undefined,
): { titleKey: MessageKey; descriptionKey: MessageKey } {
  if (legacy?.presence === "foreign") {
    return {
      titleKey: "execution.legacyServiceForeignTitle",
      descriptionKey: "execution.legacyServiceForeignDescription",
    };
  }
  if (legacy?.presence === "inaccessible") {
    return {
      titleKey: "execution.legacyServiceUncertainTitle",
      descriptionKey: "execution.legacyServiceUncertainDescription",
    };
  }
  return systemInjectionCopy["legacy-service-migrate"];
}

function projectLegacyTrayResolution(status: LegacyTrayStatus | undefined): LegacyTrayResolution | null {
  if (!status || status.conflict === "clear") return null;

  switch (status.process.state) {
    case "trusted-current-session":
      return {
        kind: "exit-current-process",
        titleKey: "execution.legacyTrayRunningTitle",
        descriptionKey: "execution.legacyTrayRunningDescription",
        canRequestExit: status.canRequestExit,
        canDisableStartup: false,
      };
    case "trusted-other-session":
      return {
        kind: "other-session",
        titleKey: "execution.legacyTrayOtherSessionTitle",
        descriptionKey: "execution.legacyTrayOtherSessionDescription",
        canRequestExit: false,
        canDisableStartup: false,
      };
    case "untrusted-same-name":
      return {
        kind: "untrusted-process",
        titleKey: "execution.legacyTrayUntrustedTitle",
        descriptionKey: "execution.legacyTrayUntrustedDescription",
        canRequestExit: false,
        canDisableStartup: false,
      };
    case "unknown":
      return {
        kind: "unknown-process",
        titleKey: "execution.legacyTrayUnknownTitle",
        descriptionKey: "execution.legacyTrayUnknownDescription",
        canRequestExit: false,
        canDisableStartup: false,
      };
    case "absent":
      break;
  }

  switch (status.startup.state) {
    case "detected":
      return {
        kind: "disable-autostart",
        titleKey: "execution.legacyTrayAutostartTitle",
        descriptionKey: "execution.legacyTrayAutostartDescription",
        canRequestExit: false,
        canDisableStartup: status.canDisableStartup,
      };
    case "untrusted":
      return {
        kind: "untrusted-autostart",
        titleKey: "execution.legacyTrayAutostartUntrustedTitle",
        descriptionKey: "execution.legacyTrayAutostartUntrustedDescription",
        canRequestExit: false,
        canDisableStartup: false,
      };
    case "unknown":
      return {
        kind: "unknown-autostart",
        titleKey: "execution.legacyTrayAutostartUnknownTitle",
        descriptionKey: "execution.legacyTrayAutostartUnknownDescription",
        canRequestExit: false,
        canDisableStartup: false,
      };
    case "absent":
      return null;
  }
}

function legacyServiceBlocksActivation(legacy: LegacyMacTrayStatus | null | undefined): boolean {
  return Boolean(legacy?.blocksActivation);
}

function projectSystemInjectionAction(
  status: ExecutionStatus | null,
  serviceBusy: string | null,
  profileMatches: boolean,
): SystemInjectionPrimaryAction {
  const service = status?.systemService;
  const running = service?.runtime === "running";
  const profileMismatch = Boolean(
    status?.expectedProfileDigest
      && service?.activeProfileDigest
      && service.activeProfileDigest !== status.expectedProfileDigest,
  );
  const legacyTrayConflict = Boolean(status && status.legacyTray.conflict !== "clear");
  const legacyBlocksActivation = legacyServiceBlocksActivation(status?.legacyMacTray);
  const verifiedActive = Boolean(
    status?.systemInjectionActive
      && running
      && service?.health === "ready"
      && profileMatches
      && !status.registryModeDetected
      && !legacyTrayConflict
      && !legacyBlocksActivation,
  );

  let state: SystemInjectionState;
  if (!status) state = "loading";
  else if (running && legacyTrayConflict) state = "running-legacy-tray-conflict";
  else if (running && status.registryModeDetected) state = "running-appinit-conflict";
  else if (verifiedActive) state = "active";
  else if (running && profileMismatch) state = "running-profile-mismatch";
  else if (running) state = "running-unverified";
  else if (legacyBlocksActivation) state = "legacy-service-migrate";
  else if (service?.runtime === "stopped") state = "inactive";
  else state = "unavailable";

  const intent = running ? "stop" : "activate";
  const idle = serviceBusy === null;
  const enabled = Boolean(
    idle
      && status
      && (intent === "stop"
        ? service?.canStop
        : service?.runtime === "stopped"
          && status.systemModesSupported
          && status.legacyTray.conflict === "clear"
          && !legacyBlocksActivation),
  );

  const copy = state === "legacy-service-migrate"
    ? legacyServiceCopy(status?.legacyMacTray)
    : state === "inactive" && !status?.activeProfile
      // Starting without an applied profile auto-applies the bundled default;
      // disclose that here, before the click, instead of surprising afterwards.
      ? { ...systemInjectionCopy.inactive, descriptionKey: "execution.systemInactiveNoProfileDescription" as const }
      : systemInjectionCopy[state];

  return {
    intent,
    command: intent === "stop" ? "stop" : "publish-profile",
    enabled,
    state,
    ...copy,
    labelKey: serviceBusy
      ? "execution.serviceWorking"
      : intent === "stop"
        ? "execution.systemPause"
        : "execution.systemApply",
  };
}

export function projectExecutionView(
  status: ExecutionStatus | null,
  serviceBusy: string | null,
): ExecutionViewModel {
  const service = status?.systemService;
  const driftedService = service?.backend === "open-source" && service.configurationDrift === true;
  const legacy = status?.legacyMacTray;
  const idle = serviceBusy === null;
  const legacyTrayConflict = Boolean(status && status.legacyTray.conflict !== "clear");
  const legacyBlocksActivation = legacyServiceBlocksActivation(legacy);
  const profileMatches = Boolean(
    status?.expectedProfileDigest
      && service?.activeProfileDigest === status.expectedProfileDigest,
  );
  const legacyMigrationComplete = Boolean(
    status?.systemInjectionActive
      && service?.backend === "open-source"
      && service.installation === "current"
      && service.runtime === "running"
      && service.health === "ready"
      && profileMatches
      && legacy?.state === "stopped"
      && !status.registryModeDetected
      && !legacyTrayConflict,
  );

  const canInstall = Boolean(idle && !legacyTrayConflict && !legacyBlocksActivation && service?.canInstall);
  const canStart = Boolean(idle && !legacyTrayConflict && !legacyBlocksActivation && service?.canStart);
  const canUpgrade = Boolean(idle && !legacyTrayConflict && service?.canUpgrade);
  const canRepair = Boolean(idle && !legacyTrayConflict && service?.canRepair);
  const canRemove = Boolean(idle && !legacyTrayConflict && service?.canRemove);
  const canMigrateLegacy = Boolean(
    idle && !legacyTrayConflict && legacy?.migrationAvailable && !legacyMigrationComplete
  );

  return {
    status,
    profileMatches,
    profileIndicator: projectProfileIndicator(status, profileMatches),
    serviceStatusLine: service ? projectServiceStatusLine(service) : null,
    serviceNeedsUpgrade: service?.installation === "outdated",
    serviceNeedsRepair: (service?.installation === "current" && service.health === "failed")
      || (driftedService && service.installation !== "outdated"),
    serviceBinaryPath: service?.installation === "absent" ? null : service?.binaryPath ?? null,
    serviceSummary: projectServiceSummary(
      status,
      serviceBusy,
      canInstall,
      canStart,
      canUpgrade,
      canRepair,
      canRemove,
      canMigrateLegacy,
    ),
    systemInjectionAction: projectSystemInjectionAction(status, serviceBusy, profileMatches),
    legacyTrayResolution: projectLegacyTrayResolution(status?.legacyTray),
    canInstall,
    canStart,
    canUpgrade,
    canRepair,
    canRemove,
    canMigrateLegacy,
    canRemoveLegacy: Boolean(idle && !legacyTrayConflict && legacy?.canRemove),
  };
}
