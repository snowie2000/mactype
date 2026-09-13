import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { loadThemePreference } from "./app/themePreference";
import { I18nProvider } from "./i18n/I18nProvider";
import "./styles/tokens.css";
import "./styles/app.css";

const root = document.getElementById("root");
const initialTheme = loadThemePreference();

document.documentElement.dataset.theme = initialTheme;

if (!root) {
  throw new Error("Control Center root element is missing");
}

createRoot(root).render(
  <StrictMode>
    <I18nProvider><App initialTheme={initialTheme} /></I18nProvider>
  </StrictMode>,
);
