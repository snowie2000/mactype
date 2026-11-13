import { readPreference, writePreference } from "./persistedPreference";

export type ThemePreference = "light" | "dark";

export const themeStorageKey = "mactype-control-center.theme";

export function loadThemePreference(): ThemePreference {
  return readPreference<ThemePreference>({
    key: themeStorageKey,
    query: "theme",
    parse: (raw) => raw === "dark" || raw === "light" ? raw : null,
    fallback: () => "light",
  });
}

export function applyThemePreference(theme: ThemePreference) {
  document.documentElement.dataset.theme = theme;
  writePreference(themeStorageKey, theme);
}
