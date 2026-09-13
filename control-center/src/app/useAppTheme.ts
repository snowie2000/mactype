import { useEffect, useState } from "react";
import type { ThemePreference } from "./themePreference";

function readTheme(): ThemePreference {
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}

/* The applied theme as `html[data-theme]` carries it, kept current when the
   user toggles it. Preview canvases read this so their default colours match
   the window they sit in. */
export function useAppTheme(): ThemePreference {
  const [theme, setTheme] = useState<ThemePreference>(readTheme);
  useEffect(() => {
    const observer = new MutationObserver(() => setTheme(readTheme()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => observer.disconnect();
  }, []);
  return theme;
}
