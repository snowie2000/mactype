import { useCallback, useEffect, useRef, useState } from "react";
import type { PreviewEngine, PreviewResult } from "../../app/model";
import { renderProfilePreview } from "../../app/tauri";
import { preparePreviewImage } from "./preparePreviewImage";

export interface SpecimenRequest {
  key: string;
  profilePath: string;
  overrides: Readonly<Record<string, number>>;
  engine?: PreviewEngine;
  text: string;
  fontFace: string;
  fontSizePt: number;
  widthPx: number;
  heightPx: number;
  dpi: number;
  foreground: string;
  background: string;
  bold?: boolean;
  italic?: boolean;
}

export interface SpecimenLine {
  key: string;
  request: SpecimenRequest;
  result: PreviewResult;
}

export interface SpecimenRenders {
  lines: ReadonlyArray<SpecimenLine>;
  error: string | null;
  rendering: boolean;
}

/* The helper rejects bitmaps below 64 device pixels and above 2048. */
const MIN_HEIGHT = 64;
const MAX_HEIGHT = 2048;
const MAX_WIDTH = 4096;

export function specimenStripHeight(text: string, fontSizePt: number, dpi: number): number {
  const lines = Math.max(1, text.split("\n").length);
  const pixelSize = Math.max(8, Math.round((fontSizePt * dpi) / 72));
  // Reserve the helper's top inset and 1.5-em line advance, including descenders.
  const inset = Math.max(8, Math.ceil(pixelSize * 0.75));
  const spacing = Math.max(22, Math.ceil(pixelSize * 1.5));
  const bottom = Math.max(8, Math.ceil(pixelSize * 0.25));
  return Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, inset + lines * spacing + bottom));
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

/* Renders a batch of specimen strips through the helper, one after another,
   and publishes the batch only when every strip of the newest request set is
   ready. A newer batch abandons the running one, and the last complete batch
   stays visible meanwhile, so a board never flashes half-rendered. */
export function useSpecimenRenders(requests: ReadonlyArray<SpecimenRequest>, enabled = true): SpecimenRenders {
  const [lines, setLines] = useState<ReadonlyArray<SpecimenLine>>([]);
  const [error, setError] = useState<string | null>(null);
  const [rendering, setRendering] = useState(false);
  const batchCounter = useRef(0);
  const running = useRef(false);
  const pending = useRef<{ id: number; requests: ReadonlyArray<SpecimenRequest> } | null>(null);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      pending.current = null;
    };
  }, []);

  const drain = useCallback(async () => {
    if (running.current) return;
    running.current = true;
    setRendering(true);
    try {
      while (pending.current) {
        const batch = pending.current;
        pending.current = null;
        const rendered: SpecimenLine[] = [];
        let aborted = false;
        for (const request of batch.requests) {
          try {
            const result = await renderProfilePreview({
              profilePath: request.profilePath,
              overrides: { ...request.overrides },
              displayScale: request.dpi / 96,
              engine: request.engine,
              sample: {
                text: request.text,
                fontFace: request.fontFace,
                fontSizePt: request.fontSizePt,
                widthPx: clamp(Math.round(request.widthPx), MIN_HEIGHT, MAX_WIDTH),
                heightPx: clamp(Math.round(request.heightPx), MIN_HEIGHT, MAX_HEIGHT),
                dpi: request.dpi,
                foreground: request.foreground,
                background: request.background,
                bold: request.bold ?? false,
                italic: request.italic ?? false,
              },
            });
            if (!mounted.current) return;
            if (!result) throw new Error("Preview renderer returned no image");
            await preparePreviewImage(result.imagePath);
            if (!mounted.current) return;
            rendered.push({ key: request.key, request, result });
            if (batch.id < batchCounter.current) {
              aborted = true;
              break;
            }
          } catch (caught: unknown) {
            if (mounted.current) setError(caught instanceof Error ? caught.message : String(caught));
            aborted = true;
            break;
          }
        }
        if (aborted || batch.id < batchCounter.current) continue;
        setLines(rendered);
        setError(null);
      }
    } finally {
      running.current = false;
      if (mounted.current) setRendering(false);
    }
  }, []);

  const signature = JSON.stringify(requests);
  useEffect(() => {
    if (!enabled) return;
    if (requests.length === 0) {
      batchCounter.current += 1;
      pending.current = null;
      setLines([]);
      return;
    }
    pending.current = { id: ++batchCounter.current, requests };
    void drain();
    // The signature captures every field of every request.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [drain, enabled, signature]);

  return { lines, error, rendering };
}
