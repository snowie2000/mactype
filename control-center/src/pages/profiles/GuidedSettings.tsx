import { ArrowLeft, ArrowRight, Eye, ListRestart, Play, Redo2, RotateCcw, Save, SaveAll, Undo2, Upload } from "lucide-react";
import type { AdvancedProfile } from "../../app/model";
import type { SettingDefinition } from "../../generated/settings";
import type { I18nValue, MessageKey } from "../../i18n/i18n";
import { FontSubstitutionEditor } from "./FontSubstitutionEditor";
import { SchemaSettings } from "./SchemaSettings";
import { stepSupportsHistory, guidedScaleBySettingId, guidedSettingIdsByStep, guidedStepIds, type GuidedStepId } from "./guidedModel";

interface GuidedSettingsProps {
  activeStep: GuidedStepId;
  advanced: AdvancedProfile;
  applyDescription: string;
  applying: boolean;
  busy: boolean;
  canApply: boolean;
  canRedoStep: boolean;
  canSave: boolean;
  canUndoStep: boolean;
  dirtyCount: number;
  dirtyKeys: ReadonlyArray<string>;
  fontFace: string;
  fontFamilies: ReadonlyArray<string>;
  fontOptionLabel: (font: string) => string;
  offerStart: boolean;
  onAdvancedCommit: (profile: AdvancedProfile) => void;
  onApply: () => void;
  onFontFaceChange: (font: string) => void;
  onPreview: () => void;
  onRedoStep: () => void;
  onSave: () => void;
  onSaveAs: () => void;
  onStartService: () => void;
  onSettingChange: (settingId: string, value: number) => void;
  onSettingPreview: (settingId: string, value: number) => void;
  onStepChange: (step: GuidedStepId) => void;
  onUndoStep: () => void;
  profileName: string | null;
  profilePath: string | null;
  savedValues?: Readonly<Record<string, number>>;
  settings: ReadonlyArray<SettingDefinition>;
  starting: boolean;
  t: I18nValue["t"];
  values: Readonly<Record<string, number>>;
}

export function GuidedSettings({
  activeStep,
  advanced,
  applyDescription,
  applying,
  busy,
  canApply,
  canRedoStep,
  canSave,
  canUndoStep,
  dirtyCount,
  dirtyKeys,
  fontFace,
  fontFamilies,
  fontOptionLabel,
  offerStart,
  onAdvancedCommit,
  onApply,
  onFontFaceChange,
  onPreview,
  onRedoStep,
  onSave,
  onSaveAs,
  onStartService,
  onSettingChange,
  onSettingPreview,
  onStepChange,
  onUndoStep,
  profileName,
  profilePath,
  savedValues,
  settings,
  starting,
  t,
  values,
}: GuidedSettingsProps) {
  const stepIndex = guidedStepIds.indexOf(activeStep);
  /* Keep the step's own order (legacy Tuner screen order), not schema order. */
  const currentSettings = guidedSettingIdsByStep[activeStep]
    .map((settingId) => settings.find((setting) => setting.id === settingId))
    .filter((setting): setting is SettingDefinition => setting !== undefined);
  const previousStep = guidedStepIds[stepIndex - 1];
  const nextStep = guidedStepIds[stepIndex + 1];
  const stepAtFactory = currentSettings.every((setting) => (values[setting.id] ?? setting.default) === setting.factory);
  const endpointWords = (settingId: string) => {
    const scale = guidedScaleBySettingId[settingId];
    if (!scale) return null;
    return { low: t(`guided.scale.${scale}.low` as MessageKey), high: t(`guided.scale.${scale}.high` as MessageKey) };
  };
  const resetStepToFactory = () => {
    for (const setting of currentSettings) {
      if ((values[setting.id] ?? setting.default) !== setting.factory) onSettingChange(setting.id, setting.factory);
    }
  };
  /* Discard only this step's settings back to their saved values. */
  const stepToolsAvailable = currentSettings.length > 0 && stepSupportsHistory(activeStep);
  const stepAtSaved = currentSettings.every((setting) => {
    const saved = savedValues?.[setting.id];
    return saved === undefined || (values[setting.id] ?? setting.default) === saved;
  });
  const discardStepChanges = () => {
    for (const setting of currentSettings) {
      const saved = savedValues?.[setting.id];
      if (saved !== undefined && (values[setting.id] ?? setting.default) !== saved) onSettingChange(setting.id, saved);
    }
  };

  return (
    <div className="guided-layout">
      <div className="guided-step-content">
        {activeStep === "start" && (
          <section className="guided-start-card" aria-label={t("guided.start")}>
            <p className="guided-start-intro">{t("guided.startIntro")}</p>
            <div className="guided-start-profile">
              <span>{t("guided.startProfile")}</span>
              <code title={profilePath ?? undefined}>{profileName ?? t("profiles.none")}</code>
            </div>
            <label className="guided-start-font">
              <span>{t("profiles.previewFont")}</span>
              <select disabled={busy} onChange={(event) => onFontFaceChange(event.target.value)} value={fontFace}>
                {fontFamilies.map((font) => <option key={font} value={font}>{fontOptionLabel(font)}</option>)}
              </select>
            </label>
            <p className="guided-start-hint">{t("guided.startSwitchHint")}</p>
          </section>
        )}
        {stepToolsAvailable && (
          <div aria-label={t("guided.stepTools")} className="guided-step-tools" role="toolbar">
            <button className="text-action" disabled={busy || !canUndoStep} onClick={onUndoStep} type="button">
              <Undo2 aria-hidden="true" size={14} /> {t("profiles.undo")}
            </button>
            <button className="text-action" disabled={busy || !canRedoStep} onClick={onRedoStep} type="button">
              <Redo2 aria-hidden="true" size={14} /> {t("profiles.redo")}
            </button>
            <button className="text-action" disabled={busy || stepAtSaved} onClick={discardStepChanges} title={t("guided.discardStepDescription")} type="button">
              <RotateCcw aria-hidden="true" size={14} /> {t("guided.discardStep")}
            </button>
            <button className="text-action" disabled={busy || stepAtFactory} onClick={resetStepToFactory} type="button">
              <ListRestart aria-hidden="true" size={14} /> {t("guided.resetStep")}
            </button>
          </div>
        )}
        {activeStep !== "start" && activeStep !== "apply" && <SchemaSettings dirtyKeys={dirtyKeys} endpointWords={endpointWords} onChange={onSettingChange} onPreviewChange={onSettingPreview} savedValues={savedValues} settings={currentSettings} t={t} values={values} variant="guided" />}
        {activeStep === "substitution" && (
          <div className="advanced-editor guided-substitution">
            <fieldset><FontSubstitutionEditor advanced={advanced} fontFamilies={fontFamilies} fontOptionLabel={fontOptionLabel} onCommit={onAdvancedCommit} t={t} /></fieldset>
          </div>
        )}
        {activeStep === "apply" && (
          <section className="guided-apply-card" aria-label={t("guided.apply")}>
            <p data-dirty={dirtyCount > 0}>{dirtyCount > 0 ? t("guided.unsavedWarning") : t("guided.savedState")}</p>
            <div>
              <button className="button secondary" onClick={onPreview} type="button"><Eye aria-hidden="true" size={16} /> {t("profiles.preview")}</button>
              <button className="button secondary" disabled={busy || dirtyCount === 0 || !canSave} data-dirty={dirtyCount > 0} onClick={onSave} type="button"><Save aria-hidden="true" size={16} /> {t("guided.saveProfile")}</button>
              <button className="button secondary" disabled={busy} onClick={onSaveAs} type="button"><SaveAll aria-hidden="true" size={16} /> {t("files.saveAs")}</button>
              {offerStart
                ? <button className="button designate" disabled={busy} onClick={onStartService} type="button"><Play aria-hidden="true" size={16} /> {starting ? t("execution.serviceWorking") : t("files.startServiceNow")}</button>
                : <button className="button designate" disabled={busy || !canApply} onClick={onApply} title={applyDescription} type="button"><Upload aria-hidden="true" size={16} /> {applying ? t("profiles.applyingToService") : t("profiles.apply")}</button>}
            </div>
          </section>
        )}
      </div>
      <nav className="guided-progress" aria-label={t("guided.progress")}>
        {previousStep && <button className="button secondary" onClick={() => onStepChange(previousStep)} type="button"><ArrowLeft aria-hidden="true" size={16} /> {t("guided.previous")}</button>}
        {nextStep && <button className="button primary guided-next" onClick={() => onStepChange(nextStep)} type="button">{t("guided.next")} <ArrowRight aria-hidden="true" size={16} /></button>}
      </nav>
    </div>
  );
}
