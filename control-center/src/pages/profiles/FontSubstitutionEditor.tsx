import { ArrowRight, Plus, Trash2 } from "lucide-react";
import type { AdvancedProfile } from "../../app/model";
import type { I18nValue } from "../../i18n/i18n";
import { splitSubstitution } from "./profileEditorUtils";

interface FontSubstitutionEditorProps {
  advanced: AdvancedProfile;
  fontFamilies: ReadonlyArray<string>;
  fontOptionLabel: (font: string) => string;
  onCommit: (profile: AdvancedProfile) => void;
  t: I18nValue["t"];
}

export function FontSubstitutionEditor({ advanced, fontFamilies, fontOptionLabel, onCommit, t }: FontSubstitutionEditorProps) {
  const updateSubstitution = (index: number, key: "source" | "replacement", value: string) => {
    const substitutions = advanced.fontSubstitutes.map((mapping, currentIndex) => {
      if (currentIndex !== index) return mapping;
      const pair = splitSubstitution(mapping);
      return `${key === "source" ? value : pair.source}=${key === "replacement" ? value : pair.replacement}`;
    });
    onCommit({ ...advanced, fontSubstitutes: substitutions });
  };

  const addSubstitution = () => {
    if (fontFamilies.length < 2) return;
    const used = new Set(advanced.fontSubstitutes.map((mapping) => splitSubstitution(mapping).source.toLocaleLowerCase()));
    const source = fontFamilies.find((font) => !used.has(font.toLocaleLowerCase())) ?? fontFamilies[0];
    const replacement = fontFamilies.find((font) => font !== source && font === "Segoe UI")
      ?? fontFamilies.find((font) => font !== source)
      ?? source;
    onCommit({ ...advanced, fontSubstitutes: [...advanced.fontSubstitutes, `${source}=${replacement}`] });
  };

  return (
    <div className="font-substitution-editor">
      <div className="font-substitution-list">
        {advanced.fontSubstitutes.map((mapping, index) => {
          const pair = splitSubstitution(mapping);
          return (
            <div className="font-substitution-row" key={`${mapping}-${index}`}>
              <label><span className="sr-only">{t("profiles.sourceFont")}</span><select aria-label={t("profiles.sourceFont")} onChange={(event) => updateSubstitution(index, "source", event.target.value)} value={pair.source}>{fontFamilies.map((font) => <option key={font} value={font}>{fontOptionLabel(font)}</option>)}</select></label>
              <ArrowRight aria-hidden="true" size={16} />
              <label><span className="sr-only">{t("profiles.replacementFont")}</span><select aria-label={t("profiles.replacementFont")} onChange={(event) => updateSubstitution(index, "replacement", event.target.value)} value={pair.replacement}>{fontFamilies.map((font) => <option key={font} value={font}>{fontOptionLabel(font)}</option>)}</select></label>
              <button className="icon-button" aria-label={t("profiles.remove", { name: pair.source })} onClick={() => onCommit({ ...advanced, fontSubstitutes: advanced.fontSubstitutes.filter((_, currentIndex) => currentIndex !== index) })} type="button"><Trash2 aria-hidden="true" size={15} /></button>
            </div>
          );
        })}
      </div>
      <button className="button secondary font-add-button" disabled={fontFamilies.length < 2} onClick={addSubstitution} type="button"><Plus aria-hidden="true" size={16} /> {t("profiles.addSubstitution")}</button>
    </div>
  );
}
