import { createContext, useContext } from "react";
import ar from "./ar.json";
import de from "./de.json";
import en from "./en.json";
import es from "./es.json";
import fr from "./fr.json";
import ja from "./ja.json";
import ko from "./ko.json";
import pt from "./pt.json";
import zhCn from "./zh-CN.json";
import zhTw from "./zh-TW.json";

export const localeOptions = [
  { value: "ko", labelKey: "app.languageKo" },
  { value: "en", labelKey: "app.languageEn" },
  { value: "zh-CN", labelKey: "app.languageZhCn" },
  { value: "zh-TW", labelKey: "app.languageZhTw" },
  { value: "ja", labelKey: "app.languageJa" },
  { value: "fr", labelKey: "app.languageFr" },
  { value: "de", labelKey: "app.languageDe" },
  { value: "es", labelKey: "app.languageEs" },
  { value: "pt", labelKey: "app.languagePt" },
  { value: "ar", labelKey: "app.languageAr" },
] as const;

export type Locale = (typeof localeOptions)[number]["value"];
export type MessageKey = keyof typeof ko;
export type Variables = Record<string, string | number>;

export const catalogs: Record<Locale, Record<MessageKey, string>> = {
  ko,
  en,
  "zh-CN": zhCn,
  "zh-TW": zhTw,
  ja,
  fr,
  de,
  es,
  pt,
  ar,
};
export const localeStorageKey = "mactype-control-center.locale";

export function isLocale(value: string | null): value is Locale {
  return localeOptions.some((option) => option.value === value);
}

export function localeFromNavigator(language: string): Locale {
  const normalized = language.toLowerCase();
  if (normalized.startsWith("ko")) return "ko";
  if (normalized.startsWith("zh-hant") || /^zh-(tw|hk|mo)/u.test(normalized)) return "zh-TW";
  if (normalized.startsWith("zh")) return "zh-CN";
  const base = normalized.split("-")[0];
  return isLocale(base) ? base : "en";
}

export function initialLocale(): Locale {
  const queryLocale = new URLSearchParams(window.location.search).get("lang");
  if (isLocale(queryLocale)) {
    window.localStorage.setItem(localeStorageKey, queryLocale);
    return queryLocale;
  }
  const stored = window.localStorage.getItem(localeStorageKey);
  if (isLocale(stored)) return stored;
  return localeFromNavigator(navigator.language);
}

export function formatMessage(message: string, variables?: Variables): string {
  if (!variables) return message;
  return message.replace(/\{(\w+)\}/g, (match, key: string) => String(variables[key] ?? match));
}

export interface I18nValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: MessageKey, variables?: Variables) => string;
}

export const I18nContext = createContext<I18nValue | null>(null);

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n must be used within I18nProvider");
  return value;
}

export function settingMessageKey(settingId: string, field: "label" | "description"): MessageKey {
  return `settings.${settingId}.${field}` as MessageKey;
}

export function settingOptionMessageKey(settingId: string, value: number): MessageKey {
  return `settings.${settingId}.option.${value}` as MessageKey;
}
