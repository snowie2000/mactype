import { useEffect, useRef, useState, type ReactNode } from "react";

interface HintProps {
  children: ReactNode;
  content: ReactNode;
  contentId?: string;
}

const SHOW_DELAY_MS = 120;
const VIEWPORT_MARGIN = 8;
const ANCHOR_GAP = 6;
const MAX_WIDTH = 360;

export function Hint({ children, content, contentId }: HintProps) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const popoverRef = useRef<HTMLSpanElement>(null);
  const showTimer = useRef<number | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => () => {
    if (showTimer.current !== null) window.clearTimeout(showTimer.current);
  }, []);

  useEffect(() => {
    if (!open) return undefined;
    const hideOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", hideOnEscape);
    return () => window.removeEventListener("keydown", hideOnEscape);
  }, [open]);

  const show = () => {
    const anchor = anchorRef.current;
    const popover = popoverRef.current;
    if (!anchor || !popover) return;
    const rect = anchor.getBoundingClientRect();
    const width = Math.min(MAX_WIDTH, window.innerWidth - 2 * VIEWPORT_MARGIN);
    popover.style.maxWidth = `${width}px`;
    const height = popover.offsetHeight;
    const below = rect.bottom + ANCHOR_GAP;
    const above = rect.top - ANCHOR_GAP - height;
    const top = below + height <= window.innerHeight - VIEWPORT_MARGIN || above < VIEWPORT_MARGIN ? below : above;
    const left = Math.max(VIEWPORT_MARGIN, Math.min(rect.left, window.innerWidth - Math.min(width, popover.offsetWidth) - VIEWPORT_MARGIN));
    popover.style.top = `${Math.round(top)}px`;
    popover.style.left = `${Math.round(left)}px`;
    setOpen(true);
  };

  const scheduleShow = () => {
    if (showTimer.current !== null) window.clearTimeout(showTimer.current);
    showTimer.current = window.setTimeout(show, SHOW_DELAY_MS);
  };

  const hide = () => {
    if (showTimer.current !== null) {
      window.clearTimeout(showTimer.current);
      showTimer.current = null;
    }
    setOpen(false);
  };

  return (
    <span className="hint" onMouseEnter={scheduleShow} onMouseLeave={hide} ref={anchorRef}>
      <span className="hint-term">{children}</span>
      <span className="hint-popover" data-open={open ? "true" : undefined} id={contentId} ref={popoverRef} role="tooltip">{content}</span>
    </span>
  );
}
