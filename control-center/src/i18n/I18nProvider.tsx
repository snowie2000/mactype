import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  catalogs,
  formatMessage,
  I18nContext,
  initialLocale,
  localeStorageKey,
  type Locale,
  type MessageKey,
  type Variables,
} from "./i18n";
import { setApplicationLocale } from "../app/tauri";

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(initialLocale);
  const setLocale = useCallback((next: Locale) => {
    window.localStorage.setItem(localeStorageKey, next);
    setLocaleState(next);
  }, []);
  const t = useCallback((key: MessageKey, variables?: Variables) => formatMessage(catalogs[locale][key], variables), [locale]);

  useEffect(() => {
    document.documentElement.lang = locale;
    document.documentElement.dir = locale === "ar" ? "rtl" : "ltr";
    document.body.dataset.locale = locale;
    document.title = t("app.title");
    void setApplicationLocale(locale).catch(() => undefined);
  }, [locale, t]);

  const value = useMemo(() => ({ locale, setLocale, t }), [locale, setLocale, t]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}
