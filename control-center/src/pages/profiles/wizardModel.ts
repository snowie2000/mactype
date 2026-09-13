export type WizardStepId = "start" | "rendering" | "quality" | "boldItalic" | "hinting" | "gamma" | "lcd" | "substitution" | "apply";

export const wizardStepIds: ReadonlyArray<WizardStepId> = ["start", "rendering", "quality", "boldItalic", "hinting", "gamma", "lcd", "substitution", "apply"];

/* Step contents follow the legacy MacType Tuner screens: bold and italic share
   one screen, contrast lives next to the gamma slider, and the RGB text tuning
   joins the LCD layout screen. Ids are listed in on-screen order. */
export const wizardSettingIdsByStep: Readonly<Record<WizardStepId, ReadonlyArray<string>>> = {
  start: [],
  rendering: ["anti_alias_mode"],
  quality: ["normal_weight", "render_weight", "enable_kerning"],
  boldItalic: ["bold_weight", "bolder_mode", "italic_slant"],
  hinting: ["hinting_mode", "hint_small_font"],
  gamma: ["contrast", "gamma_value", "gamma_mode"],
  lcd: ["lcd_filter", "text_tuning", "text_tuning_r", "text_tuning_g", "text_tuning_b"],
  substitution: ["font_substitutes"],
  apply: [],
};

export const wizardSettingIds = [...new Set(Object.values(wizardSettingIdsByStep).flat())];

/* The substitution step owns one schema setting, but its substance is the
   mapping list, which carries no saved snapshot. Undo and discard there would
   restore half the step, so it opts out of step history entirely — the tools
   and the keyboard shortcuts stay inert together. */
export function stepSupportsHistory(step: WizardStepId): boolean {
  return wizardSettingIdsByStep[step].length > 0 && step !== "substitution";
}

export type WizardScaleId = "weight" | "contrast" | "gamma";

/* Guided sliders speak in outcomes, not numbers, following the legacy
   MacType Tuner endpoints (Thin↔Thick, Low↔High, Dark↔Light). */
export const wizardScaleBySettingId: Readonly<Record<string, WizardScaleId>> = {
  normal_weight: "weight",
  bold_weight: "weight",
  render_weight: "weight",
  contrast: "contrast",
  gamma_value: "gamma",
};
