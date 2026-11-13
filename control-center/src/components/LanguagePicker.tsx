import { Languages } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { localeOptions, useI18n, type Locale } from "../i18n/i18n";

export function LanguagePicker() {
  const { locale, setLocale, t } = useI18n();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const selected = localeOptions.find((option) => option.value === locale) ?? localeOptions[0];

  useEffect(() => {
    if (!open) return undefined;
    const closeOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      triggerRef.current?.focus();
    };
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  const choose = (next: Locale) => {
    setLocale(next);
    setOpen(false);
    triggerRef.current?.focus();
  };

  return (
    <div className="language-control" ref={rootRef}>
      <Languages aria-hidden="true" size={17} />
      <div className="language-picker">
        <button
          aria-expanded={open}
          aria-haspopup="listbox"
          aria-label={t("app.language")}
          className="language-picker-trigger"
          data-testid="language-picker-trigger"
          onClick={() => setOpen((current) => !current)}
          ref={triggerRef}
          type="button"
        >
          {t(selected.labelKey)}
        </button>
        {open && (
          <div aria-label={t("app.language")} className="language-menu" role="listbox">
            {localeOptions.map((option) => (
              <button
                aria-selected={option.value === locale}
                className="language-option"
                data-locale-option={option.value}
                key={option.value}
                onClick={() => choose(option.value)}
                role="option"
                type="button"
              >
                {t(option.labelKey)}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
