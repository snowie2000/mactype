import type { I18nValue } from "../../i18n/i18n";

/* The native preview window draws its own chrome, so the Control Center
   hands it every string in the reader's language; the window never carries
   text of its own. Keys mirror the helper's `labels` object. */
export const NATIVE_LADDER_SIZES: ReadonlyArray<number> = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24];

export function nativePreviewLabels(t: I18nValue["t"]): Record<string, string> {
  return {
    title: t("native.title"),
    fontFace: t("profiles.previewFont"),
    fontSize: t("profiles.previewSize"),
    bold: t("nativePreview.bold"),
    italic: t("nativePreview.italic"),
    modeSample: t("profiles.nativeDisplayDefault"),
    modeLadder: t("profiles.nativeDisplayLadder"),
    modeCompare: t("profiles.nativeDisplayCompare"),
    modeListing: t("profiles.nativeDisplayListing"),
    invert: t("profiles.invertColours"),
    loupe: t("native.loupe"),
    zoom: t("nativePreview.zoom"),
    topmost: t("native.topmost"),
    editText: t("profiles.editSample"),
    savePng: t("nativePreview.savePng"),
    copy: t("native.copy"),
    compareMacType: t("native.compareMacType"),
    compareWindows: t("nativePreview.compareWindows"),
    compareUnavailable: t("native.compareUnavailable"),
    engineMacType: t("native.engineMacType"),
    coreVersion: t("native.coreVersion"),
    pngFilter: t("nativePreview.pngFilter"),
    saved: t("native.saved"),
    copied: t("native.copied"),
  };
}
