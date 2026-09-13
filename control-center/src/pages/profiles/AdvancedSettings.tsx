import type { AdvancedProfile } from "../../app/model";
import { Hint } from "../../components/Hint";
import type { SettingDefinition } from "../../generated/settings";
import type { I18nValue } from "../../i18n/i18n";
import { FontSubstitutionEditor } from "./FontSubstitutionEditor";
import { SchemaSettings } from "./SchemaSettings";

interface AdvancedSettingsProps {
  settings: ReadonlyArray<SettingDefinition>;
  values: Readonly<Record<string, number>>;
  savedValues?: Readonly<Record<string, number>>;
  dirtyKeys: ReadonlyArray<string>;
  advanced: AdvancedProfile;
  fontFamilies: ReadonlyArray<string>;
  fontOptionLabel: (font: string) => string;
  onSettingChange: (settingId: string, value: number) => void;
  onSettingPreview: (settingId: string, value: number) => void;
  onAdvancedChange: (profile: AdvancedProfile) => void;
  onAdvancedCommit: (profile: AdvancedProfile) => void;
  t: I18nValue["t"];
}

interface AdvancedSectionProps {
  advanced: AdvancedProfile;
  onChange: (profile: AdvancedProfile) => void;
  onCommit: (profile: AdvancedProfile) => void;
  t: I18nValue["t"];
}

const updateVector = (values: ReadonlyArray<number>, index: number, value: number) =>
  values.map((current, currentIndex) => currentIndex === index ? value : current);

const colorValue = (value: number) => `#${value.toString(16).padStart(6, "0")}`;

function ShadowSettings({ advanced, onChange, onCommit, t }: AdvancedSectionProps) {
  const shadow = advanced.shadow;
  const numberFields = ["offsetX", "offsetY", "darkAlpha", "lightAlpha"] as const;
  const colorFields = ["darkColor", "lightColor"] as const;

  return (
    <fieldset>
      <legend><Hint content={t("advanced.shadowHelp")}>{t("advanced.shadow")}</Hint></legend>
      <label className="checkbox-row">
        <input
          checked={shadow !== null}
          onChange={(event) => onCommit({
            ...advanced,
            shadow: event.target.checked
              ? { offsetX: 1, offsetY: 1, darkAlpha: 4, darkColor: 0, lightAlpha: 4, lightColor: 0 }
              : null,
          })}
          type="checkbox"
        /> {t("advanced.enableCustom")}
      </label>
      {shadow && (
        <div className="advanced-grid">
          {numberFields.map((key) => (
            <label key={key}>
              <span>{t(`advanced.${key}`)}</span>
              <input
                max={key.includes("Alpha") ? 255 : 128}
                min={key.includes("Alpha") ? 0 : -128}
                onBlur={() => onCommit(advanced)}
                onChange={(event) => onChange({ ...advanced, shadow: { ...shadow, [key]: Number(event.target.value) } })}
                type="number"
                value={shadow[key]}
              />
            </label>
          ))}
          {colorFields.map((key) => (
            <label key={key}>
              <span>{t(`advanced.${key}`)}</span>
              <input
                onChange={(event) => onCommit({ ...advanced, shadow: { ...shadow, [key]: Number.parseInt(event.target.value.slice(1), 16) } })}
                type="color"
                value={colorValue(shadow[key])}
              />
            </label>
          ))}
        </div>
      )}
    </fieldset>
  );
}

function LcdFilterSettings({ advanced, onChange, onCommit, t }: AdvancedSectionProps) {
  const weights = advanced.lcdFilterWeight;
  return (
    <fieldset>
      <legend><Hint content={t("advanced.lcdWeightHelp")}>{t("advanced.lcdWeight")}</Hint></legend>
      <label className="checkbox-row">
        <input
          checked={weights !== null}
          onChange={(event) => onCommit({ ...advanced, lcdFilterWeight: event.target.checked ? [8, 77, 86, 77, 8] : null })}
          type="checkbox"
        /> {t("advanced.enableCustom")}
      </label>
      {weights && (
        <div className="vector-grid">
          {weights.map((value, index) => (
            <label key={index}>
              <span>{index + 1}</span>
              <input
                max={255}
                min={0}
                onBlur={() => onCommit(advanced)}
                onChange={(event) => onChange({ ...advanced, lcdFilterWeight: updateVector(weights, index, Number(event.target.value)) })}
                type="number"
                value={value}
              />
            </label>
          ))}
        </div>
      )}
    </fieldset>
  );
}

function PixelLayoutSettings({ advanced, onChange, onCommit, t }: AdvancedSectionProps) {
  const pixelLayout = advanced.pixelLayout;
  const labels = ["R x", "R y", "G x", "G y", "B x", "B y"];
  return (
    <fieldset>
      <legend><Hint content={t("advanced.pixelLayoutHelp")}>{t("advanced.pixelLayout")}</Hint></legend>
      <label className="checkbox-row">
        <input
          checked={pixelLayout !== null}
          onChange={(event) => onCommit({ ...advanced, pixelLayout: event.target.checked ? [-21, 0, 0, 0, 21, 0] : null })}
          type="checkbox"
        /> {t("advanced.enableCustom")}
      </label>
      {pixelLayout && (
        <div className="vector-grid">
          {pixelLayout.map((value, index) => (
            <label key={labels[index]}>
              <span>{labels[index]}</span>
              <input
                max={127}
                min={-128}
                onBlur={() => onCommit(advanced)}
                onChange={(event) => onChange({ ...advanced, pixelLayout: updateVector(pixelLayout, index, Number(event.target.value)) })}
                type="number"
                value={value}
              />
            </label>
          ))}
        </div>
      )}
    </fieldset>
  );
}

function RoutingSettings({
  advanced,
  fontFamilies,
  fontOptionLabel,
  onCommit,
  t,
}: AdvancedSectionProps & Pick<AdvancedSettingsProps, "fontFamilies" | "fontOptionLabel">) {
  return (
    <fieldset className="advanced-text-fields">
      <legend><Hint content={t("advanced.fontSubstitutesHelp")}>{t("advanced.fontSubstitutes")}</Hint></legend>
      <FontSubstitutionEditor advanced={advanced} fontFamilies={fontFamilies} fontOptionLabel={fontOptionLabel} onCommit={onCommit} t={t} />
    </fieldset>
  );
}

export function AdvancedSettings({
  settings,
  values,
  savedValues,
  dirtyKeys,
  advanced,
  fontFamilies,
  fontOptionLabel,
  onSettingChange,
  onSettingPreview,
  onAdvancedChange,
  onAdvancedCommit,
  t,
}: AdvancedSettingsProps) {
  const sectionProps = { advanced, onChange: onAdvancedChange, onCommit: onAdvancedCommit, t };
  return (
    <>
      <SchemaSettings dirtyKeys={dirtyKeys} onChange={onSettingChange} onPreviewChange={onSettingPreview} savedValues={savedValues} settings={settings} t={t} values={values} />
      <div className="advanced-editor">
        <ShadowSettings {...sectionProps} />
        <LcdFilterSettings {...sectionProps} />
        <PixelLayoutSettings {...sectionProps} />
        <RoutingSettings {...sectionProps} fontFamilies={fontFamilies} fontOptionLabel={fontOptionLabel} />
      </div>
    </>
  );
}
