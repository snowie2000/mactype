import { Trash2 } from "lucide-react";
import type { IndividualSetting } from "../../app/model";
import type { I18nValue } from "../../i18n/i18n";

interface IndividualSettingsProps {
  individuals: ReadonlyArray<IndividualSetting>;
  individualLabels: ReadonlyArray<string>;
  fontFamilies: ReadonlyArray<string>;
  installedFontKeys: ReadonlySet<string>;
  onAdd: (font: string) => void;
  onCommit: (individuals: IndividualSetting[]) => void;
  t: I18nValue["t"];
}

export function IndividualSettings({ individuals, individualLabels, fontFamilies, installedFontKeys, onAdd, onCommit, t }: IndividualSettingsProps) {
  return (
    <div className="collection-editor">
      <div className="font-picker-row"><label htmlFor="individual-font-picker">{t("profiles.selectFont")}</label><select id="individual-font-picker" onChange={(event) => onAdd(event.target.value)} value=""><option value="">{t("profiles.selectFont")}</option>{fontFamilies.filter((font) => installedFontKeys.has(font.toLocaleLowerCase()) && !individuals.some((entry) => entry.fontFace.toLocaleLowerCase() === font.toLocaleLowerCase())).map((font) => <option key={font} value={font}>{font}</option>)}</select></div>
      {individuals.map((entry, rowIndex) => (
        <div className="individual-row" key={`${entry.fontFace}-${rowIndex}`}>
          <strong>{entry.fontFace}</strong>
          <div>{individualLabels.map((label, valueIndex) => <label key={label}><span>{label}</span><input aria-label={`${entry.fontFace} ${label}`} max={valueIndex === 2 ? 64 : valueIndex === 3 || valueIndex === 4 ? 32 : valueIndex === 1 ? 6 : valueIndex === 0 ? 2 : 1} min={valueIndex === 2 ? -64 : valueIndex === 3 || valueIndex === 4 ? -32 : valueIndex === 1 ? -1 : 0} onChange={(event) => { const next = individuals.map((item) => ({ ...item, values: [...item.values] })); next[rowIndex].values[valueIndex] = event.target.value === "" ? null : Number(event.target.value); onCommit(next); }} placeholder={t("profiles.inherit")} type="number" value={entry.values[valueIndex] ?? ""} /></label>)}</div>
          <button className="icon-button" aria-label={t("profiles.remove", { name: entry.fontFace })} onClick={() => onCommit(individuals.filter((_, index) => index !== rowIndex))} type="button"><Trash2 aria-hidden="true" size={15} /></button>
        </div>
      ))}
      {individuals.length === 0 && <p className="empty-state">{t("profiles.emptyIndividuals")}</p>}
    </div>
  );
}
