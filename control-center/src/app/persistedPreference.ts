/* One policy for every persisted UI preference: a valid explicit query value
   wins and is persisted, like ?lang=, so the browser gallery can render each
   locale and theme directly; then the stored value; then the owner's
   fallback. Storage may be absent or throw, and a failed write never changes
   the selection for this session. */
export function readPreference<T>({ key, query, parse, fallback }: {
  key: string;
  query?: string;
  parse: (raw: string) => T | null;
  fallback: () => T;
}): T {
  const requested = query === undefined ? null : new URLSearchParams(window.location.search).get(query);
  if (requested !== null) {
    const selected = parse(requested);
    if (selected !== null) {
      writePreference(key, requested);
      return selected;
    }
  }

  let stored: string | null = null;
  try {
    stored = window.localStorage.getItem(key);
  } catch {
    // Storage can be unavailable even when the window exists.
  }
  if (stored !== null) {
    const selected = parse(stored);
    if (selected !== null) return selected;
  }
  return fallback();
}

export function writePreference(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Persistence failure must not change the selection for this session.
  }
}
