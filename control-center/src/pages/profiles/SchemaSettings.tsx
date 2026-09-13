import { RotateCcw, Undo2 } from "lucide-react";
import { useRef, useState } from "react";
import { Hint } from "../../components/Hint";
import type { SettingDefinition } from "../../generated/settings";
import {
  settingMessageKey,
  settingOptionMessageKey,
  type I18nValue,
} from "../../i18n/i18n";

interface SchemaSettingsProps {
  settings: ReadonlyArray<SettingDefinition>;
  values: Readonly<Record<string, number>>;
  savedValues?: Readonly<Record<string, number>>;
  dirtyKeys: ReadonlyArray<string>;
  onChange: (settingId: string, value: number) => void;
  onPreviewChange: (settingId: string, value: number) => void;
  t: I18nValue["t"];
  variant?: "compact" | "guided";
  endpointWords?: (settingId: string) => { low: string; high: string } | null;
}

interface SettingControlProps {
  setting: SettingDefinition;
  settingLabel: string;
  hintId: string;
  value: number;
  savedValue: number;
  onCommit: (value: number) => void;
  onPreview: (value: number) => void;
  t: I18nValue["t"];
  guided?: boolean;
  endpoints?: { low: string; high: string } | null;
}

const rangeAdjustmentKeys = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End"]);

function SettingControl({ setting, settingLabel, hintId, value, savedValue, onCommit, onPreview, t, guided = false, endpoints = null }: SettingControlProps) {
  const rangeStart = useRef<number | null>(null);
  const numberStart = useRef<number | null>(null);
  const [numberDraft, setNumberDraft] = useState<string | null>(null);
  const step = setting.type === "integer" ? 1 : 0.01;
  const normalize = (next: number) => Math.min(setting.max, Math.max(setting.min, setting.type === "integer" ? Math.round(next) : next));
  const parseNumber = (raw: string) => {
    if (raw.trim() === "") return null;
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : null;
  };
  const beginRange = () => {
    if (rangeStart.current === null) rangeStart.current = value;
  };
  const finishRange = (next: number) => {
    const start = rangeStart.current;
    rangeStart.current = null;
    if (start !== null && next !== start) onCommit(next);
  };
  const previewRange = (next: number) => {
    onPreview(next);
    if (rangeStart.current === null) onCommit(next);
  };
  const beginNumber = () => {
    if (numberStart.current === null) numberStart.current = value;
    setNumberDraft(String(value));
  };
  const previewNumber = (raw: string) => {
    setNumberDraft(raw);
    const next = parseNumber(raw);
    if (next !== null && next >= setting.min && next <= setting.max) onPreview(normalize(next));
  };
  const finishNumber = (raw: string) => {
    const start = numberStart.current;
    if (start === null) return;
    numberStart.current = null;
    setNumberDraft(null);
    const parsed = parseNumber(raw);
    if (parsed === null) {
      onPreview(start);
      return;
    }
    const next = normalize(parsed);
    onPreview(next);
    if (next !== start) onCommit(next);
  };
  const cancelNumber = () => {
    const start = numberStart.current;
    numberStart.current = null;
    setNumberDraft(null);
    if (start !== null) onPreview(start);
  };
  const exactValueInput = (
    <input
      aria-describedby={hintId}
      aria-label={settingLabel}
      id={setting.control === "number" ? setting.id : `${setting.id}-value`}
      inputMode="decimal"
      max={setting.max}
      min={setting.min}
      onBlur={(event) => finishNumber(event.currentTarget.value)}
      onChange={(event) => previewNumber(event.currentTarget.value)}
      onFocus={beginNumber}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          finishNumber(event.currentTarget.value);
          event.currentTarget.blur();
        } else if (event.key === "Escape") {
          event.preventDefault();
          cancelNumber();
          event.currentTarget.blur();
        }
      }}
      step={step}
      type="number"
      value={numberDraft ?? value}
    />
  );
  const controlClass = setting.control === "range"
    ? "range-control"
    : `range-control ${setting.control === "number" ? "number-control" : "discrete-control"}`;

  return (
    <div className={controlClass}>
      {setting.control === "select" && "options" in setting && guided ? (
        <fieldset aria-describedby={hintId} className="guided-choice">
          <legend className="sr-only">{settingLabel}</legend>
          {setting.options.map((option) => (
            <label className="guided-choice-option" data-selected={value === option.value} key={option.value}>
              <input checked={value === option.value} name={setting.id} onChange={() => onCommit(option.value)} type="radio" value={option.value} />
              <span>{t(settingOptionMessageKey(setting.id, option.value))}</span>
            </label>
          ))}
        </fieldset>
      ) : setting.control === "select" && "options" in setting ? (
        <select aria-describedby={hintId} id={setting.id} onChange={(event) => onCommit(Number(event.target.value))} value={value}>
          {setting.options.map((option) => (
            <option key={option.value} value={option.value}>
              {t(settingOptionMessageKey(setting.id, option.value))}
            </option>
          ))}
        </select>
      ) : setting.control === "boolean" ? (
        <label className="switch-control">
          <input aria-describedby={hintId} checked={value === 1} id={setting.id} onChange={(event) => onCommit(event.target.checked ? 1 : 0)} role="switch" type="checkbox" />
          <span aria-hidden="true">{value === 1 ? t("common.on") : t("common.off")}</span>
        </label>
      ) : setting.control === "number" ? exactValueInput : (
        <input
          aria-describedby={hintId}
          id={setting.id}
          max={setting.max}
          min={setting.min}
          onBlur={(event) => finishRange(Number(event.currentTarget.value))}
          onChange={(event) => previewRange(Number(event.currentTarget.value))}
          onKeyDown={(event) => {
            if (rangeAdjustmentKeys.has(event.key)) beginRange();
          }}
          onKeyUp={(event) => {
            if (rangeAdjustmentKeys.has(event.key)) finishRange(Number(event.currentTarget.value));
          }}
          onPointerCancel={(event) => finishRange(Number(event.currentTarget.value))}
          onPointerDown={beginRange}
          onPointerUp={(event) => finishRange(Number(event.currentTarget.value))}
          step={step}
          type="range"
          value={value}
        />
      )}
      {setting.control === "range" && exactValueInput}
      {guided && setting.control === "range" && endpoints && (
        <div aria-hidden="true" className="guided-scale-words"><span>{endpoints.low}</span><span>{endpoints.high}</span></div>
      )}
      {!guided && (
        <div className="setting-actions">
          <button className="icon-button" aria-label={t("profiles.revertSetting", { setting: settingLabel })} disabled={value === savedValue} onClick={() => onCommit(savedValue)} title={t("profiles.revertSetting", { setting: settingLabel })} type="button">
            <Undo2 aria-hidden="true" size={14} />
          </button>
          <button className="icon-button" aria-label={t("profiles.restoreDefault", { setting: settingLabel })} disabled={value === setting.factory} onClick={() => onCommit(setting.factory)} title={t("profiles.restoreDefault", { setting: settingLabel })} type="button">
            <RotateCcw aria-hidden="true" size={14} />
          </button>
        </div>
      )}
    </div>
  );
}

function SchemaSettings({ settings, values, savedValues, dirtyKeys, onChange, onPreviewChange, t, variant = "compact", endpointWords }: SchemaSettingsProps) {
  const guided = variant === "guided";
  return settings.map((setting) => {
    const value = values[setting.id] ?? setting.default;
    const savedValue = savedValues?.[setting.id] ?? value;
    const dirty = dirtyKeys.includes(setting.id);
    const settingLabel = t(settingMessageKey(setting.id, "label"));
    const settingDescription = t(settingMessageKey(setting.id, "description"));
    const hintId = `${setting.id}-hint`;
    if (guided) {
      return (
        <div className="setting-row guided-row" data-control={setting.control} key={setting.id}>
          <div className="setting-label guided-label">
            <label htmlFor={setting.id}>{settingLabel}</label>
            {dirty && <span className="dirty-mark">{t("profiles.changed")}</span>}
            <p id={hintId}>{settingDescription}</p>
          </div>
          <SettingControl endpoints={endpointWords?.(setting.id) ?? null} guided hintId={hintId} savedValue={savedValue} setting={setting} settingLabel={settingLabel} t={t} value={value} onCommit={(nextValue) => onChange(setting.id, nextValue)} onPreview={(nextValue) => onPreviewChange(setting.id, nextValue)} />
        </div>
      );
    }
    return (
      <div className="setting-row" key={setting.id}>
        <div className="setting-label">
          <Hint
            content={<>
              {settingDescription}
              <span className="hint-meta">
                {t("profiles.settingMeta", { default: setting.factory, min: setting.min, max: setting.max })}
                {setting.apply === "restart_required" ? ` · ${t("profiles.restartRequired")}` : ""}
              </span>
            </>}
            contentId={hintId}
          >
            <label htmlFor={setting.id}>{settingLabel}</label>
          </Hint>
          {dirty && <span className="dirty-mark">{t("profiles.changed")}</span>}
        </div>
        <SettingControl hintId={hintId} savedValue={savedValue} setting={setting} settingLabel={settingLabel} t={t} value={value} onCommit={(nextValue) => onChange(setting.id, nextValue)} onPreview={(nextValue) => onPreviewChange(setting.id, nextValue)} />
      </div>
    );
  });
}

export function BasicSettings(props: SchemaSettingsProps) {
  return <SchemaSettings {...props} />;
}

export function ShapeSettings(props: SchemaSettingsProps) {
  return <SchemaSettings {...props} />;
}

export function LcdSettings(props: SchemaSettingsProps) {
  return <SchemaSettings {...props} />;
}

export function SearchSettings(props: SchemaSettingsProps) {
  return <SchemaSettings {...props} />;
}

export { SchemaSettings };
