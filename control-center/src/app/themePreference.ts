export type ThemePreference = "light" | "dark";

export const themeStorageKey = "mactype-control-center.theme";

/* An explicit ?theme= wins and is persisted, like ?lang= and ?skin=, so the
   browser gallery can render both themes of every skin directly. */
export function loadThemePreference(): ThemePreference {
  try {
    const requested = new URLSearchParams(window.location.search).get("theme");
    if (requested === "dark" || requested === "light") {
      window.localStorage.setItem(themeStorageKey, requested);
      return requested;
    }
    const stored = window.localStorage.getItem(themeStorageKey);
    return stored === "dark" || stored === "light" ? stored : "light";
  } catch {
    return "light";
  }
}

export function applyThemePreference(theme: ThemePreference) {
  document.documentElement.dataset.theme = theme;
  try {
    window.localStorage.setItem(themeStorageKey, theme);
  } catch {
    // The selected theme still applies for this session when storage is unavailable.
  }
}
