import type { ProfileEntry, ProfileSnapshot } from "./model";
import { openDefaultProfile, openProfile } from "./tauri";

export const recentProfileStorageKey = "mactype-control-center.recent-profile";

export function rememberProfile(path: string) {
  try {
    window.localStorage.setItem(recentProfileStorageKey, path);
  } catch {
    // The profile remains usable when storage is unavailable.
  }
}

function rememberedProfile(): string | null {
  try {
    return window.localStorage.getItem(recentProfileStorageKey);
  } catch {
    return null;
  }
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
  appliedProfile: string | null,
): Promise<ProfileSnapshot | null> {
  if (opened) {
    rememberProfile(opened.path);
    return opened;
  }

  const preferred = availablePath(profiles, rememberedProfile()) ?? availablePath(profiles, appliedProfile);
  if (preferred) {
    try {
      const selected = await openProfile(preferred);
      rememberProfile(selected.path);
      return selected;
    } catch {
      // A file can disappear after enumeration; the default remains a safe fallback.
    }
  }

  const fallback = await openDefaultProfile();
  if (fallback) rememberProfile(fallback.path);
  return fallback;
}
