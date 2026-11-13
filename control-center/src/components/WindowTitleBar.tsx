import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ReactNode } from "react";
import { useI18n } from "../i18n/i18n";

function withTauriWindow(action: (window: ReturnType<typeof getCurrentWindow>) => Promise<void>) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  void action(getCurrentWindow());
}

interface WindowTitleBarProps {
  /* Replaces the product name in standalone windows. */
  title?: ReactNode;
  icon?: boolean;
  className?: string;
}

export function WindowTitleBar({ title, icon = true, className }: WindowTitleBarProps = {}) {
  const { t } = useI18n();

  return (
    <header className={className ? `window-titlebar ${className}` : "window-titlebar"} data-tauri-drag-region>
      <div className="window-title" data-tauri-drag-region onDoubleClick={() => withTauriWindow((appWindow) => appWindow.toggleMaximize())}>
        {icon && <img alt="" data-tauri-drag-region height="18" src="/mactype-icon.png" width="18" />}
        <span data-tauri-drag-region>{title ?? t("app.title")}</span>
      </div>
      <div className="window-controls">
        <button aria-label={t("app.windowMinimize")} onClick={() => withTauriWindow((appWindow) => appWindow.minimize())} type="button">
          <Minus aria-hidden="true" size={16} strokeWidth={1.7} />
        </button>
        <button aria-label={t("app.windowMaximize")} onClick={() => withTauriWindow((appWindow) => appWindow.toggleMaximize())} type="button">
          <Square aria-hidden="true" size={13} strokeWidth={1.5} />
        </button>
        <button aria-label={t("app.windowClose")} className="window-close" onClick={() => withTauriWindow((appWindow) => appWindow.close())} type="button">
          <X aria-hidden="true" size={17} strokeWidth={1.7} />
        </button>
      </div>
    </header>
  );
}
