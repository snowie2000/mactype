import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { ProfileNameVerdict } from "../features/profiles/useProfileDocument";
import { useI18n } from "../i18n/i18n";

interface ProfileNameDialogProps {
  busy: boolean;
  initialName: string;
  onCancel: () => void;
  onSubmit: (name: string) => void;
  verdict: (candidate: string) => ProfileNameVerdict;
}

export function ProfileNameDialog({ busy, initialName, onCancel, onSubmit, verdict }: ProfileNameDialogProps) {
  const { t } = useI18n();
  const [name, setName] = useState(initialName);
  const inputRef = useRef<HTMLInputElement>(null);
  const nameVerdict = verdict(name);
  const invalid = nameVerdict !== "ok";

  useEffect(() => {
    const trigger = document.activeElement;
    inputRef.current?.focus();
    inputRef.current?.select();
    return () => {
      if (trigger instanceof HTMLElement) trigger.focus();
    };
  }, []);

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = [...event.currentTarget.querySelectorAll<HTMLInputElement | HTMLButtonElement>("input:not(:disabled), button:not(:disabled)")];
    const first = focusable[0];
    const last = focusable.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <div className="confirmation-backdrop">
      <section aria-labelledby="profile-name-dialog-title" aria-modal="true" className="profile-name-dialog" onKeyDown={handleKeyDown} role="dialog">
        <h2 id="profile-name-dialog-title">{t("files.saveAs")}</h2>
        <p>{t("profiles.saveAsHint")}</p>
        <form onSubmit={(event) => {
          event.preventDefault();
          if (!busy && !invalid) onSubmit(name);
        }}>
          <label>
            <span>{t("profiles.saveAsName")}</span>
            <input aria-describedby={invalid ? "profile-name-dialog-message" : undefined} aria-invalid={invalid} onChange={(event) => setName(event.target.value)} readOnly={busy} ref={inputRef} type="text" value={name} />
          </label>
          <p aria-live="polite" className="profile-name-dialog-message" id="profile-name-dialog-message">{nameVerdict === "taken" ? t("profiles.saveAsTaken") : nameVerdict === "reserved-character" ? t("profiles.saveAsReserved") : null}</p>
          <div className="profile-name-dialog-actions">
            <button className="button secondary" onClick={onCancel} type="button">{t("common.cancel")}</button>
            <button className="button primary" disabled={invalid || busy} type="submit">{busy ? t("profiles.saving") : t("profiles.save")}</button>
          </div>
        </form>
      </section>
    </div>
  );
}
