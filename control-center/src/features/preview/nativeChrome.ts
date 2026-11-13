import type { ThemePreference } from "../../app/themePreference";
import { nativePreviewChromeDefaults } from "../../generated/nativePreview";

export interface NativePreviewChrome {
  skin: "classic";
  canvas: string;
  surface: string;
  surfaceSubtle: string;
  border: string;
  text: string;
  muted: string;
  accent: string;
  onAccent: string;
  radius: number;
  controlHeight: number;
  toolbarHeight: number;
  statusHeight: number;
  canvasRadius: number;
  canvasInset: number;
  monoStatus: boolean;
}

export function nativeChrome(theme: ThemePreference): NativePreviewChrome {
  return {
    skin: nativePreviewChromeDefaults.skin,
    ...nativePreviewChromeDefaults.palettes[theme],
    ...nativePreviewChromeDefaults.metrics,
  };
}
