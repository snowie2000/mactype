import { Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { listManualLaunchCandidates } from "../../app/tauri";
import { Hint } from "../../components/Hint";
import type { I18nValue } from "../../i18n/i18n";

export type ListKind = "excludeFonts" | "includeFonts" | "excludeModules" | "includeModules" | "unloadDlls" | "excludeSubstitutionModules";

export interface ListDefinition {
  kind: ListKind;
  label: string;
  help: string;
}

const processSuggestionsId = "list-process-suggestions";

interface ListsEditorProps {
  definitions: ReadonlyArray<ListDefinition>;
  entries: Readonly<Record<string, ReadonlyArray<string>>>;
  fontFamilies: ReadonlyArray<string>;
  installedFontKeys: ReadonlySet<string>;
  fontOptionLabel: (font: string) => string;
  onUpdateList: (kind: ListKind, entries: ReadonlyArray<string>) => void;
  t: I18nValue["t"];
}

export function ListsEditor({ definitions, entries, fontFamilies, installedFontKeys, fontOptionLabel, onUpdateList, t }: ListsEditorProps) {
  const [processNames, setProcessNames] = useState<ReadonlyArray<string>>([]);
  const [pending, setPending] = useState<Record<string, string>>({});
  const [rejections, setRejections] = useState<Record<string, string>>({});

  useEffect(() => {
    let active = true;
    void listManualLaunchCandidates()
      .then((candidates) => {
        if (active) setProcessNames([...new Set(candidates.map((candidate) => candidate.name))].sort((left, right) => left.localeCompare(right)));
      })
      .catch(() => {
        // Suggestions are optional; typing a name still works without them.
      });
    return () => {
      active = false;
    };
  }, []);

  const clearRejection = (kind: ListKind) => {
    setRejections((current) => ({ ...current, [kind]: "" }));
  };

  const addPendingEntry = (kind: ListKind) => {
    const value = (pending[kind] ?? "").trim();
    if (!value) return;
    const current = entries[kind] ?? [];
    if (current.some((entry) => entry.toLocaleLowerCase() === value.toLocaleLowerCase())) {
      setRejections((existing) => ({ ...existing, [kind]: t("profiles.duplicateListEntry", { name: value }) }));
      return;
    }
    onUpdateList(kind, [...current, value]);
    setPending((existing) => ({ ...existing, [kind]: "" }));
    clearRejection(kind);
  };

  const removeEntry = (kind: ListKind, entry: string) => {
    onUpdateList(kind, (entries[kind] ?? []).filter((existing) => existing !== entry));
    clearRejection(kind);
  };

  return (
    <div className="list-grid">
      {definitions.map((definition) => {
        const kind = definition.kind;
        const listEntries = entries[kind] ?? [];
        const isFontList = kind === "excludeFonts" || kind === "includeFonts";
        const availableFonts = isFontList
          ? fontFamilies.filter((font) => installedFontKeys.has(font.toLocaleLowerCase()) && !listEntries.some((entry) => entry.toLocaleLowerCase() === font.toLocaleLowerCase()))
          : [];
        const rejection = rejections[kind] ?? "";
        return (
          <section className="list-editor" key={kind}>
            <strong><Hint content={definition.help}>{definition.label}</Hint></strong>
            {listEntries.length > 0
              ? <ul>{listEntries.map((entry) => <li key={entry}><code>{isFontList ? fontOptionLabel(entry) : entry}</code><button aria-label={t("profiles.remove", { name: entry })} className="icon-button" onClick={() => removeEntry(kind, entry)} type="button"><Trash2 aria-hidden="true" size={14} /></button></li>)}</ul>
              : <p className="list-editor-empty">{t("profiles.emptyList")}</p>}
            {isFontList
              ? <select aria-label={`${definition.label} · ${t("profiles.addFontToList")}`} disabled={availableFonts.length === 0} onChange={(event) => { if (event.target.value) onUpdateList(kind, [...listEntries, event.target.value]); }} value=""><option value="">{t("profiles.addFontToList")}</option>{availableFonts.map((font) => <option key={font} value={font}>{font}</option>)}</select>
              : (
                <form className="list-add-row" onSubmit={(event) => { event.preventDefault(); addPendingEntry(kind); }}>
                  <input aria-label={`${definition.label} · ${t("profiles.addListEntry")}`} list={processSuggestionsId} onChange={(event) => { const value = event.target.value; setPending((existing) => ({ ...existing, [kind]: value })); clearRejection(kind); }} placeholder={t("profiles.listEntryPlaceholder")} type="text" value={pending[kind] ?? ""} />
                  <button aria-label={`${definition.label} · ${t("profiles.addListEntry")}`} className="button" disabled={!(pending[kind] ?? "").trim()} type="submit"><Plus aria-hidden="true" size={14} /> {t("profiles.addListEntry")}</button>
                </form>
              )}
            {rejection && <p className="inline-error" role="alert">{rejection}</p>}
          </section>
        );
      })}
      <datalist id={processSuggestionsId}>{processNames.map((name) => <option key={name} value={name} />)}</datalist>
    </div>
  );
}
