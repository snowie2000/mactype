import { settingsSchema } from "../../generated/settings";
import type {
  AdvancedProfile,
  IndividualSetting,
  ProfileEntry,
  ProfileSnapshot,
} from "../model";

export const fallbackGalleryProfilePath = "C:\\Program Files\\MacType\\ini\\Default.ini";

const fallbackGalleryProfile: ProfileSnapshot = {
  path: fallbackGalleryProfilePath,
  displayPath: "ini\\Default.ini",
  location: "installation",
  canSave: true,
  encoding: "utf-8",
  bom: "none",
  lineEnding: "cr-lf",
  originalHash: "browser-gallery",
  values: Object.fromEntries(settingsSchema.map((setting) => [setting.id, setting.default])),
  savedValues: Object.fromEntries(settingsSchema.map((setting) => [setting.id, setting.default])),
  dirtyKeys: [],
  canUndo: false,
  canRedo: false,
  individuals: [{ fontFace: "Segoe UI", values: [1, 2, null, null, null, 1] }],
  lists: {
    excludeFonts: ["Raster Fonts"],
    includeFonts: [],
    excludeModules: ["fontview.exe"],
    includeModules: [],
    unloadDlls: [],
    excludeSubstitutionModules: [],
  },
  advanced: {
    shadow: null,
    lcdFilterWeight: null,
    pixelLayout: null,
    fontSubstitutes: [],
  },
};

const recentGalleryProfile: ProfileSnapshot = {
  ...fallbackGalleryProfile,
  path: "C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\profiles\\Recent.ini",
  displayPath: "Profiles\\Recent.ini",
  location: "personal",
};

const cloneProfile = (profile: ProfileSnapshot): ProfileSnapshot => structuredClone(profile);

interface BrowserGalleryProfileState {
  current(): ProfileSnapshot;
  discard(): ProfileSnapshot;
  resetDefaults(): ProfileSnapshot;
  duplicate(name: string): ProfileSnapshot;
  import(path: string): ProfileSnapshot;
  list(): ReadonlyArray<ProfileEntry>;
  open(path: string): ProfileSnapshot;
  openDefault(): ProfileSnapshot;
  redo(): ProfileSnapshot;
  save(): ProfileSnapshot;
  setCanSave(canSave: boolean): ProfileSnapshot;
  undo(): ProfileSnapshot;
  updateAdvanced(advanced: AdvancedProfile): ProfileSnapshot;
  updateIndividuals(entries: ReadonlyArray<IndividualSetting>): ProfileSnapshot;
  updateList(kind: string, entries: ReadonlyArray<string>): ProfileSnapshot;
  updateSetting(settingId: string, value: number): ProfileSnapshot;
}

export function createBrowserGalleryProfileState(): BrowserGalleryProfileState {
  let profile = cloneProfile(fallbackGalleryProfile);
  let savedProfile = cloneProfile(fallbackGalleryProfile);
  const undoHistory: ProfileSnapshot[] = [];
  const redoHistory: ProfileSnapshot[] = [];

  function snapshot(): ProfileSnapshot {
    return { ...cloneProfile(profile), savedValues: structuredClone(savedProfile.values) };
  }

  function open(next: ProfileSnapshot): ProfileSnapshot {
    profile = { ...cloneProfile(next), canUndo: false, canRedo: false };
    savedProfile = cloneProfile(profile);
    undoHistory.length = 0;
    redoHistory.length = 0;
    return snapshot();
  }

  function edit(update: (current: ProfileSnapshot) => ProfileSnapshot, dirtyKey: string): ProfileSnapshot {
    undoHistory.push(cloneProfile(profile));
    redoHistory.length = 0;
    const next = update(cloneProfile(profile));
    profile = {
      ...next,
      dirtyKeys: [...new Set([...next.dirtyKeys, dirtyKey])],
      canUndo: true,
      canRedo: false,
    };
    return snapshot();
  }

  function moveHistory(from: ProfileSnapshot[], to: ProfileSnapshot[]): ProfileSnapshot {
    const destination = from.pop();
    if (!destination) return snapshot();
    to.push(cloneProfile(profile));
    profile = {
      ...cloneProfile(destination),
      canUndo: undoHistory.length > 0,
      canRedo: redoHistory.length > 0,
    };
    return snapshot();
  }

  function profileForPath(path: string): ProfileSnapshot {
    const fileName = path.split(/[\\/]/).pop() ?? path;
    const normalized = path.toLocaleLowerCase();
    if (normalized.includes("\\mactype\\controlcenter\\profiles\\")) {
      return { ...fallbackGalleryProfile, path, displayPath: `Profiles\\${fileName}`, location: "personal" };
    }
    if (normalized.includes("\\mactype\\ini\\")) {
      return { ...fallbackGalleryProfile, path, displayPath: `ini\\${fileName}`, location: "installation" };
    }
    return { ...fallbackGalleryProfile, path, displayPath: path, location: "external", canSave: false };
  }

  return {
    current: () => snapshot(),
    discard: () => open(savedProfile),
    resetDefaults: () => {
      const changed = settingsSchema
        .filter((setting) => profile.values[setting.id] !== setting.factory)
        .map((setting) => setting.id);
      if (changed.length === 0) return snapshot();
      undoHistory.push(cloneProfile(profile));
      redoHistory.length = 0;
      profile = {
        ...cloneProfile(profile),
        values: Object.fromEntries(settingsSchema.map((setting) => [setting.id, setting.factory])),
        dirtyKeys: [...new Set([...profile.dirtyKeys, ...changed])],
        canUndo: true,
        canRedo: false,
      };
      return snapshot();
    },
    duplicate: (name) => open({
      ...profile,
      path: `C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\profiles\\${name}.ini`,
      displayPath: `Profiles\\${name}.ini`,
      location: "personal",
      canSave: true,
      dirtyKeys: [],
    }),
    import: (path) => {
      const fileName = path.split(/[\\/]/).pop() ?? "Imported.ini";
      return open({
        ...fallbackGalleryProfile,
        path: `C:\\Users\\Gallery\\AppData\\Local\\MacType\\ControlCenter\\profiles\\${fileName}`,
        displayPath: `Profiles\\${fileName}`,
        location: "personal",
        canSave: true,
      });
    },
    list: () => [
      { name: "Default", path: fallbackGalleryProfile.path, displayPath: fallbackGalleryProfile.displayPath },
      { name: "Pretendard forever", path: "C:\\Program Files\\MacType\\ini\\pretendard forever.ini", displayPath: "ini\\pretendard forever.ini" },
      { name: "Recent", path: recentGalleryProfile.path, displayPath: recentGalleryProfile.displayPath },
    ],
    open: (path) => open(profileForPath(path)),
    openDefault: () => open(fallbackGalleryProfile),
    redo: () => moveHistory(redoHistory, undoHistory),
    save: () => {
      profile = { ...profile, dirtyKeys: [], canUndo: false, canRedo: false };
      savedProfile = cloneProfile(profile);
      undoHistory.length = 0;
      redoHistory.length = 0;
      return snapshot();
    },
    setCanSave: (canSave) => {
      profile = { ...profile, canSave };
      savedProfile = { ...savedProfile, canSave };
      return snapshot();
    },
    undo: () => moveHistory(undoHistory, redoHistory),
    updateAdvanced: (advanced) => edit(
      (current) => ({ ...current, advanced: structuredClone(advanced) }),
      "advanced",
    ),
    updateIndividuals: (entries) => edit(
      (current) => ({ ...current, individuals: structuredClone(entries) }),
      "section:Individual",
    ),
    updateList: (kind, entries) => edit(
      (current) => ({ ...current, lists: { ...current.lists, [kind]: [...entries] } }),
      `section:${kind}`,
    ),
    updateSetting: (settingId, value) => edit(
      (current) => ({ ...current, values: { ...current.values, [settingId]: value } }),
      settingId,
    ),
  };
}
