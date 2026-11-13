import { ListRestart, Play, Redo2, RotateCcw, Save, SaveAll, Search, Undo2, Upload } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { ProfileNameDialog } from "../components/ProfileNameDialog";
import { Hint } from "../components/Hint";
import { settingsSchema } from "../generated/settings";
import { settingMessageKey, useI18n } from "../i18n/i18n";
import { runtime } from "../app/runtimeAdapter";
import { AdvancedSettings } from "./profiles/AdvancedSettings";
import { IndividualSettings } from "./profiles/IndividualSettings";
import { ListsEditor } from "./profiles/ListsEditor";
import { BasicSettings, LcdSettings, SearchSettings, ShapeSettings } from "./profiles/SchemaSettings";
import { splitSubstitution } from "./profiles/profileEditorUtils";
import { ProfilePreviewPanel, type PreviewVariant, type ProfilePreviewHandle } from "./profiles/ProfilePreviewPanel";
import { useProfileDocument } from "../features/profiles/useProfileDocument";
import { useStepHistory } from "./profiles/useStepHistory";
import { GuidedSettings } from "./profiles/GuidedSettings";
import { stepSupportsHistory, guidedStepIds, type GuidedStepId } from "./profiles/guidedModel";

type GroupId = "basic" | "shape" | "lcd" | "advanced" | "individual" | "lists";
type ProfileMode = "guided" | "all";

/* The guided step is a short column of choices and trades width for height
   readily, so it docks the preview early. The settings table needs room for a
   label beside its control column, so it docks later. Below the threshold the
   preview falls back to a capped bottom panel.

   The all-settings figure is what the shipped default window reaches: the workspace
   is the window less the navigation rail, the page padding and the section
   index, so docking by default costs roughly 1300 logical pixels of window.
   Raising it further would push the default past a 1366-wide laptop, and the
   preview only reads as a right column if it starts as one. */
const DOCKED_PREVIEW_MIN_WIDTH: Readonly<Record<ProfileMode, number>> = { guided: 780, all: 840 };

interface ProfilesPageProps {
  ciSmoke?: boolean;
  mode?: ProfileMode;
  onPreviewReady?: () => void;
}

export function ProfilesPage({ ciSmoke = false, mode = "all", onPreviewReady }: ProfilesPageProps) {
  const { locale, t } = useI18n();
  const groups = useMemo<ReadonlyArray<{ id: GroupId; label: string; description: string }>>(() => [
    { id: "basic", label: t("group.basic.label"), description: t("group.basic.description") },
    { id: "shape", label: t("group.shape.label"), description: t("group.shape.description") },
    { id: "lcd", label: t("group.lcd.label"), description: t("group.lcd.description") },
    { id: "advanced", label: t("group.advanced.label"), description: t("group.advanced.description") },
    { id: "individual", label: t("group.individual.label"), description: t("group.individual.description") },
    { id: "lists", label: t("group.lists.label"), description: t("group.lists.description") },
  ], [t]);
  const individualLabels = useMemo(() => [
    t("individual.hinting"), t("individual.aa"), t("individual.normalWeight"),
    t("individual.boldWeight"), t("individual.slant"), t("individual.kerning"),
  ], [t]);
  const listDefinitions = useMemo(() => [
    { kind: "excludeFonts", label: t("list.excludeFonts.label"), help: t("list.excludeFonts.help") },
    { kind: "includeFonts", label: t("list.includeFonts.label"), help: t("list.includeFonts.help") },
    { kind: "excludeModules", label: t("list.excludeModules.label"), help: t("list.excludeModules.help") },
    { kind: "includeModules", label: t("list.includeModules.label"), help: t("list.includeModules.help") },
    { kind: "unloadDlls", label: t("list.unloadDlls.label"), help: t("list.unloadDlls.help") },
    { kind: "excludeSubstitutionModules", label: t("list.excludeSubstitutionModules.label"), help: t("list.excludeSubstitutionModules.help") },
  ] as const, [t]);
  const {
    addIndividual,
    advanced,
    applyDescription,
    applyPending,
    applyProfile,
    busy,
    canApply,
    changeSetting,
    command: profileCommand,
    commitAdvanced,
    commitIndividuals,
    dirtyCount,
    dirtyKeys,
    discard,
    error: documentError,
    individuals,
    offerStart,
    startServiceNow,
    lists,
    loading,
    message: profileMessage,
    previewSetting,
    profile,
    profileNameVerdict,
    recoveryRequired,
    redo,
    resetDefaults,
    savedValues,
    saveCurrentProfile,
    saveProfileAs,
    setAdvanced,
    suggestedProfileName,
    undo,
    updateList,
    values,
  } = useProfileDocument(t);
  const [renderError, setPreviewError] = useState<string | null>(null);
  const previewError = documentError ?? renderError;
  const [activeGroup, setActiveGroup] = useState<GroupId>("basic");
  const [activeGuidedStep, setActiveGuidedStep] = useState<GuidedStepId>("start");
  /* Step-scoped guided history. All-settings mode can rewrite the document
     through the global backend history, so the per-step record resets when
     the mode or the open document changes. */
  const stepHistory = useStepHistory(`${mode}::${profile?.path ?? ""}`);
  const [installedFonts, setInstalledFonts] = useState<ReadonlyArray<string>>([]);
  const [fontFace, setFontFace] = useState("Segoe UI");
  const [query, setQuery] = useState("");
  const [nameDialogOpen, setNameDialogOpen] = useState(false);
  const [previewDocked, setPreviewDocked] = useState(false);
  const previewPanelRef = useRef<ProfilePreviewHandle>(null);
  const workspaceRef = useRef<HTMLDivElement>(null);

  const dockedMinimumWidth = DOCKED_PREVIEW_MIN_WIDTH[mode];
  useEffect(() => {
    const workspace = workspaceRef.current;
    if (!workspace) return undefined;
    setPreviewDocked(workspace.clientWidth >= dockedMinimumWidth);
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) setPreviewDocked(entry.contentRect.width >= dockedMinimumWidth);
    });
    observer.observe(workspace);
    return () => observer.disconnect();
  }, [dockedMinimumWidth]);

  const guidedBusy = !profile || busy || recoveryRequired;
  const changeGuidedSetting = (settingId: string, value: number) => {
    if (stepSupportsHistory(activeGuidedStep)) {
      stepHistory.record(activeGuidedStep, settingId, value, profile?.values[settingId] ?? value);
    }
    changeSetting(settingId, value);
  };
  const undoStepEdit = () => {
    const entry = stepHistory.undo(activeGuidedStep);
    if (entry) changeSetting(entry.settingId, entry.before);
  };
  const redoStepEdit = () => {
    const entry = stepHistory.redo(activeGuidedStep);
    if (entry) changeSetting(entry.settingId, entry.after);
  };

  /* Ctrl+Z / Ctrl+Y inside guided mode drive the step-scoped history. Text
     fields keep their native editing shortcuts, and the default is only
     prevented when this step actually has something to undo or redo. */
  useEffect(() => {
    if (mode !== "guided") return undefined;
    const listener = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.metaKey) return;
      const target = event.target;
      if (target instanceof HTMLTextAreaElement) return;
      if (target instanceof HTMLInputElement && target.type !== "range" && target.type !== "checkbox" && target.type !== "radio") return;
      const key = event.key.toLocaleLowerCase();
      const wantsUndo = key === "z" && !event.shiftKey;
      const wantsRedo = key === "y" || (key === "z" && event.shiftKey);
      if ((!wantsUndo && !wantsRedo) || guidedBusy) return;
      if (wantsUndo && stepHistory.canUndo(activeGuidedStep)) {
        event.preventDefault();
        undoStepEdit();
      } else if (wantsRedo && stepHistory.canRedo(activeGuidedStep)) {
        event.preventDefault();
        redoStepEdit();
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  });

  const fontFamilies = useMemo(() => {
    const referenced = [
      fontFace,
      ...individuals.map((entry) => entry.fontFace),
      ...advanced.fontSubstitutes.flatMap((mapping) => {
        const pair = splitSubstitution(mapping);
        return [pair.source, pair.replacement];
      }),
      ...(lists.excludeFonts ?? []),
      ...(lists.includeFonts ?? []),
    ].map((font) => font.trim()).filter(Boolean);
    const collator = new Intl.Collator(locale, { sensitivity: "base", numeric: true });
    return [...new Set([...installedFonts, ...referenced])]
      .sort((left, right) => collator.compare(left, right));
  }, [advanced.fontSubstitutes, fontFace, individuals, installedFonts, lists.excludeFonts, lists.includeFonts, locale]);
  const installedFontKeys = useMemo(() => new Set(installedFonts.map((font) => font.toLocaleLowerCase())), [installedFonts]);
  const fontOptionLabel = (font: string) => installedFontKeys.has(font.toLocaleLowerCase())
    ? font
    : `${font} · ${t("profiles.fontUnavailable")}`;

  useEffect(() => {
    let active = true;
    void runtime().loadInstalledFontFamilies()
      .then((families) => {
        if (!active) return;
        setInstalledFonts(families);
        setFontFace((current) => families.some((font) => font.toLocaleLowerCase() === current.toLocaleLowerCase()) ? current : families[0] ?? current);
      })
      .catch((error: unknown) => {
        if (active) setPreviewError(error instanceof Error ? error.message : String(error));
      });
    return () => {
      active = false;
    };
  }, [setPreviewError]);

  const filteredSettings = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return settingsSchema.filter((setting) => {
      if (!needle && setting.group !== activeGroup) return false;
      const localized = `${t(settingMessageKey(setting.id, "label"))} ${t(settingMessageKey(setting.id, "description"))} ${setting.key}`;
      return !needle || localized.toLocaleLowerCase().includes(needle);
    });
  }, [activeGroup, query, t]);

  const showPreview = () => {
    previewPanelRef.current?.show();
  };

  /* Step-aware preview stacks, mirroring the legacy Tuner screens: the bold
     and italic screen compares the three styles and the LCD screen compares
     the current method against the red, green, and blue channels
     (channel-pure foregrounds isolate each subpixel). Every other screen
     renders the sample once, because a second sample group would claim the
     height the step body needs. */
  const previewVariants = useMemo<ReadonlyArray<PreviewVariant>>(() => {
    const pangram = t("profiles.samplePangram");
    if (mode === "guided" && activeGuidedStep === "boldItalic") {
      return [
        { key: "bold", label: t("guided.previewBold"), text: pangram, bold: true },
        { key: "italic", label: t("guided.previewItalic"), text: pangram, italic: true },
        { key: "bold-italic", label: t("guided.previewBoldItalic"), text: pangram, bold: true, italic: true },
      ];
    }
    if (mode === "guided" && activeGuidedStep === "lcd") {
      return [
        { key: "current", label: t("guided.previewCurrent"), text: pangram },
        { key: "channel-r", label: "R", text: pangram, foreground: "#C80000" },
        { key: "channel-g", label: "G", text: pangram, foreground: "#008A00" },
        { key: "channel-b", label: "B", text: pangram, foreground: "#0000C8" },
      ];
    }
    return [{ key: "normal", label: null }];
  }, [activeGuidedStep, mode, t]);

  const activeDefinition = groups.find((group) => group.id === activeGroup) ?? groups[0];
  const activeGuidedLabel = t(`guided.${activeGuidedStep}`);
  return (
    <section className="page profile-page view-enter" aria-labelledby="profiles-title" data-mode={mode}>
      <header className="page-header compact profile-header">
        <div>
          <div className="profile-mode-title"><h1 id="profiles-title"><Hint content={t(mode === "guided" ? "profiles.quickDescription" : "profiles.advancedDescription")}>{t(mode === "guided" ? "nav.guidedSetup" : "nav.allSettings")}</Hint></h1><span>Tuner</span></div>
          {loading
            ? <p>{t("profiles.searching")}</p>
            : <p className="profile-editing"><span>{t("profiles.editing")}</span> <code title={profile?.path}>{profile?.displayPath ?? t("profiles.none")}</code><span> · {t("profiles.unsavedSummary", { count: dirtyCount })}</span></p>}
          {profile && !profile.canSave && <p className="profile-save-warning">{t("profiles.readOnly")}</p>}
          {profileMessage && <p aria-live="polite" className="profile-message">{profileMessage}</p>}
        </div>
        {mode === "all" && <div aria-label={t("profiles.editActions")} className="profile-history-actions" role="toolbar">
          <button className="button secondary compact-action" disabled={!profile?.canUndo || busy} onClick={() => void undo()} type="button"><Undo2 aria-hidden="true" size={14} /> {t("profiles.undo")}</button>
          <button className="button secondary compact-action" disabled={!profile?.canRedo || busy} onClick={() => void redo()} type="button"><Redo2 aria-hidden="true" size={14} /> {t("profiles.redo")}</button>
          <button className="button secondary compact-action" disabled={!profile || dirtyCount === 0 || busy} onClick={() => void discard()} title={t("profiles.discardDescription")} type="button"><RotateCcw aria-hidden="true" size={14} /> {t("profiles.discard")}</button>
          <button className="button secondary compact-action" disabled={!profile || busy || recoveryRequired} onClick={resetDefaults} title={t("profiles.resetDefaultsDescription")} type="button"><ListRestart aria-hidden="true" size={14} /> {t("profiles.resetDefaults")}</button>
          <button className="button secondary compact-action" disabled={!profile || !profile.canSave || dirtyCount === 0 || busy || recoveryRequired} data-dirty={dirtyCount > 0} onClick={() => void saveCurrentProfile()} type="button"><Save aria-hidden="true" size={14} /> {profileCommand === "save" ? t("profiles.saving") : t("profiles.save")}</button>
          <button className="button secondary compact-action" disabled={!profile || busy || recoveryRequired} onClick={() => setNameDialogOpen(true)} type="button"><SaveAll aria-hidden="true" size={14} /> {t("files.saveAs")}</button>
          {offerStart && !applyPending
            ? <button className="button designate compact-action" disabled={busy} onClick={() => void startServiceNow()} type="button"><Play aria-hidden="true" size={14} /> {profileCommand === "start" ? t("execution.serviceWorking") : t("files.startServiceNow")}</button>
            : <button className="button designate compact-action" disabled={!canApply} onClick={() => void applyProfile()} title={applyDescription} type="button"><Upload aria-hidden="true" size={14} /> {profileCommand === "apply" ? t("profiles.applyingToService") : t("profiles.apply")}</button>}
        </div>}
      </header>

      {nameDialogOpen && <ProfileNameDialog busy={busy} initialName={suggestedProfileName} onCancel={() => setNameDialogOpen(false)} onSubmit={(name) => void saveProfileAs(name).then((saved) => {
        if (saved) setNameDialogOpen(false);
      })} verdict={profileNameVerdict} />}

      <div className="profile-layout">
        <aside className="settings-index" aria-label={mode === "guided" ? t("guided.progress") : t("profiles.sections")}>
          {mode === "all" && <label className="search-field"><Search aria-hidden="true" size={16} /><span className="sr-only">{t("profiles.search")}</span><input onChange={(event) => setQuery(event.target.value)} placeholder={t("profiles.search")} type="search" value={query} /></label>}
          <ul>{mode === "guided" ? guidedStepIds.map((step, index) => <li key={step}><button data-selected={activeGuidedStep === step} onClick={() => setActiveGuidedStep(step)} type="button"><span className="settings-step" aria-hidden="true">{index + 1}</span><span>{t(`guided.${step}`)}</span></button></li>) : groups.map((group) => <li key={group.id}><button data-selected={!query && activeGroup === group.id} onClick={() => { setActiveGroup(group.id); setQuery(""); }} type="button"><span>{group.label}</span></button></li>)}</ul>
        </aside>

        <div className="settings-workspace" data-preview-docked={previewDocked} ref={workspaceRef}>
          <div className="settings-form">
            <div className="section-heading"><h2><Hint content={mode === "guided" ? t("guided.guidance") : query ? t("profiles.searchDescription", { query }) : activeDefinition.description}>{mode === "guided" ? activeGuidedLabel : query ? t("profiles.searchResults") : activeDefinition.label}</Hint></h2></div>

            {mode === "guided" && <GuidedSettings activeStep={activeGuidedStep} advanced={advanced} busy={guidedBusy} canRedoStep={stepHistory.canRedo(activeGuidedStep)} canSave={profile?.canSave ?? false} canUndoStep={stepHistory.canUndo(activeGuidedStep)} applyDescription={applyDescription} applying={profileCommand === "apply"} canApply={canApply} dirtyCount={dirtyCount} dirtyKeys={dirtyKeys} fontFace={fontFace} fontFamilies={fontFamilies} fontOptionLabel={fontOptionLabel} onAdvancedCommit={(next) => void commitAdvanced(next)} onApply={() => void applyProfile()} onFontFaceChange={setFontFace} onPreview={showPreview} onRedoStep={redoStepEdit} onSave={() => void saveCurrentProfile()} offerStart={offerStart && !applyPending} onSaveAs={() => setNameDialogOpen(true)} onStartService={() => void startServiceNow()} onSettingChange={changeGuidedSetting} onSettingPreview={previewSetting} onStepChange={setActiveGuidedStep} onUndoStep={undoStepEdit} profileName={profile?.displayPath ?? null} profilePath={profile?.path ?? null} savedValues={savedValues} settings={settingsSchema} starting={profileCommand === "start"} t={t} values={values} />}

            {mode === "all" && query && <SearchSettings dirtyKeys={dirtyKeys} onChange={changeSetting} onPreviewChange={previewSetting} savedValues={savedValues} settings={filteredSettings} t={t} values={values} />}
            {mode === "all" && !query && activeGroup === "basic" && <BasicSettings dirtyKeys={dirtyKeys} onChange={changeSetting} onPreviewChange={previewSetting} savedValues={savedValues} settings={filteredSettings} t={t} values={values} />}
            {mode === "all" && !query && activeGroup === "shape" && <ShapeSettings dirtyKeys={dirtyKeys} onChange={changeSetting} onPreviewChange={previewSetting} savedValues={savedValues} settings={filteredSettings} t={t} values={values} />}
            {mode === "all" && !query && activeGroup === "lcd" && <LcdSettings dirtyKeys={dirtyKeys} onChange={changeSetting} onPreviewChange={previewSetting} savedValues={savedValues} settings={filteredSettings} t={t} values={values} />}

            {mode === "all" && !query && activeGroup === "advanced" && (
              <AdvancedSettings
                advanced={advanced}
                dirtyKeys={dirtyKeys}
                fontFamilies={fontFamilies}
                fontOptionLabel={fontOptionLabel}
                onAdvancedChange={setAdvanced}
                onAdvancedCommit={(next) => void commitAdvanced(next)}
                onSettingChange={changeSetting}
                onSettingPreview={previewSetting}
                savedValues={savedValues}
                settings={filteredSettings}
                t={t}
                values={values}
              />
            )}

            {mode === "all" && !query && activeGroup === "individual" && (
              <IndividualSettings
                fontFamilies={fontFamilies}
                individualLabels={individualLabels}
                individuals={individuals}
                installedFontKeys={installedFontKeys}
                onAdd={addIndividual}
                onCommit={(next) => void commitIndividuals(next)}
                t={t}
              />
            )}

            {mode === "all" && !query && activeGroup === "lists" && (
              <ListsEditor
                definitions={listDefinitions}
                entries={lists}
                fontFamilies={fontFamilies}
                fontOptionLabel={fontOptionLabel}
                installedFontKeys={installedFontKeys}
                onUpdateList={(kind, entries) => void updateList(kind, entries)}
                t={t}
              />
            )}
          </div>

          <ProfilePreviewPanel
            ciSmoke={ciSmoke}
            docked={previewDocked}
            error={previewError}
            fontFace={fontFace}
            fontFamilies={fontFamilies}
            fontOptionLabel={fontOptionLabel}
            mode={mode}
            onError={setPreviewError}
            onFontFaceChange={setFontFace}
            onPreviewReady={onPreviewReady}
            profilePath={profile?.path ?? null}
            ref={previewPanelRef}
            savedValues={savedValues}
            t={t}
            values={values}
            variants={previewVariants}
          />
        </div>
      </div>
    </section>
  );
}
