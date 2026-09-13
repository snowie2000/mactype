import type { ThemePreference } from "../../app/themePreference";

// Window colours and metrics from DESIGN.md.
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

type Palette = Omit<NativePreviewChrome, "skin" | "radius" | "controlHeight" | "toolbarHeight" | "statusHeight" | "canvasRadius" | "canvasInset" | "monoStatus">;

const palettes: Record<"classic", Record<ThemePreference, Palette>> = {
  classic: {
    light: { canvas: "#F3F5F7", surface: "#FFFFFF", surfaceSubtle: "#E9EDF1", border: "#C9D1D8", text: "#17212B", muted: "#5A6773", accent: "#0067C0", onAccent: "#FFFFFF" },
    dark: { canvas: "#11161B", surface: "#192027", surfaceSubtle: "#222B33", border: "#34414C", text: "#E8EDF2", muted: "#9AA8B5", accent: "#4CA6E8", onAccent: "#07131C" },
  },
};

const metrics: Record<"classic", Pick<NativePreviewChrome, "radius" | "controlHeight" | "toolbarHeight" | "statusHeight" | "canvasRadius" | "canvasInset" | "monoStatus">> = {
  classic: { radius: 4, controlHeight: 32, toolbarHeight: 44, statusHeight: 28, canvasRadius: 4, canvasInset: 18, monoStatus: false },
};

export function nativeChrome(theme: ThemePreference): NativePreviewChrome {
  return { skin: "classic", ...palettes.classic[theme], ...metrics.classic };
}
