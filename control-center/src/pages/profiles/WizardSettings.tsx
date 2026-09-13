import { ArrowLeft, ArrowRight, Eye, ListRestart, Play, Redo2, RotateCcw, Save, Undo2 } from "lucide-react";
import type { AdvancedProfile } from "../../app/model";
import type { SettingDefinition } from "../../generated/settings";
import type { I18nValue, MessageKey } from "../../i18n/i18n";
import { FontSubstitutionEditor } from "./FontSubstitutionEditor";
import { SchemaSettings } from "./SchemaSettings";
import { stepSupportsHistory, wizardScaleBySettingId, wizardSettingIdsByStep, wizardStepIds, type WizardStepId } from "./wizardModel";

interface WizardSettingsProps {
  activeStep: WizardStepId;
  advanced: AdvancedProfile;
  busy: boolean;
  canRedoStep: boolean;
  canSave: boolean;
  canUndoStep: boolean;
  dirtyCount: number;
  dirtyKeys: ReadonlyArray<string>;
  fontFace: string;
  fontFamilies: ReadonlyArray<string>;
  fontOptionLabel: (font: string) => string;
  onAdvancedCommit: (profile: AdvancedProfile) => void;
  onApply: () => void;
  onFontFaceChange: (font: string) => void;
  onPreview: () => void;
  onRedoStep: () => void;
  onSave: () => void;
  onSettingChange: (settingId: string, value: number) => void;
  onSettingPreview: (settingId: string, value: number) => void;
  onStepChange: (step: WizardStepId) => void;
  onUndoStep: () => void;
  profileName: string | null;
  profilePath: string | null;
  savedValues?: Readonly<Record<string, number>>;
  settings: ReadonlyArray<SettingDefinition>;
  t: I18nValue["t"];
  values: Readonly<Record<string, number>>;
}

export function WizardSettings({
  activeStep,
  advanced,
  busy,
  canRedoStep,
  canSave,
  canUndoStep,
  dirtyCount,
  dirtyKeys,
  fontFace,
  fontFamilies,
  fontOptionLabel,
  onAdvancedCommit,
  onApply,
  onFontFaceChange,
  onPreview,
  onRedoStep,
  onSave,
  onSettingChange,
  onSettingPreview,
  onStepChange,
  onUndoStep,
  profileName,
  profilePath,
  savedValues,
  settings,
  t,
  values,
}: WizardSettingsProps) {
  const stepIndex = wizardStepIds.indexOf(activeStep);
  /* Keep the step's own order (legacy Tuner screen order), not schema order. */
  const currentSettings = wizardSettingIdsByStep[activeStep]
    .map((settingId) => settings.find((setting) => setting.id === settingId))
    .filter((setting): setting is SettingDefinition => setting !== undefined);
  const previousStep = wizardStepIds[stepIndex - 1];
  const nextStep = wizardStepIds[stepIndex + 1];
  const stepAtFactory = currentSettings.every((setting) => (values[setting.id] ?? setting.default) === setting.factory);
  const endpointWords = (settingId: string) => {
    const scale = wizardScaleBySettingId[settingId];
    if (!scale) return null;
    return { low: t(`wizard.scale.${scale}.low` as MessageKey), high: t(`wizard.scale.${scale}.high` as MessageKey) };
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
    <div className="wizard-layout">
      <div className="wizard-step-content">
        {activeStep === "start" && (
          <section className="wizard-start-card" aria-label={t("wizard.start")}>
            <p className="wizard-start-intro">{t("wizard.startIntro")}</p>
            <div className="wizard-start-profile">
              <span>{t("wizard.startProfile")}</span>
              <code title={profilePath ?? undefined}>{profileName ?? t("profiles.none")}</code>
            </div>
            <label className="wizard-start-font">
              <span>{t("profiles.previewFont")}</span>
              <select disabled={busy} onChange={(event) => onFontFaceChange(event.target.value)} value={fontFace}>
                {fontFamilies.map((font) => <option key={font} value={font}>{fontOptionLabel(font)}</option>)}
              </select>
            </label>
            <p className="wizard-start-hint">{t("wizard.startSwitchHint")}</p>
          </section>
        )}
        {stepToolsAvailable && (
          <div aria-label={t("wizard.stepTools")} className="wizard-step-tools" role="toolbar">
            <button className="text-action" disabled={busy || !canUndoStep} onClick={onUndoStep} type="button">
              <Undo2 aria-hidden="true" size={14} /> {t("profiles.undo")}
            </button>
            <button className="text-action" disabled={busy || !canRedoStep} onClick={onRedoStep} type="button">
              <Redo2 aria-hidden="true" size={14} /> {t("profiles.redo")}
            </button>
            <button className="text-action" disabled={busy || stepAtSaved} onClick={discardStepChanges} title={t("wizard.discardStepDescription")} type="button">
              <RotateCcw aria-hidden="true" size={14} /> {t("wizard.discardStep")}
            </button>
            <button className="text-action" disabled={busy || stepAtFactory} onClick={resetStepToFactory} type="button">
              <ListRestart aria-hidden="true" size={14} /> {t("wizard.resetStep")}
            </button>
          </div>
        )}
        {activeStep !== "start" && activeStep !== "apply" && <SchemaSettings dirtyKeys={dirtyKeys} endpointWords={endpointWords} onChange={onSettingChange} onPreviewChange={onSettingPreview} savedValues={savedValues} settings={currentSettings} t={t} values={values} variant="guided" />}
        {activeStep === "substitution" && (
          <div className="advanced-editor wizard-substitution">
            <fieldset><FontSubstitutionEditor advanced={advanced} fontFamilies={fontFamilies} fontOptionLabel={fontOptionLabel} onCommit={onAdvancedCommit} t={t} /></fieldset>
          </div>
        )}
        {activeStep === "apply" && (
          <section className="wizard-apply-card" aria-label={t("wizard.apply")}>
            <p data-dirty={dirtyCount > 0}>{dirtyCount > 0 ? t("wizard.unsavedWarning") : t("wizard.savedState")}</p>
            <div>
              <button className="button secondary" onClick={onPreview} type="button"><Eye aria-hidden="true" size={16} /> {t("profiles.preview")}</button>
              <button className="button secondary" disabled={busy || dirtyCount === 0 || !canSave} onClick={onSave} type="button"><Save aria-hidden="true" size={16} /> {t("wizard.saveProfile")}</button>
              <button className="button primary" disabled={busy || dirtyCount > 0} onClick={onApply} title={dirtyCount > 0 ? t("profiles.saveBeforeApply") : undefined} type="button"><Play aria-hidden="true" size={16} /> {t("wizard.applyMacType")}</button>
            </div>
          </section>
        )}
      </div>
      <nav className="wizard-progress" aria-label={t("wizard.progress")}>
        {previousStep && <button className="button secondary" onClick={() => onStepChange(previousStep)} type="button"><ArrowLeft aria-hidden="true" size={16} /> {t("wizard.previous")}</button>}
        {nextStep && <button className="button primary wizard-next" onClick={() => onStepChange(nextStep)} type="button">{t("wizard.next")} <ArrowRight aria-hidden="true" size={16} /></button>}
      </nav>
    </div>
  );
}
