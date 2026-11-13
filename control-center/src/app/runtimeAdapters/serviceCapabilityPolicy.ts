import type {
  InstallationState,
  LegacyMacTrayStatus,
  LegacyTrayConflictState,
  ServiceBackend,
  ServiceManagementPackageState,
  ServiceRuntimeState as RuntimeState,
} from "../model";

export interface ServiceCapabilityInput {
  runtime: RuntimeState;
  installation: InstallationState;
  configurationDrift: boolean;
  backend: ServiceBackend;
  registryModeDetected: boolean;
  legacyTrayConflict: LegacyTrayConflictState;
  legacy: {
    presence: LegacyMacTrayStatus["presence"];
    state: LegacyMacTrayStatus["state"];
    migrationAvailable?: boolean;
    trustedBinaryAvailable?: boolean;
  };
  managementPackage: ServiceManagementPackageState;
}

export interface ServiceCapabilityProjection {
  canInstall: boolean;
  canRemove: boolean;
  canStart: boolean;
  canStop: boolean;
  canRepair: boolean;
  canUpgrade: boolean;
  systemModesSupported: boolean;
  migrationAvailable: boolean;
}

export function projectServiceCapabilities(input: ServiceCapabilityInput): ServiceCapabilityProjection {
  const stable = input.runtime === "running" || input.runtime === "stopped";
  const none = {
    canInstall: false,
    canRemove: false,
    canStart: false,
    canStop: false,
    canRepair: false,
    canUpgrade: false,
  };
  let capabilities = { ...none };
  if (input.backend === "none" && input.installation === "absent") {
    capabilities.canInstall = true;
  } else if (input.backend === "open-source") {
    capabilities = {
      canInstall: false,
      canRemove: stable,
      canStart: input.runtime === "stopped"
        && input.installation === "current"
        && !input.configurationDrift,
      canStop: input.runtime === "running",
      canRepair: stable && input.installation === "current",
      canUpgrade: stable && input.installation === "outdated",
    };
  }

  if (input.registryModeDetected || input.legacyTrayConflict !== "clear") {
    capabilities = {
      ...none,
      canStop: capabilities.canStop && input.backend === "open-source" && input.runtime === "running",
    };
  }
  if (input.managementPackage !== "ready") capabilities = { ...none };

  const systemModesSupported = input.managementPackage === "ready"
    && !input.registryModeDetected
    && input.backend !== "foreign"
    && (input.installation === "absent" || input.installation === "current" || input.installation === "outdated")
    && stable;
  const migrationAvailable = (input.legacy.presence === "owned" || input.legacy.presence === "compatible-unquoted")
    && !input.registryModeDetected
    && (input.legacy.state === "stopped"
      || (input.legacy.state === "running" && (input.legacy.trustedBinaryAvailable ?? false)));

  return { ...capabilities, systemModesSupported, migrationAvailable };
}
