export interface SpecimenPalette {
  foreground: string;
  background: string;
}

/* One palette for every helper-rendered specimen, so light and dark mean the
   same colours on every board and in the native window. */
export function specimenPalette(dark: boolean): SpecimenPalette {
  return dark
    ? { foreground: "#F1F3F5", background: "#171A1F" }
    : { foreground: "#181D23", background: "#FFFFFF" };
}
