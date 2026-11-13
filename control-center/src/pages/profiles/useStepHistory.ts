import { useRef, useState } from "react";
import type { GuidedStepId } from "./guidedModel";

export interface StepHistoryEntry {
  settingId: string;
  before: number;
  after: number;
}

interface StepStacks {
  undo: StepHistoryEntry[];
  redo: StepHistoryEntry[];
}

export interface StepHistory {
  canUndo: (step: GuidedStepId) => boolean;
  canRedo: (step: GuidedStepId) => boolean;
  record: (step: GuidedStepId, settingId: string, after: number, committedFallback: number) => void;
  undo: (step: GuidedStepId) => StepHistoryEntry | null;
  redo: (step: GuidedStepId) => StepHistoryEntry | null;
}

/* Guided mode scopes undo, redo, and discard to the current guided step. The
   backend document history stays global and untouched; this frontend history
   records each committed guided edit under the step it happened on, so one
   step's undo never replays another step's changes. It clears whenever the
   profile document or the editing mode changes. */
export function useStepHistory(resetKey: string): StepHistory {
  const stacks = useRef(new Map<GuidedStepId, StepStacks>());
  /* Last committed value per setting. It seeds `before` for the next entry
     even while earlier commits are still in flight in the mutation queue. */
  const committed = useRef(new Map<string, number>());
  const lastResetKey = useRef(resetKey);
  const [, setRevision] = useState(0);
  const bump = () => setRevision((current) => current + 1);

  if (lastResetKey.current !== resetKey) {
    lastResetKey.current = resetKey;
    stacks.current.clear();
    committed.current.clear();
  }

  const stepStacks = (step: GuidedStepId): StepStacks => {
    const existing = stacks.current.get(step);
    if (existing) return existing;
    const created: StepStacks = { undo: [], redo: [] };
    stacks.current.set(step, created);
    return created;
  };

  const record = (step: GuidedStepId, settingId: string, after: number, committedFallback: number) => {
    const before = committed.current.get(settingId) ?? committedFallback;
    committed.current.set(settingId, after);
    if (before === after) return;
    const stack = stepStacks(step);
    stack.undo.push({ settingId, before, after });
    stack.redo.length = 0;
    bump();
  };

  const undo = (step: GuidedStepId): StepHistoryEntry | null => {
    const stack = stacks.current.get(step);
    const entry = stack?.undo.pop();
    if (!stack || !entry) return null;
    stack.redo.push(entry);
    committed.current.set(entry.settingId, entry.before);
    bump();
    return entry;
  };

  const redo = (step: GuidedStepId): StepHistoryEntry | null => {
    const stack = stacks.current.get(step);
    const entry = stack?.redo.pop();
    if (!stack || !entry) return null;
    stack.undo.push(entry);
    committed.current.set(entry.settingId, entry.after);
    bump();
    return entry;
  };

  return {
    canUndo: (step) => (stacks.current.get(step)?.undo.length ?? 0) > 0,
    canRedo: (step) => (stacks.current.get(step)?.redo.length ?? 0) > 0,
    record,
    undo,
    redo,
  };
}
