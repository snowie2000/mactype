import type { LegacyProfileCandidate, ProfileEntry } from "../../app/model";

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function profileNameStem(path: string): string {
  return fileName(path).replace(/\.ini$/i, "");
}

/* The same set the Tauri side refuses when it names the copy; keeping it here
   lets the naming dialog explain the refusal before the round trip. */
export const reservedProfileNameCharacters = /[<>:"/\\|?*]/;

export function matchesAppliedProfile(entry: { path: string; displayPath: string }, appliedProfile: string | null): boolean {
  if (!appliedProfile) return false;
  const normalized = appliedProfile.toLocaleLowerCase();
  return entry.path.toLocaleLowerCase() === normalized || entry.displayPath.toLocaleLowerCase() === normalized;
}

export function sameProfileIdentity(candidate: LegacyProfileCandidate, activeProfile: string | null): boolean {
  if (!activeProfile) return false;
  const stem = (path: string) => fileName(path).replace(/\.ini$/i, "").toLocaleLowerCase();
  return candidate.name.toLocaleLowerCase() === stem(activeProfile) || stem(candidate.path) === stem(activeProfile);
}

export function managedProfileFor(candidate: LegacyProfileCandidate, profiles: ReadonlyArray<ProfileEntry>): ProfileEntry | null {
  const candidatePath = candidate.path.toLocaleLowerCase();
  return profiles.find((profile) => profile.path.toLocaleLowerCase() === candidatePath) ?? null;
}

export const splitSubstitution = (mapping: string) => {
  const separator = mapping.indexOf("=");
  return separator < 0
    ? { source: mapping, replacement: mapping }
    : {
        source: mapping.slice(0, separator).trim(),
        replacement: mapping.slice(separator + 1).trim(),
      };
};
