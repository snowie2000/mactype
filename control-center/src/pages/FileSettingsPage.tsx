import { AlertTriangle, Check, FileInput, FileOutput, FolderOpen, Play, Save, SaveAll, SlidersHorizontal } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { LegacyProfileCandidate, PreviewRequest, PreviewResult, ProfileEntry, ProfileSnapshot } from "../app/model";
import { operationErrorMessage } from "../app/operationError";
import {
  applyOpenProfile,
  currentProfile,
  discoverLegacyProfile,
  duplicateProfile,
  exportProfile,
  importProfile,
  listProfiles,
  loadExecutionStatus,
  openProfile,
  pickIniProfile,
  pickIniExportPath,
  previewImageUrl,
  renderProfilePreview,
  revealProfileFile,
  saveProfile,
} from "../app/tauri";
import { openPreferredProfile, rememberProfile } from "../app/profilePreference";
import { useI18n } from "../i18n/i18n";

const THUMBNAIL_SAMPLE_TEXT = "The quick brown fox jumps over the lazy dog 0123456789";
const THUMBNAIL_WIDTH = 640;
const THUMBNAIL_HEIGHT = 140;
const thumbnailCache = new Map<string, PreviewResult | null>();

interface FileSettingsPageProps {
  onEditInTuner?: () => void;
}

export function FileSettingsPage({ onEditInTuner }: FileSettingsPageProps) {
  const { t } = useI18n();
  const [profile, setProfile] = useState<ProfileSnapshot | null>(null);
  const [profiles, setProfiles] = useState<ReadonlyArray<ProfileEntry>>([]);
  const [appliedProfile, setAppliedProfile] = useState<string | null>(null);
  const [legacy, setLegacy] = useState<LegacyProfileCandidate | null>(null);
  const [thumbnails, setThumbnails] = useState<ReadonlyMap<string, PreviewResult | null>>(() => new Map(thumbnailCache));
  const [copyName, setCopyName] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshProfiles = useCallback(async () => {
    setProfiles(await listProfiles());
  }, []);

  useEffect(() => {
    let active = true;
    void Promise.all([currentProfile(), listProfiles(), discoverLegacyProfile(), loadExecutionStatus()])
      .then(async ([opened, available, detected, execution]) => {
        const managedDetected = detected ? managedProfileFor(detected, available) : null;
        const preferredProfile = execution.injectionReady
          ? execution.activeProfile
          : managedDetected?.displayPath ?? execution.activeProfile;
        const selected = await openPreferredProfile(
          opened,
          available,
          preferredProfile,
        );
        if (!active) return;
        setProfile(selected);
        setProfiles(available);
        setAppliedProfile(execution.activeProfile);
        setLegacy(detected && !managedDetected && !sameProfileIdentity(detected, execution.activeProfile) ? detected : null);
      })
      .catch((caught: unknown) => {
        if (active) setError(caught instanceof Error ? caught.message : String(caught));
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    void (async () => {
      for (const entry of profiles) {
        if (thumbnailCache.has(entry.path)) continue;
        let rendered: PreviewResult | null = null;
        try {
          rendered = await renderProfilePreview(thumbnailRequest(entry.path));
        } catch {
          rendered = null;
        }
        thumbnailCache.set(entry.path, rendered);
        if (!active) return;
        setThumbnails(new Map(thumbnailCache));
      }
    })();
    return () => {
      active = false;
    };
  }, [profiles]);

  const run = async (operation: string, action: () => Promise<ProfileSnapshot>, success: (opened: ProfileSnapshot) => string): Promise<boolean> => {
    setBusy(operation);
    try {
      const opened = await action();
      rememberProfile(opened.path);
      setProfile(opened);
      await refreshProfiles();
      setMessage(success(opened));
      setError(null);
      return true;
    } catch (caught: unknown) {
      setError(operationErrorMessage(caught, t));
      setMessage(null);
      return false;
    } finally {
      setBusy(null);
    }
  };

  const chooseProfile = async (path: string): Promise<boolean> => {
    if (profile?.path === path) return true;
    return run("open", () => openProfile(path), (opened) => t("files.opened", { name: fileName(opened.path) }));
  };

  const editInTuner = async (path: string) => {
    if (await chooseProfile(path)) onEditInTuner?.();
  };

  const duplicate = async () => {
    const name = copyName.trim();
    if (!name) return;
    await run("duplicate", () => duplicateProfile(name), (opened) => {
      setCopyName("");
      return t("files.duplicated", { name: fileName(opened.path) });
    });
  };

  const save = async () => {
    await run("save", async () => {
      const saved = await saveProfile();
      if (!saved) throw new Error(t("profiles.none"));
      return saved;
    }, (opened) => t("files.saved", { name: fileName(opened.path) }));
  };

  const apply = async () => {
    setBusy("apply");
    try {
      const applied = await applyOpenProfile();
      setAppliedProfile(applied.sourceProfile);
      setLegacy((detected) => detected && sameProfileIdentity(detected, applied.sourceProfile) ? null : detected);
      setMessage(t("files.applied", { name: fileName(applied.sourceProfile) }));
      setError(null);
    } catch (caught: unknown) {
      setError(operationErrorMessage(caught, t));
      setMessage(null);
    } finally {
      setBusy(null);
    }
  };

  const importFrom = async (path: string) => {
    await run("import", () => importProfile(path), (opened) => {
      setLegacy(null);
      return t("files.imported", { name: fileName(opened.path) });
    });
  };

  const chooseImport = async () => {
    try {
      const selected = await pickIniProfile(t("files.iniFilter"));
      if (selected) await importFrom(selected);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  };

  const exportIni = async () => {
    if (!profile) return;
    setBusy("export");
    try {
      const defaultName = fileName(profile.path);
      const selected = await pickIniExportPath(t("files.iniFilter"), defaultName);
      if (selected) {
        const destination = await exportProfile(selected);
        setMessage(t("files.exported", { name: fileName(destination) }));
        setError(null);
      }
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setMessage(null);
    } finally {
      setBusy(null);
    }
  };

  const revealCurrentProfile = async () => {
    setBusy("reveal");
    try {
      const path = await revealProfileFile();
      setMessage(t("files.revealed", { name: fileName(path) }));
      setError(null);
    } catch (caught: unknown) {
      setError(caught instanceof Error ? caught.message : String(caught));
      setMessage(null);
    } finally {
      setBusy(null);
    }
  };

  const dirtyCount = profile?.dirtyKeys.length ?? 0;
  const detailsSummary = profile
    ? `${t("files.fileDetails")} · ${profile.encoding.toUpperCase()} · ${profile.lineEnding.replace(/-/g, "").toUpperCase()}${dirtyCount ? ` · ${t("files.unsaved")} ${t("files.unsavedCount", { count: dirtyCount })}` : ""}`
    : `${t("files.fileDetails")} · —`;

  return (
    <section className="page view-enter" aria-labelledby="files-title">
      <header className="page-header">
        <div><h1 id="files-title">{t("nav.profiles")}</h1><p>{t("files.subtitle")}</p></div>
        <div className="header-actions">
          <button className="button" disabled={busy !== null} onClick={() => void chooseImport()} type="button"><FileInput aria-hidden="true" size={17} /> {busy === "import" ? t("files.importing") : t("files.chooseImport")}</button>
        </div>
      </header>

      {legacy && (
        <section className="legacy-import-banner" aria-labelledby="legacy-import-title">
          <FileInput aria-hidden="true" size={22} />
          <div>
            <h2 id="legacy-import-title">{t("files.detectedTitle")}</h2>
            <p>{t("files.detectedDescription", { name: legacy.name })}</p>
            <code title={legacy.path}>{legacy.path}</code>
          </div>
          <button className="button primary" disabled={busy !== null} onClick={() => void importFrom(legacy.path)} type="button">
            {busy === "import" ? t("files.importing") : t("files.importDetected")}
          </button>
        </section>
      )}

      <section className="section-block" aria-labelledby="profile-select-title">
        <div className="section-heading"><div><h2 id="profile-select-title">{t("profiles.select")}</h2><p>{t("files.selectDescription")}</p></div></div>
        <ul className="profile-grid">
          {profiles.map((entry) => {
            const selected = profile?.path === entry.path;
            const applied = matchesAppliedProfile(entry, appliedProfile);
            const thumbnail = thumbnails.get(entry.path) ?? null;
            return (
              <li className="profile-card" data-applied={applied} data-selected={selected} key={entry.path}>
                <button aria-pressed={selected} className="profile-card-select" disabled={busy !== null} onClick={() => void chooseProfile(entry.path)} type="button">
                  <span className="profile-card-thumb">
                    {thumbnail
                      ? <img alt={t("files.thumbnailAlt", { name: entry.name })} loading="lazy" src={previewImageUrl(thumbnail.imagePath)} />
                      : <span aria-hidden="true" className="profile-card-thumb-fallback">{THUMBNAIL_SAMPLE_TEXT}</span>}
                  </span>
                  <span className="profile-card-title">
                    <strong>{entry.name}</strong>
                    {applied && <span className="profile-card-badge">{t("files.appliedBadge")}</span>}
                  </span>
                  <code title={entry.path}>{entry.displayPath}</code>
                </button>
                <div className="profile-card-actions">
                  <button className="text-action" disabled={busy !== null} onClick={() => void editInTuner(entry.path)} type="button"><SlidersHorizontal aria-hidden="true" size={14} /> {t("files.editInTuner")}</button>
                </div>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="section-block" aria-labelledby="current-file-title">
        <div className="section-heading"><div><h2 id="current-file-title">{t("files.currentTitle")}</h2><p>{t("files.currentDescription")}</p></div></div>
        <div className="selected-file-area">
          <div className="selected-file-summary" data-empty={!profile}>
            <FolderOpen aria-hidden="true" size={22} />
            <div><strong>{profile ? t("files.editing") : t("profiles.none")}</strong>{profile && <div className="selected-file-path"><code title={profile.path}>{profile.displayPath}</code><button aria-label={t("files.reveal")} className="icon-button" disabled={busy !== null} onClick={() => void revealCurrentProfile()} title={t("files.reveal")} type="button"><FolderOpen aria-hidden="true" size={15} /></button></div>}</div>
          </div>
        </div>
        {profile && !profile.canSave && <p className="file-save-warning">{t("files.readOnly")}</p>}
        <details className="file-details">
          <summary>{detailsSummary}</summary>
          <dl className="detail-list compact-details">
            <div><dt>{t("files.encoding")}</dt><dd>{profile?.encoding ?? "—"}</dd></div>
            <div><dt>{t("files.lineEnding")}</dt><dd>{profile?.lineEnding ?? "—"}</dd></div>
            <div><dt>{t("files.unsaved")}</dt><dd>{dirtyCount ? t("files.unsavedCount", { count: dirtyCount }) : t("files.noUnsaved")}</dd></div>
          </dl>
        </details>
        <div className="file-primary-actions">
          <button className="button secondary" disabled={!profile || !profile.canSave || dirtyCount === 0 || busy !== null} onClick={() => void save()} type="button"><Save aria-hidden="true" size={17} /> {busy === "save" ? t("profiles.saving") : t("profiles.save")}</button>
          <div className="file-save-as"><input aria-label={t("profiles.copyName")} disabled={!profile || busy !== null} onChange={(event) => setCopyName(event.target.value)} placeholder={t("files.saveAsName")} value={copyName} /><button className="button secondary" disabled={!profile || !copyName.trim() || busy !== null} onClick={() => void duplicate()} type="button"><SaveAll aria-hidden="true" size={16} /> {t("files.saveAs")}</button></div>
          <button className="button secondary" disabled={!profile || busy !== null} onClick={() => void exportIni()} type="button"><FileOutput aria-hidden="true" size={17} /> {busy === "export" ? t("files.exporting") : t("files.chooseExport")}</button>
          <button className="button primary" disabled={!profile || dirtyCount > 0 || busy !== null} onClick={() => void apply()} title={dirtyCount > 0 ? t("profiles.saveBeforeApply") : undefined} type="button"><Play aria-hidden="true" size={17} /> {busy === "apply" ? t("profiles.applying") : t("profiles.apply")}</button>
        </div>
      </section>

      {message && <p aria-live="polite" className="success-message" data-operation="file-settings"><Check aria-hidden="true" size={16} /> {message}</p>}
      {error && <p className="inline-error"><AlertTriangle aria-hidden="true" size={15} /> {error}</p>}
    </section>
  );
}

function thumbnailRequest(profilePath: string): PreviewRequest {
  const displayScale = window.devicePixelRatio || 1;
  return {
    profilePath,
    overrides: {},
    displayScale,
    sample: {
      text: THUMBNAIL_SAMPLE_TEXT,
      fontFace: "Segoe UI",
      fontSizePt: 12,
      widthPx: Math.round(THUMBNAIL_WIDTH * displayScale),
      heightPx: Math.round(THUMBNAIL_HEIGHT * displayScale),
      dpi: Math.round(96 * displayScale),
      foreground: "#181D23",
      background: "#EEF1F4",
    },
  };
}

function matchesAppliedProfile(entry: ProfileEntry, appliedProfile: string | null): boolean {
  if (!appliedProfile) return false;
  const normalized = appliedProfile.toLocaleLowerCase();
  return entry.path.toLocaleLowerCase() === normalized || entry.displayPath.toLocaleLowerCase() === normalized;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function sameProfileIdentity(candidate: LegacyProfileCandidate, activeProfile: string | null): boolean {
  if (!activeProfile) return false;
  const stem = (path: string) => fileName(path).replace(/\.ini$/i, "").toLocaleLowerCase();
  return candidate.name.toLocaleLowerCase() === stem(activeProfile) || stem(candidate.path) === stem(activeProfile);
}

function managedProfileFor(candidate: LegacyProfileCandidate, profiles: ReadonlyArray<ProfileEntry>): ProfileEntry | null {
  const candidatePath = candidate.path.toLocaleLowerCase();
  return profiles.find((profile) => profile.path.toLocaleLowerCase() === candidatePath) ?? null;
}
