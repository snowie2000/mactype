import { AlertTriangle, BadgeCheck, Check, FileInput, FileOutput, FolderOpen, Play, Save, SaveAll, SlidersHorizontal } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import type { LegacyProfileCandidate, PreviewRequest, PreviewResult } from "../app/model";
import { runtime } from "../app/runtimeAdapter";
import { ProfileNameDialog } from "../components/ProfileNameDialog";
import { useProfileDocument } from "../features/profiles/useProfileDocument";
import { fileName, managedProfileFor, matchesAppliedProfile, sameProfileIdentity } from "./profiles/profileEditorUtils";
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
  const [nameDialogOpen, setNameDialogOpen] = useState(false);
  const [detectedLegacy, setLegacy] = useState<LegacyProfileCandidate | null>(null);
  const [thumbnails, setThumbnails] = useState<ReadonlyMap<string, PreviewResult | null>>(() => new Map(thumbnailCache));
  const discoverLegacyProfile = useCallback(async () => {
    const detected = await runtime().discoverLegacyProfile();
    setLegacy(detected);
    return detected;
  }, []);
  const {
    appliedProfile,
    chooseProfile,
    command: busy,
    busy: documentBusy,
    designateProfile: designate,
    dirtyCount,
    error,
    loading,
    message,
    offerStart,
    profile,
    profiles,
    profileNameVerdict,
    recoveryRequired,
    replaceDocument,
    runFileOperation,
    saveCurrentProfile: save,
    saveProfileAs: duplicate,
    suggestedProfileName,
    startServiceNow,
  } = useProfileDocument(t, { page: "files", discoverLegacyProfile });
  const legacy = !loading && detectedLegacy && !managedProfileFor(detectedLegacy, profiles) && !sameProfileIdentity(detectedLegacy, appliedProfile)
    ? detectedLegacy
    : null;
  useEffect(() => {
    if (!loading && detectedLegacy && (managedProfileFor(detectedLegacy, profiles) || sameProfileIdentity(detectedLegacy, appliedProfile))) {
      setLegacy(null);
    }
  }, [appliedProfile, detectedLegacy, loading, profiles]);

  useEffect(() => {
    let active = true;
    void (async () => {
      for (const entry of profiles) {
        if (thumbnailCache.has(entry.path)) continue;
        let rendered: PreviewResult | null = null;
        try {
          rendered = await runtime().renderProfilePreview(thumbnailRequest(entry.path));
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

  const editInTuner = async (path: string) => {
    if (await chooseProfile(path)) onEditInTuner?.();
  };

  const importFrom = async (path: string) => {
    await replaceDocument("import", () => runtime().importProfile(path), (opened) => {
      setLegacy(null);
      return t("files.imported", { name: fileName(opened.path) });
    });
  };

  const chooseImport = async () => {
    let selected: string | null = null;
    const picked = await runFileOperation("pick-import", async () => {
      selected = await runtime().pickIniProfile(t("files.iniFilter"));
      return null;
    });
    if (picked && selected) await importFrom(selected);
  };

  const exportIni = async () => {
    if (!profile) return;
    await runFileOperation("export", async () => {
      const selected = await runtime().pickIniExportPath(t("files.iniFilter"), fileName(profile.path));
      if (!selected) return null;
      const destination = await runtime().exportProfile(selected);
      return t("files.exported", { name: fileName(destination) });
    });
  };

  const revealCurrentProfile = async () => {
    await runFileOperation("reveal", async () => {
      const path = await runtime().revealProfileFile();
      return t("files.revealed", { name: fileName(path) });
    });
  };

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
              <li className="profile-card" data-applied={applied} data-run-profile={applied} data-selected={selected} key={entry.path}>
                <button aria-pressed={selected} className="profile-card-select" disabled={busy !== null} onClick={() => void chooseProfile(entry.path)} type="button">
                  <span className="profile-card-thumb">
                    {thumbnail
                      ? <img alt={t("files.thumbnailAlt", { name: entry.name })} loading="lazy" src={runtime().previewImageUrl(thumbnail.imagePath)} />
                      : <span aria-hidden="true" className="profile-card-thumb-fallback">{THUMBNAIL_SAMPLE_TEXT}</span>}
                  </span>
                  <span className="profile-card-title">
                    <strong>{entry.name}</strong>
                    {applied && <span className="profile-card-badge">{t("files.runProfileBadge")}</span>}
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
          <button className="button secondary" disabled={!profile || !profile.canSave || dirtyCount === 0 || busy !== null} data-dirty={dirtyCount > 0} onClick={() => void save()} type="button"><Save aria-hidden="true" size={17} /> {busy === "save" ? t("profiles.saving") : t("profiles.save")}</button>
          <button className="button secondary" disabled={!profile || documentBusy || recoveryRequired} onClick={() => setNameDialogOpen(true)} type="button"><SaveAll aria-hidden="true" size={16} /> {t("files.saveAs")}</button>
          <button className="button secondary" disabled={!profile || busy !== null} onClick={() => void exportIni()} type="button"><FileOutput aria-hidden="true" size={17} /> {busy === "export" ? t("files.exporting") : t("files.chooseExport")}</button>
          <button className="button designate" disabled={!profile || dirtyCount > 0 || busy !== null} onClick={() => void designate()} title={dirtyCount > 0 ? t("profiles.saveBeforeDesignate") : undefined} type="button"><BadgeCheck aria-hidden="true" size={17} /> {busy === "designate" ? t("profiles.designating") : t("profiles.designate")}</button>
        </div>
      </section>

      {nameDialogOpen && <ProfileNameDialog busy={documentBusy} initialName={suggestedProfileName} onCancel={() => setNameDialogOpen(false)} onSubmit={(name) => void duplicate(name).then((saved) => {
        if (saved) setNameDialogOpen(false);
      })} verdict={profileNameVerdict} />}

      {message && (
        <p aria-live="polite" className="success-message" data-operation="file-settings">
          <Check aria-hidden="true" size={16} /> {message}
          {offerStart && <button className="text-action" disabled={busy !== null} onClick={() => void startServiceNow()} type="button"><Play aria-hidden="true" size={14} /> {busy === "start" ? t("execution.serviceWorking") : t("files.startServiceNow")}</button>}
        </p>
      )}
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
