import type { I18nValue } from "../../i18n/i18n";
import {
  nativePreviewLadderSizes,
  type NativePreviewLabelKey,
} from "../../generated/nativePreview";

export const NATIVE_LADDER_SIZES: ReadonlyArray<number> = nativePreviewLadderSizes;

export function nativePreviewLabels(t: I18nValue["t"]): Record<NativePreviewLabelKey, string> {
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
