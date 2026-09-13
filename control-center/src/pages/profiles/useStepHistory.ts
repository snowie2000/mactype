import { useRef, useState } from "react";
import type { WizardStepId } from "./wizardModel";

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
  canUndo: (step: WizardStepId) => boolean;
  canRedo: (step: WizardStepId) => boolean;
  record: (step: WizardStepId, settingId: string, after: number, committedFallback: number) => void;
  undo: (step: WizardStepId) => StepHistoryEntry | null;
  redo: (step: WizardStepId) => StepHistoryEntry | null;
}

/* Guided mode scopes undo, redo, and discard to the current wizard step. The
   backend document history stays global and untouched; this frontend history
   records each committed guided edit under the step it happened on, so one
   step's undo never replays another step's changes. It clears whenever the
   profile document or the editing mode changes. */
export function useStepHistory(resetKey: string): StepHistory {
  const stacks = useRef(new Map<WizardStepId, StepStacks>());
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

  const stepStacks = (step: WizardStepId): StepStacks => {
    const existing = stacks.current.get(step);
    if (existing) return existing;
    const created: StepStacks = { undo: [], redo: [] };
    stacks.current.set(step, created);
    return created;
  };

  const record = (step: WizardStepId, settingId: string, after: number, committedFallback: number) => {
    const before = committed.current.get(settingId) ?? committedFallback;
    committed.current.set(settingId, after);
    if (before === after) return;
    const stack = stepStacks(step);
    stack.undo.push({ settingId, before, after });
    stack.redo.length = 0;
    bump();
  };

  const undo = (step: WizardStepId): StepHistoryEntry | null => {
    const stack = stacks.current.get(step);
    const entry = stack?.undo.pop();
    if (!stack || !entry) return null;
    stack.redo.push(entry);
    committed.current.set(entry.settingId, entry.before);
    bump();
    return entry;
  };

  const redo = (step: WizardStepId): StepHistoryEntry | null => {
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
