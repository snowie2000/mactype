# Control Center design maintenance

This is a file map and editing checklist for maintainers. It explains where to change the interface; it is not a description of the visual concept.

## Keep design work out of Rust

Routine visual work belongs under `control-center/src` and can be previewed in a browser. Do not edit these paths for a color, spacing, typography, layout, copy, icon, or page-composition change:

- `control-center/src-tauri/**`
- `preview-helper/**`
- `shared/settings-schema.json`
- generated settings files

Those paths define native behavior, IPC, packaging, or the MacType setting model. If a proposed design change needs a new native command or new data from Windows, split that behavior into a separate change. The visual part can then consume the new TypeScript adapter method without mixing Rust into design review.

## Where to make each change

| Change | Primary file | Also check |
| --- | --- | --- |
| Colors, spacing, control height, radii, animation timing, navigation width | `control-center/src/styles/tokens.css` | Both `:root` and `:root[data-theme="dark"]` |
| Shared controls, cards, grids, page spacing, responsive behavior, RTL rules | `control-center/src/styles/app.css` | The mobile and tablet media queries at the end of the file |
| Application shell, navigation order, expandable Settings group, navigation icons, theme switch | `control-center/src/app/App.tsx` | `.nav-group`, `.nav-submenu`, and `.nav-subitem` in `app.css` |
| Custom title bar layout, icon, and window-control icons | `control-center/src/components/WindowTitleBar.tsx` | `.window-titlebar`, `.window-title`, and `.window-controls` in `app.css` |
| Language picker layout, option order, and scrollable menu | `control-center/src/components/LanguagePicker.tsx` and `control-center/src/i18n/i18n.ts` | `.language-*` selectors in `app.css`, including dark and mobile states |
| Overview status summary and recent-activity disclosure | `control-center/src/pages/OverviewPage.tsx` | `.overview-service-*`, `.recent-activity`, and `.disclosure-actions` in `app.css`; Overview interaction assertions in `tests/frontend-gallery/gallery.spec.ts` |
| Settings-file and migration layout | `control-center/src/pages/FileSettingsPage.tsx` | `.legacy-import-*` and `.file-*` selectors in `app.css` |
| Profile editor shell, Wizard/Tuner mode, quick actions, and preview placement | `control-center/src/pages/ProfilesPage.tsx` | `WizardSettings.tsx`, `wizardModel.ts`, and `.wizard-*` selectors in `app.css` |
| Basic, shape, and LCD setting rows | `control-center/src/pages/profiles/SchemaSettings.tsx` | Shared `.setting-row`, `.range-control`, and `.number-control` selectors |
| Advanced, per-font, and list editors | `control-center/src/pages/profiles/AdvancedSettings.tsx`, `IndividualSettings.tsx`, and `ListsEditor.tsx` | Font picker and collection selectors in `app.css` |
| Execution and service-control layout | `control-center/src/pages/ExecutionPage.tsx` | `control-center/src/app/executionViewModel.ts` and the `.manual-*`, `.service-*`, `.system-injection-*`, and `.system-mode-*` selectors |
| Diagnostics installation details and recent-log disclosure | `control-center/src/pages/DiagnosticsPage.tsx` | `.installation-heading`, `.diagnostic-*`, `.log-view`, and `.disclosure-actions` selectors |
| User-facing text | Every JSON catalog under `control-center/src/i18n/` | All ten catalogs must contain the same keys and placeholders |
| Locale order, language detection, RTL selection | `control-center/src/i18n/i18n.ts` and `I18nProvider.tsx` | `tests/frontend-gallery/windows.ts` |
| Browser-only sample data used during design work | `control-center/src/app/runtimeAdapters/browserGalleryAdapter.ts` and `browserGalleryExecution.ts` | Keep fixture DTOs equal to `runtimeAdapter.ts`; do not duplicate UI mutation gates here |
| In-app MacType logo | `control-center/public/mactype-icon.png` | Preserve the existing filename to avoid code changes |
| Packaged EXE and installer icon | `control-center/src-tauri/icons/icon.ico` and `assets/mactype.ico` | These are assets; no Rust source change is required |

## Common maintenance tasks

### Change the palette or density

Start in `tokens.css`. Prefer changing a semantic token instead of replacing individual hex values or pixel sizes throughout `app.css`. Update the light and dark values together, then inspect status colors, disabled controls, focus rings, and preview backgrounds.

Spacing follows the `--space-*` scale. Controls use `--control-height`; navigation and profile-editor widths have dedicated tokens. A density change should normally be possible without editing a page component.

### Change a shared control or card

Edit the shared selector in `app.css` before adding a page-specific override. The main reusable patterns are:

- `.button`, `.icon-button`, and `.text-action`
- `.section-block` and `.section-heading`
- `.detail-list`
- `.setting-row`, `.range-control`, and `.number-control`
- `.success-message`, `.inline-error`, and `.warning-text`

Keep focus-visible styling and disabled-state contrast. Do not remove visible focus just to match a screenshot.

### Change a page layout

Edit the relevant component in `control-center/src/pages` and its selectors in `app.css`. Page components should decide structure and accessibility; CSS should decide placement and presentation. Keep user-facing copy in the locale catalogs rather than embedding it in TSX.

For grid children that contain paths, translations, or font names, use `minmax(0, 1fr)` and `min-width: 0`. Test long German, Chinese, and Arabic content instead of relying on English width.

### Change the custom title bar

The visible title bar is ordinary React and CSS. Change its markup, logo, text, or Lucide icons in `WindowTitleBar.tsx`; change its height with `--titlebar-height` in `tokens.css`; and change colors, borders, hover states, or control widths in the `.window-titlebar`, `.window-title`, and `.window-controls` rules in `app.css`. None of those changes requires Rust.

Keep `data-tauri-drag-region` on the non-interactive title area so the real window remains draggable. Keep minimize, maximize/restore, and close as separate buttons with translated `aria-label` values. `WindowTitleBar.tsx` calls the narrow Tauri window API only after detecting the native runtime, so the same component remains visible and safe in browser gallery mode.

The native frame is disabled once in `control-center/src-tauri/tauri.conf.json`, and the matching window-control permissions live in `control-center/src-tauri/capabilities/default.json`. Treat those as platform wiring, not design files. Ordinary title-bar redesign must leave them alone. Only restoring the operating-system title bar or adding a new native window operation should require a separate native configuration change.

### Change the system-application status card

The visible MacType on/off card is ordinary React and CSS. Change its markup and Lucide icons in `ExecutionPage.tsx`; change its spacing, colors, responsive stacking, and 40-pixel action target in the `.system-injection-*` rules in `app.css`; and change its copy in every locale catalog. Browser gallery mode supplies both the service status and the activation result, so these visual changes do not require Rust.

`projectExecutionView` in `control-center/src/app/executionViewModel.ts` is the one pure projection for the entire Execution page. It receives only `ExecutionStatus` and the current busy operation, then derives the displayed state, copy keys, primary action, service upgrade/repair visibility, and every mutation gate. `ExecutionPage.tsx` consumes that projection in both the real Tauri application and browser gallery; the gallery adapters only supply representative status DTOs. This keeps screenshots and the installed application on the same UI decision path without introducing a frontend store.

Keep the existing adapter calls behind the buttons. `systemInjectionActive` is native truth from the generation-bound verified service state, while `systemModesSupported` decides whether activation is safe to offer. Renaming, recoloring, or rearranging the card may change TSX, CSS, locale JSON, or the pure TypeScript projection, but must not recreate `canInstall`, `canStart`, `canRepair`, migration, or primary-action gates in the component or gallery fixtures. A new service action or a different safety policy is native behavior and should be reviewed separately from the design change.

### Change the Overview or Diagnostics disclosures

`OverviewPage.tsx` intentionally shows current MacType state and recent successful activity rather than installation inventory. Its status card, four-column detail list, conditional Service shortcut, and activity disclosure are ordinary React and CSS. Change their composition in `OverviewPage.tsx`, their responsive layout in the `.overview-service-*`, `.recent-activity`, and `.disclosure-actions` rules, and their copy in the locale catalogs.

Keep the Service shortcut conditional: healthy running state has no action, while inactive or problem states offer the route to service controls. The recent-activity list is collapsed by default, contains at most five successful events, and leaves errors to Diagnostics. Those are interaction and information-hierarchy invariants covered by the browser gallery.

Installation inventory and diagnostic logs live in `DiagnosticsPage.tsx`. Change the installation card and its relocate/reconnect actions there. The recent-log viewer is also collapsed by default; keep Log folder on the left and Expand/Collapse on the right through the shared `.disclosure-actions` layout. Its visual presentation can be redesigned without Rust.

The data itself crosses the native boundary: `runtimeAdapter.ts` exposes execution state, recent activity, and diagnostic log entries, while browser fixtures live under `runtimeAdapters/`. Do not edit Rust merely to rearrange, rename, recolor, or collapse these sections. A new persisted activity type, different retention policy, or new installation fact is native behavior and belongs in a separate change.

### Change Settings, Wizard, or Tuner navigation

The expandable Settings group is entirely frontend-owned. Change its order, icons, labels, or nesting in `App.tsx`; change the hierarchy background, indentation, chevron, and mobile horizontal layout with `.nav-group*` and `.nav-sub*` in `app.css`. The three child entries intentionally reuse the existing `files` and `profiles` views, so ordinary menu redesign does not require a new `ViewId` or Rust launch parser change.

Wizard and Tuner are two presentations of the same profile state, not separate native editors. `ProfilesPage.tsx` owns the selected presentation and shared save, apply, undo, preview, and profile-loading behavior. Keep those operations shared when changing either presentation.

- Change the seven Wizard labels, order, and INI-setting membership in `control-center/src/pages/profiles/wizardModel.ts`.
- Change Wizard step composition, the previous/continue row, and the final preview/save/apply card in `WizardSettings.tsx`.
- Change the always-available quick-action placement in `ProfilesPage.tsx` and `.wizard-quick-actions`.
- Change Tuner section composition in the existing editor components under `control-center/src/pages/profiles/`.
- Change the shared font-substitution UI in `FontSubstitutionEditor.tsx`; both Wizard and Tuner consume it.
- Change visual hierarchy, fixed progress controls, and compact Wizard preview height with `.wizard-*` and `.profile-page[data-mode="quick"]` in `app.css`.

Keep Wizard step buttons directly selectable. The bottom navigation must show only Continue on the first step, both controls in the middle, and only Previous on the final step. Keep preview, font substitution, all advanced settings, profile save, restore defaults, Tuner, and skip available from every Wizard step. These are UX invariants covered by `tests/frontend-gallery/gallery.spec.ts`.

All of the above is React, TypeScript, CSS, and locale JSON. Rust is only needed if a redesign introduces a genuinely new native operation or data source.

### Add a navigation page without native behavior

For a frontend-only page:

1. Add the `ViewId` in `control-center/src/app/model.ts`.
2. Add the page component under `control-center/src/pages`.
3. Register its icon, order, and render branch in `App.tsx`.
4. Add `nav.<id>` to every locale catalog.
5. Add the view and localized titles to `tests/frontend-gallery/windows.ts`.
6. Add a gallery interaction test if the page has controls.

This does not require Rust. Only native command-line launch support for the new view would require a separate change to the Tauri launch parser.

### Change copy or add a locale

Never put translated text directly in TSX. Existing messages live in ten JSON files under `control-center/src/i18n`. Keys and `{placeholder}` names must match exactly across catalogs.

When adding a locale, update `localeOptions`, `catalogs`, and locale detection in `i18n.ts`, then add its script and direction to `tests/frontend-gallery/windows.ts`. RTL is selected in `I18nProvider.tsx`; use logical CSS properties such as `padding-inline-start` when possible and add `[dir="rtl"]` only when the visual direction genuinely changes.

### Change icons

Interface icons come from `lucide-react` and are selected in TSX. Decorative icons need `aria-hidden="true"`. An icon-only button needs a translated `aria-label`. Keep icon size and stroke weight consistent with neighboring controls.

## Browser preview without Rust

The browser gallery adapter supplies installation, profile, font, diagnostics, and service sample data. The real page then runs that data through the same `projectExecutionView` projection used under Tauri. This makes visual and interaction work possible without compiling or launching Rust while preserving the installed application's display and mutation rules.

```powershell
cd control-center
pnpm install --frozen-lockfile
pnpm dev
```

Open a specific state with query parameters, for example:

```text
http://localhost:1420/?gallery=1&view=files&lang=en
http://localhost:1420/?gallery=1&view=profiles&lang=zh-CN
http://localhost:1420/?gallery=1&view=execution&lang=ar
```

The `profiles` URL opens Tuner, matching native launch behavior. To review Wizard, open that URL and choose **Settings → Quick setup (Wizard)** in the sidebar. Both presentations run against the same browser-gallery profile state.

Use the in-app theme button to inspect dark mode. Browser mode is for design and interaction review; native Windows behavior remains behind the runtime adapter.

## Before sending a design change

Run the frontend-only checks from `control-center`:

```powershell
pnpm test:i18n
pnpm test:settings
pnpm lint
pnpm build
pnpm test:gallery
```

The gallery covers every public page in all supported locales at 390, 768, and 1280 pixels. It fails on JavaScript errors, horizontal overflow, broken RTL, missing translations, or inaccessible interaction results. Review the generated images under `artifacts/frontend-gallery`, including at least one mobile layout, one dark state, and Arabic RTL.

## Stop conditions

A design-only change has crossed into native scope if it requires any of the following:

- a new Tauri command;
- a different Rust DTO;
- filesystem, registry, service, tray, or process behavior;
- a new MacType INI setting;
- a Preview Helper protocol change.

Stop there and separate the native behavior from the design change. Maintainers should be able to review and revise the interface without rebuilding Rust for ordinary visual decisions.
