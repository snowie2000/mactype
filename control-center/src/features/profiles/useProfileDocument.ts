import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AdvancedProfile, DesignationEffect, ExecutionStatus, IndividualSetting, LegacyProfileCandidate, ProfileEntry, ProfileSnapshot } from "../../app/model";
import { operationErrorMessage } from "../../app/operationError";
import { openPreferredProfile, rememberProfile } from "../../app/profilePreference";
import { runtime } from "../../app/runtimeAdapter";
import { settingsSchema } from "../../generated/settings";
import { fileName, managedProfileFor, matchesAppliedProfile, profileNameStem, reservedProfileNameCharacters } from "../../pages/profiles/profileEditorUtils";
import type { I18nValue } from "../../i18n/i18n";

type ProfileCommand = "undo" | "redo" | "discard" | "save" | "save-as" | "designate" | "apply" | "open" | "import" | "export" | "reveal" | "start";

/* What a completed save still owes the service. "designate" follows a save as,
   which leaves a profile the service has never seen; "apply-to-service"
   follows a save over the run profile, which the running service will not
   read on its own. */
export type ProfileFollowUp = "designate" | "apply-to-service";

/* Mirrors the managed-directory collision the Tauri side enforces, so the
   naming dialog can refuse a taken name without a round trip. */
export type ProfileNameVerdict = "ok" | "empty" | "taken" | "reserved-character";

interface ProfileDocumentOptions {
  page?: "files" | "profiles";
  discoverLegacyProfile?: () => Promise<LegacyProfileCandidate | null>;
}

const emptyAdvancedProfile: AdvancedProfile = {
  shadow: null,
  lcdFilterWeight: null,
  pixelLayout: null,
  fontSubstitutes: [],
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function cloneAdvancedProfile(advanced: AdvancedProfile): AdvancedProfile {
  return {
    ...advanced,
    shadow: advanced.shadow ? { ...advanced.shadow } : null,
    lcdFilterWeight: advanced.lcdFilterWeight ? [...advanced.lcdFilterWeight] : null,
    pixelLayout: advanced.pixelLayout ? [...advanced.pixelLayout] : null,
    fontSubstitutes: [...advanced.fontSubstitutes],
  };
}

function profileLists(profile: ProfileSnapshot): Record<string, ReadonlyArray<string>> {
  return {
    excludeFonts: [...profile.lists.excludeFonts],
    includeFonts: [...profile.lists.includeFonts],
    excludeModules: [...profile.lists.excludeModules],
    includeModules: [...profile.lists.includeModules],
    unloadDlls: [...profile.lists.unloadDlls],
    excludeSubstitutionModules: [...profile.lists.excludeSubstitutionModules],
  };
}

export function useProfileDocument(t: I18nValue["t"], { page = "profiles", discoverLegacyProfile }: ProfileDocumentOptions = {}) {
  const [profile, setProfile] = useState<ProfileSnapshot | null>(null);
  const [profiles, setProfiles] = useState<ReadonlyArray<ProfileEntry>>([]);
  const [appliedProfile, setAppliedProfile] = useState<string | null>(null);
  const [execution, setExecution] = useState<ExecutionStatus | null>(null);
  const [designationEffect, setDesignationEffect] = useState<DesignationEffect | null>(null);
  /* Which unfinished consequence the last successful save left behind. The
     service resolves its profile generation once at startup and accepts no
     reload control, so saving the run profile leaves the running service on
     the previous bytes until the user publishes them. */
  const [followUp, setFollowUp] = useState<ProfileFollowUp | null>(null);
  const [values, setValues] = useState<Record<string, number>>(
    Object.fromEntries(settingsSchema.map((setting) => [setting.id, setting.default])),
  );
  const [individuals, setIndividuals] = useState<IndividualSetting[]>([]);
  const [lists, setLists] = useState<Record<string, ReadonlyArray<string>>>({});
  const [advanced, setAdvanced] = useState<AdvancedProfile>(emptyAdvancedProfile);
  const [loading, setLoading] = useState(true);
  const [pendingEdits, setPendingEdits] = useState(0);
  const [command, setCommand] = useState<ProfileCommand | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recoveryRequired, setRecoveryRequired] = useState(false);
  const mutationQueue = useRef<Promise<void>>(Promise.resolve());

  const applySnapshot = useCallback((opened: ProfileSnapshot) => {
    rememberProfile(opened.path);
    setProfile(opened);
    setValues(opened.values);
    setIndividuals(opened.individuals.map((entry) => ({ ...entry, values: [...entry.values] })));
    setLists(profileLists(opened));
    setAdvanced(cloneAdvancedProfile(opened.advanced));
  }, []);

  const queueMutation = useCallback((mutation: () => Promise<ProfileSnapshot | null>) => {
    setPendingEdits((current) => current + 1);
    const operation = mutationQueue.current.then(mutation);
    mutationQueue.current = operation.then(() => undefined, () => undefined);
    void operation
      .then((snapshot) => {
        if (snapshot) setProfile(snapshot);
        setMessage(null);
        setFollowUp(null);
        setDesignationEffect(null);
      })
      .catch((caught: unknown) => {
        setRecoveryRequired(true);
        setError(errorMessage(caught));
      })
      .finally(() => setPendingEdits((current) => Math.max(0, current - 1)));
  }, []);

  useEffect(() => {
    let active = true;
    void Promise.all([runtime().currentProfile(), runtime().listProfiles(), runtime().loadExecutionStatus(), discoverLegacyProfile ? discoverLegacyProfile() : runtime().discoverLegacyProfile()])
      .then(async ([current, available, nextExecution, detected]) => {
        const managedDetected = detected ? managedProfileFor(detected, available) : null;
        const opened = await openPreferredProfile(current, available, nextExecution, managedDetected);
        if (!active) return;
        if (opened) applySnapshot(opened);
        setProfiles(available);
        setAppliedProfile(nextExecution.activeProfile);
        setExecution(nextExecution);
      })
      .catch((caught: unknown) => {
        if (active) setError(errorMessage(caught));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [applySnapshot, discoverLegacyProfile]);

  const previewSetting = (settingId: string, value: number) => {
    setValues((current) => ({ ...current, [settingId]: value }));
  };

  const changeSetting = (settingId: string, value: number) => {
    previewSetting(settingId, value);
    queueMutation(() => runtime().updateProfileSetting(settingId, value));
  };

  const commitIndividuals = (next: IndividualSetting[]) => {
    setIndividuals(next);
    queueMutation(() => runtime().updateProfileIndividuals(next));
  };

  const addIndividual = (font: string) => {
    const normalized = font.trim();
    if (!normalized || individuals.some((entry) => entry.fontFace.toLocaleLowerCase() === normalized.toLocaleLowerCase())) return;
    commitIndividuals([...individuals, { fontFace: normalized, values: [null, null, null, null, null, null] }]);
  };

  const updateList = (kind: string, entries: ReadonlyArray<string>) => {
    const normalized = entries.map((entry) => entry.trim()).filter(Boolean);
    setLists((current) => ({ ...current, [kind]: normalized }));
    queueMutation(() => runtime().updateProfileList(kind, normalized));
  };

  const commitAdvanced = (next: AdvancedProfile) => {
    setAdvanced(next);
    queueMutation(() => runtime().updateProfileAdvanced(next));
  };

  const resetDefaults = () => {
    setValues(Object.fromEntries(settingsSchema.map((setting) => [setting.id, setting.factory])));
    queueMutation(() => runtime().resetProfileDefaults());
  };

  const runHistoryCommand = async (nextCommand: "undo" | "redo" | "discard") => {
    const action = nextCommand === "undo" ? () => runtime().undoProfile() : nextCommand === "redo" ? () => runtime().redoProfile() : () => runtime().discardProfileChanges();
    setCommand(nextCommand);
    /* Undo, redo and discard move the document away from the save whose
       consequence the slot is describing, so they retire it as any other
       command does. No user can reach that today, because saving clears the
       document's history and leaves nothing dirty, so all three are disabled
       until an edit that already retires the slot. The clearing is here so the
       invariant does not depend on that distant detail staying true. */
    setDesignationEffect(null);
    setFollowUp(null);
    try {
      await mutationQueue.current;
      applySnapshot(await action());
      setRecoveryRequired(false);
      setMessage(null);
      setError(null);
    } catch (caught: unknown) {
      setError(errorMessage(caught));
    } finally {
      setCommand(null);
    }
  };

  const runCommand = async (
    nextCommand: ProfileCommand,
    action: () => Promise<string | null>,
    formatError: (caught: unknown) => string = errorMessage,
  ): Promise<boolean> => {
    if (pendingEdits > 0 || command !== null) return false;
    setCommand(nextCommand);
    setDesignationEffect(null);
    setFollowUp(null);
    try {
      await mutationQueue.current;
      const success = await action();
      if (success !== null) {
        setMessage(success);
        setError(null);
      }
      return true;
    } catch (caught: unknown) {
      setError(formatError(caught));
      setMessage(null);
      return false;
    } finally {
      setCommand(null);
    }
  };

  const replaceDocument = async (
    nextCommand: "open" | "import" | "save" | "save-as",
    action: () => Promise<ProfileSnapshot>,
    success: (opened: ProfileSnapshot) => string,
  ): Promise<boolean> => runCommand(nextCommand, async () => {
    const opened = await action();
    applySnapshot(opened);
    setProfiles(await runtime().listProfiles());
    return success(opened);
  }, page === "files" ? (caught) => operationErrorMessage(caught, t) : errorMessage);

  const runFileOperation = async (operation: "pick-import" | "export" | "reveal", action: () => Promise<string | null>) => {
    if (operation !== "pick-import") return runCommand(operation, action);
    try {
      await action();
      return true;
    } catch (caught: unknown) {
      setError(errorMessage(caught));
      return false;
    }
  };

  const chooseProfile = async (path: string): Promise<boolean> => {
    if (profile?.path === path) return true;
    if (recoveryRequired) return false;
    return replaceDocument("open", () => runtime().openProfile(path), (opened) => t("files.opened", { name: fileName(opened.path) }));
  };

  /* One command, two framings. "designate" makes this profile the run profile;
     "apply-to-service" republishes the run profile the user has just edited. */
  const designateOpenProfile = async (intent: ProfileFollowUp) => {
    const designated = await runtime().designateOpenProfile();
    setAppliedProfile(designated.sourceProfile);
    setDesignationEffect(designated.effect);
    setExecution(await runtime().loadExecutionStatus());
    const live = designated.effect === "live";
    const key = intent === "apply-to-service"
      ? (live ? "profiles.appliedLive" : "profiles.appliedNextStart")
      : (live ? "profiles.designatedLive" : "profiles.designatedNextStart");
    return t(key, { name: fileName(designated.sourceProfile) });
  };

  const designateProfile = async (intent: ProfileFollowUp = "designate") => {
    if (!profile || recoveryRequired || dirtyKeys.length > 0) return;
    await runCommand("designate", () => designateOpenProfile(intent), (caught) => operationErrorMessage(caught, t));
  };

  const saveProfileAs = async (requestedName: string) => {
    const name = requestedName.trim();
    if (!profile || recoveryRequired || !name) return false;
    const saved = await replaceDocument("save-as", () => runtime().duplicateProfile(name), (opened) => page === "files"
      ? t("files.duplicated", { name: fileName(opened.path) })
      : t("profiles.savedAs", { path: opened.displayPath }));
    if (saved) setFollowUp("designate");
    return saved;
  };

  const saveCurrentProfile = async () => {
    if (!profile?.canSave || recoveryRequired || dirtyKeys.length === 0) return;
    const written = { path: profile.path, displayPath: profile.displayPath };
    const saved = await replaceDocument("save", async () => {
      const next = await runtime().saveProfile();
      if (!next) throw new Error(t("profiles.none"));
      return next;
    }, (opened) => t(page === "files" ? "files.saved" : "profiles.savedNow", { name: fileName(opened.path) }));
    if (saved && matchesAppliedProfile(written, appliedProfile)) setFollowUp("apply-to-service");
  };

  const serviceCanStart = Boolean(
    execution?.systemService.installation === "current"
      && execution.systemService.runtime === "stopped"
      && execution.systemService.canStart,
  );
  const offerStart = designationEffect === "next-start" && serviceCanStart;

  // Designation never starts the service; only this separate user action may do so.
  const startServiceNow = async () => {
    if (!offerStart) return;
    await runCommand("start", async () => {
      const next = await runtime().manageSystemService("start");
      setExecution(next);
      setAppliedProfile(next.activeProfile);
      return t("files.serviceStartedWithRunProfile", { name: next.activeProfile ? fileName(next.activeProfile) : "" });
    }, (caught) => operationErrorMessage(caught, t, "execution.operationFailed"));
  };

  /* The Tauri side writes every copy into the managed profile directory, so a
     name collides only with the profiles already shown as Profiles\<name>.ini. */
  const managedProfileNames = useMemo(
    () => new Set(
      profiles
        .filter((entry) => entry.displayPath.toLocaleLowerCase().startsWith("profiles\\"))
        .map((entry) => profileNameStem(entry.path).toLocaleLowerCase()),
    ),
    [profiles],
  );

  const profileNameVerdict = useCallback((candidate: string): ProfileNameVerdict => {
    const name = candidate.trim();
    if (!name) return "empty";
    if (reservedProfileNameCharacters.test(name)) return "reserved-character";
    return managedProfileNames.has(name.toLocaleLowerCase()) ? "taken" : "ok";
  }, [managedProfileNames]);

  /* Save As opens on the current name, the way Windows does, but advanced past
     any collision so the dialog never opens already refusing itself. */
  const suggestedProfileName = useMemo(() => {
    if (!profile) return "";
    const stem = profileNameStem(profile.path);
    if (!managedProfileNames.has(stem.toLocaleLowerCase())) return stem;
    for (let index = 2; index < 1000; index += 1) {
      const candidate = `${stem} (${index})`;
      if (!managedProfileNames.has(candidate.toLocaleLowerCase())) return candidate;
    }
    return stem;
  }, [managedProfileNames, profile]);

  const dirtyKeys = useMemo(() => {
    const keys = new Set(profile?.dirtyKeys ?? []);
    if (profile) {
      for (const setting of settingsSchema) {
        if (values[setting.id] !== profile.values[setting.id]) keys.add(setting.id);
      }
    }
    return [...keys];
  }, [profile, values]);
  const busy = pendingEdits > 0 || command !== null;
  const dirtyCount = dirtyKeys.length;

  /* Apply is the editor's single route to the service: it saves unsaved edits
     and designates the open profile, which republishes it when it already is
     the run profile. It stays enabled while the service is behind this
     document: unsaved edits, another run profile, a save the service has not
     received, or a running service on bytes other than the run profile's. */
  const isRunProfile = profile !== null && matchesAppliedProfile(profile, appliedProfile);
  const serviceBehind = followUp !== null
    || execution?.runProfilePublication !== "published"
    || Boolean(execution.expectedProfileDigest
      && execution.systemService.activeProfileDigest
      && execution.systemService.activeProfileDigest !== execution.expectedProfileDigest);
  const applyPending = dirtyCount > 0 || !isRunProfile || serviceBehind;
  const applyBlockedByReadOnly = dirtyCount > 0 && profile?.canSave === false;
  const canApply = profile !== null && !busy && !recoveryRequired && !applyBlockedByReadOnly && applyPending;
  const applyDescription = applyBlockedByReadOnly
    ? t("profiles.readOnly")
    : !applyPending
      ? t("profiles.applyUpToDate")
      : t(isRunProfile ? "profiles.applyDescription" : "profiles.applyAsRunProfileDescription");

  /* A save that lands before the designation fails leaves its follow-up
     recorded, so the same button retries only the designation. */
  const applyProfile = async () => {
    if (!canApply) return;
    const intent: ProfileFollowUp = isRunProfile ? "apply-to-service" : "designate";
    const unsaved = dirtyCount > 0;
    await runCommand("apply", async () => {
      if (unsaved) {
        const saved = await runtime().saveProfile();
        if (!saved) throw new Error(t("profiles.none"));
        applySnapshot(saved);
        setProfiles(await runtime().listProfiles());
        setFollowUp(intent);
      }
      const message = await designateOpenProfile(intent);
      setFollowUp(null);
      return message;
    }, (caught) => operationErrorMessage(caught, t));
  };

  return {
    appliedProfile,
    applyDescription,
    applyPending,
    applyProfile,
    canApply,
    chooseProfile,
    designationEffect,
    offerStart,
    profiles,
    profileNameVerdict,
    replaceDocument,
    runFileOperation,
    startServiceNow,
    suggestedProfileName,
    addIndividual,
    advanced,
    designateProfile,
    busy,
    changeSetting,
    command,
    commitAdvanced,
    commitIndividuals,
    dirtyCount,
    dirtyKeys,
    discard: () => runHistoryCommand("discard"),
    error,
    individuals,
    lists,
    loading,
    message,
    previewSetting,
    profile,
    recoveryRequired,
    redo: () => runHistoryCommand("redo"),
    resetDefaults,
    savedValues: profile?.savedValues,
    saveCurrentProfile,
    saveProfileAs,
    setAdvanced,
    undo: () => runHistoryCommand("undo"),
    updateList,
    values,
  };
}
