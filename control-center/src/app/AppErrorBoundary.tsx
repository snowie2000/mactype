import { Component, useEffect, useRef, type ReactNode } from "react";
import { WindowTitleBar } from "../components/WindowTitleBar";
import { useI18n } from "../i18n/i18n";
import { runtime } from "./runtimeAdapter";

interface AppErrorBoundaryProps {
  children: ReactNode;
}

interface AppErrorBoundaryState {
  error: Error | null;
}

function asError(value: unknown): Error {
  return value instanceof Error ? value : new Error(String(value));
}

function errorDetail(error: Error): string {
  return error.message || error.name || String(error);
}

/* A fatal startup or render error replaces the blank window with a
   recovery card. The window has no native frame, so the title bar stays
   to keep minimise and close reachable. */
export class AppErrorBoundary extends Component<AppErrorBoundaryProps, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: unknown): AppErrorBoundaryState {
    return { error: asError(error) };
  }

  componentDidCatch(error: unknown): void {
    try {
      void runtime().reportFrontendFailure("overview", errorDetail(asError(error))).catch(() => undefined);
    } catch {
      /* Telemetry is best effort; the recovery card must still render. */
    }
  }

  render(): ReactNode {
    const { error } = this.state;
    return error ? <AppRecovery error={error} /> : this.props.children;
  }
}

export function AppRecovery({ error }: { error: Error }) {
  const { t } = useI18n();
  const titleRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  return (
    <>
      <WindowTitleBar />
      <main className="work-area" data-testid="app-recovery" id="main-content">
        <section aria-describedby="app-recovery-body" aria-labelledby="app-recovery-title" className="page view-enter" role="alert">
          <header className="page-header compact">
            <div>
              <h1 id="app-recovery-title" ref={titleRef} tabIndex={-1}>{t("recovery.title")}</h1>
              <p id="app-recovery-body">{t("recovery.body")}</p>
            </div>
            <div className="header-actions">
              <button className="button primary" onClick={() => window.location.reload()} type="button">{t("recovery.reload")}</button>
            </div>
          </header>
          <section aria-labelledby="app-recovery-detail" className="section-block">
            <div className="section-heading"><h2 id="app-recovery-detail">{t("recovery.detail")}</h2></div>
            <div style={{ padding: "var(--space-4)" }}>
              <pre className="event-detail-text">{errorDetail(error)}</pre>
            </div>
          </section>
        </section>
      </main>
    </>
  );
}
