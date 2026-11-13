import type { ExecutionStatus, ProfileEntry, ProfileSnapshot } from "./model";
import { runtime } from "./runtimeAdapter";
import { readPreference, writePreference } from "./persistedPreference";

export const recentProfileStorageKey = "mactype-control-center.recent-profile";

export function rememberProfile(path: string) {
  writePreference(recentProfileStorageKey, path);
}

function rememberedProfile(): string | null {
  return readPreference<string | null>({
    key: recentProfileStorageKey,
    parse: (raw) => raw,
    fallback: () => null,
  });
}

function availablePath(profiles: ReadonlyArray<ProfileEntry>, candidate: string | null): string | null {
  if (!candidate) return null;
  const normalized = candidate.toLocaleLowerCase();
  return profiles.find((profile) =>
    profile.path.toLocaleLowerCase() === normalized
    || profile.displayPath.toLocaleLowerCase() === normalized
  )?.path ?? null;
}

export async function openPreferredProfile(
  opened: ProfileSnapshot | null,
  profiles: ReadonlyArray<ProfileEntry>,
  execution: Pick<ExecutionStatus, "activeProfile" | "injectionReady">,
  managedLegacyProfile: ProfileEntry | null = null,
): Promise<ProfileSnapshot | null> {
  if (opened) {
    rememberProfile(opened.path);
    return opened;
  }

  const appliedProfile = execution.injectionReady
    ? execution.activeProfile
    : managedLegacyProfile?.displayPath ?? execution.activeProfile;
  const preferred = availablePath(profiles, rememberedProfile()) ?? availablePath(profiles, appliedProfile);
  if (preferred) {
    try {
      const selected = await runtime().openProfile(preferred);
      rememberProfile(selected.path);
      return selected;
    } catch {
      // A file can disappear after enumeration; the default remains a safe fallback.
    }
  }

  const fallback = await runtime().openDefaultProfile();
  if (fallback) rememberProfile(fallback.path);
  return fallback;
}
