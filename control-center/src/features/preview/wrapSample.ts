const SAMPLE_INSET = 18;
/* Browser metrics and the helper's GDI metrics disagree by a little, so the
   last word keeps a margin rather than risking the clip. */
const SAMPLE_RIGHT_MARGIN = 12;
/* Latin breaks at spaces; CJK writes without them, so every syllable or
   ideograph is its own break opportunity. */
const CJK = "\\u1100-\\u11FF\\u2E80-\\u303F\\u3040-\\u30FF\\u3400-\\u4DBF\\u4E00-\\u9FFF\\uA960-\\uA97F\\uAC00-\\uD7FF\\uF900-\\uFAFF\\uFF00-\\uFF60";
const SAMPLE_TOKENS = new RegExp(`[${CJK}]\\s*|[^\\s${CJK}]+\\s*|\\s+`, "gu");

let sampleMeasureContext: CanvasRenderingContext2D | null | undefined;

function sampleMeasurer(fontFace: string, fontSizePt: number): ((text: string) => number) | null {
  if (sampleMeasureContext === undefined) sampleMeasureContext = document.createElement("canvas").getContext("2d");
  const context = sampleMeasureContext;
  if (!context) return null;
  context.font = `${(fontSizePt * 96) / 72}px "${fontFace}", sans-serif`;
  return (text: string) => context.measureText(text).width;
}

function wrapSampleLine(line: string, measure: (text: string) => number, room: number): ReadonlyArray<string> {
  const tokens = line.match(SAMPLE_TOKENS);
  if (!tokens) return [line];
  const wrapped: string[] = [];
  let current = "";
  for (const token of tokens) {
    if (current && measure((current + token).trimEnd()) > room) {
      wrapped.push(current.trimEnd());
      current = token.trimStart();
    } else {
      current += token;
    }
  }
  wrapped.push(current.trimEnd());
  return wrapped;
}

export function wrapSample(text: string, fontFace: string, fontSizePt: number, widthPx: number): string {
  const room = widthPx - SAMPLE_INSET - SAMPLE_RIGHT_MARGIN;
  const measure = room > 0 ? sampleMeasurer(fontFace, fontSizePt) : null;
  if (!measure) return text;
  return text.split("\n").flatMap((line) => wrapSampleLine(line, measure, room)).join("\n");
}
