# Control Center design maintenance

This is a file map and editing checklist for maintainers. It explains where to change the interface; it is not a description of the visual concept, which lives in `DESIGN.md`.

## Keep design work out of Rust

Routine visual work belongs under `control-center/src` and can be previewed in a browser. Do not edit these paths for a color, spacing, typography, layout, copy, icon, or page-composition change:

- `control-center/src-tauri/**`
- `preview-helper/**`
- `shared/settings-schema.json`
- the generated files under `control-center/src/generated/`

Those paths define native behavior, IPC, packaging, or the MacType setting model. One shared file is an exception in the other direction: `shared/native-preview-contract.json` holds the native preview window's palettes, chrome metrics, ladder sizes, and label keys. Editing it is design work; `pnpm build` regenerates `control-center/src/generated/nativePreview.ts` from it, and the x86 helper regenerates its own header from the same file on its next build.

If a proposed design change needs a new native command or new data from Windows, put that behavior in its own change. The visual part can then consume the new adapter method without mixing Rust into design review.

## How the frontend is put together

- `control-center/src/app/App.tsx` owns navigation, the theme and language controls, and the installation status. The five views are `overview`, `files`, `profiles`, `execution`, and `diagnostics`; the view ids are interface and never change, even when their labels do.
- Navigation has two groups. The **Wizard** group (`nav.wizardGroup`) holds Profiles (view `files`) and Service (view `execution`). The **Tuner** group (`nav.tunerGroup`) holds Guided setup and All settings, which are the two modes of the `profiles` view (`profileMode: "guided" | "all"`). Wizard vocabulary belongs to that navigation group only; the guided mode is never called a wizard in code or copy.
- Every backend call goes through `runtime()` from `control-center/src/app/runtimeAdapter.ts`. Under Tauri that is `runtimeAdapters/tauriRuntimeAdapter.ts`; in a plain browser it is `runtimeAdapters/browserGalleryAdapter.ts`, chosen automatically when the Tauri runtime is absent. The adapter interface is the one list of what the frontend can ask the native side for.
- `control-center/src/features/` holds behavior shared by pages: the profile document owner (`features/profiles/useProfileDocument.ts`), the overview and diagnostics models, the event timeline, and the preview helpers. `control-center/src/pages/` holds the page compositions. `control-center/src/components/` holds the title bar, the language picker, and the hint popover.
- `control-center/src/app/executionViewModel.ts` is the one pure projection behind the Service page and the Overview: `projectExecutionView(status, busy)` derives every displayed state, copy key, primary action, package notice, foreign/drift warning, active-profile display name, overview state, and mutation gate.

## Where to make each change

| Change | Primary file | Also check |
| --- | --- | --- |
| Colors, spacing, control height, radii, animation timing, navigation width, title bar height | `control-center/src/styles/tokens.css` | Both `:root` and `:root[data-theme="dark"]` |
| Shared controls, sections, grids, page spacing, responsive behavior, RTL rules | `control-center/src/styles/app.css` | The tablet and mobile media queries at the end of the file |
| Application shell, navigation order, the two navigation groups, navigation icons, theme switch | `control-center/src/app/App.tsx` | `.navigation`, `.nav-group`, `.nav-group-label`, `.nav-group-items`, `.nav-item`, `.nav-subitem` in `app.css` |
| Custom title bar layout, icon, window-control icons | `control-center/src/components/WindowTitleBar.tsx` | `.window-titlebar`, `.window-title`, `.window-controls` in `app.css`; `--titlebar-height` in `tokens.css` |
| Language picker layout, option order, scrollable menu | `control-center/src/components/LanguagePicker.tsx` and `control-center/src/i18n/i18n.ts` | `.language-*` selectors in `app.css`, including dark and mobile states |
| Hint popovers on section headings | `control-center/src/components/Hint.tsx` | `.hint`, `.hint-term`, `.hint-popover` |
| Overview status card, four-column details, activity disclosure | `control-center/src/pages/OverviewPage.tsx` and `control-center/src/features/overview/useOverviewModel.ts` | `.overview-service-*`, `.recent-activity`, `.disclosure-actions` |
| Profile list (Files view), legacy import banner, import/export, run profile badge and designate control | `control-center/src/pages/FileSettingsPage.tsx` | `control-center/src/features/profiles/useProfileDocument.ts`; `.profile-card*`, `.legacy-import-*`, `.file-*`, `.selected-file*`, `data-run-profile` |
| Tuner shell: mode switch, header, history actions, save-as, preview placement | `control-center/src/pages/ProfilesPage.tsx` | `.profile-page`, `.profile-layout`, `.profile-header`, `.profile-mode-title`, `.profile-history-actions`, `.settings-workspace`, `.settings-index`, `.settings-form` |
| Guided setup steps, per-step tools, start card, final apply card | `control-center/src/pages/profiles/GuidedSettings.tsx` and `guidedModel.ts` | `.guided-*` and `.profile-page[data-mode="guided"]` in `app.css`; `useStepHistory.ts` for per-step undo |
| Basic, shape, and LCD setting rows | `control-center/src/pages/profiles/SchemaSettings.tsx` | Shared `.setting-row`, `.range-control`, `.number-control` |
| Advanced, per-font, list, and font-substitution editors | `control-center/src/pages/profiles/AdvancedSettings.tsx`, `IndividualSettings.tsx`, `ListsEditor.tsx`, `FontSubstitutionEditor.tsx` | `.advanced-*`, `.individual-*`, `.list-*`, `.font-substitution-*`, `.font-picker*` |
| In-app preview panel: docking, resizer, toolbar, strips, native display modes | `control-center/src/pages/profiles/ProfilePreviewPanel.tsx` | `.preview-panel`, `.preview-resizer`, `.preview-toolbar`, `.preview-controls`, `.preview-canvas`, `.preview-strip`, `.preview-footer`; `features/preview/wrapSample.ts` for line breaking |
| Native preview window colors, metrics, ladder sizes | `shared/native-preview-contract.json` | `control-center/src/features/preview/nativeChrome.ts` (consumes the generated `nativePreview.ts`); its localized labels come from the catalogs through `nativePreviewLabels.ts` |
| Service page layout, status card, mode rows, manual launcher | `control-center/src/pages/ExecutionPage.tsx` | `control-center/src/app/executionViewModel.ts`; `.service-*`, `.service-row*`, `.service-summary*`, `.service-package-notice*`, `.system-injection-*`, `.system-mode-*`, `.manual-*`, `.legacy-tray-conflict*`, `.migration-confirmation*` |
| Diagnostics installation card, components table, timeline placement, log sources | `control-center/src/pages/DiagnosticsPage.tsx` and `control-center/src/features/diagnostics/useDiagnosticsModel.ts` | `.installation-heading`, `.diagnostic-list`, `.diagnostic-events`, `.disclosure-actions` |
| Event timeline rows, chips, search, details, view options | `control-center/src/features/events/EventTimeline.tsx` | `.event-*` in `app.css`; `eventText.ts` for titles and localized reasons; `eventViewPreference.ts` for the three persisted view options |
| User-facing text | Every JSON catalog under `control-center/src/i18n/` | All ten catalogs keep the same keys and placeholders; every non-ASCII character in `ko.json` must exist in `control-center/src/assets/fonts/ko-glyphs.txt` |
| Locale order, language detection, RTL selection | `control-center/src/i18n/i18n.ts` and `I18nProvider.tsx` | `localeOptions` fixes the picker order; `I18nProvider.tsx` sets the document direction |
| Browser-only sample data used during design work | `control-center/src/app/runtimeAdapters/browserGalleryAdapter.ts`, `browserGalleryExecution.ts`, `browserGalleryProfiles.ts` | Keep fixture DTOs equal to `runtimeAdapter.ts`; the fixtures simulate the native status, they are not a second place to decide what the UI allows |
| In-app MacType logo | `control-center/public/mactype-icon.png` | Keep the filename so no code changes |
| Packaged EXE and installer icon | `control-center/src-tauri/icons/icon.ico` and `assets/mactype.ico` | Assets only; no Rust source change |

## Common maintenance tasks

### Change the palette or density

Start in `tokens.css`. Prefer changing a semantic token to replacing hex values or pixel sizes throughout `app.css`. Update the light and dark values together, then inspect status colors, disabled controls, focus rings, and preview backgrounds.

Spacing follows the `--space-*` scale. Controls use `--control-height`; the navigation rail, the title bar, and the Tuner's index and control columns have their own tokens (`--nav-width`, `--titlebar-height`, `--settings-index-width`, `--settings-control-width`). A density change should normally be possible without editing a page component.

### Change a shared control or section

Edit the shared selector in `app.css` before adding a page-specific override. The reusable patterns are:

- `.button`, `.icon-button`, and `.text-action`
- `.section-block` and `.section-heading`
- `.detail-list` and `.status-band`
- `.setting-row`, `.range-control`, and `.number-control`
- `.success-message`, `.inline-error`, and `.warning-text`
- `.disclosure-actions` for the collapsed-by-default blocks on Overview and Diagnostics

Keep focus-visible styling and disabled-state contrast. Do not remove visible focus to match a screenshot.

### Change a page layout

Edit the page component under `control-center/src/pages` and its selectors in `app.css`. Page components decide structure and accessibility; CSS decides placement and presentation. Keep user-facing copy in the locale catalogs rather than in TSX.

For grid children that contain paths, translations, or font names, use `minmax(0, 1fr)` and `min-width: 0`. Test long German, Chinese, and Arabic content instead of relying on English width.

### Change the custom title bar

The visible title bar is ordinary React and CSS. Change its markup, logo, text, or Lucide icons in `WindowTitleBar.tsx`; change its height with `--titlebar-height` in `tokens.css`; and change colors, borders, hover states, or control widths in the `.window-titlebar`, `.window-title`, and `.window-controls` rules. None of those changes requires Rust.

Keep `data-tauri-drag-region` on the non-interactive title area so the real window remains draggable. Keep minimize, maximize/restore, and close as separate buttons with translated `aria-label` values. `WindowTitleBar.tsx` calls the narrow Tauri window API only after detecting the native runtime, so the same component stays visible and safe in browser mode.

The native frame is disabled once in `control-center/src-tauri/tauri.conf.json`, and the matching window-control permissions live in `control-center/src-tauri/capabilities/default.json`. Treat those as platform wiring. Ordinary title-bar redesign leaves them alone; only restoring the operating-system title bar or adding a new native window operation needs a native configuration change.

### Change the Service page or its status card

The MacType on/off card, the summary grid, the notices, and the expandable mode rows are ordinary React and CSS in `ExecutionPage.tsx` and the `.service-*`, `.system-injection-*`, and `.system-mode-*` rules; their copy lives in the catalogs. Browser mode supplies the service status and the activation result, so these visual changes do not require Rust.

`projectExecutionView` in `control-center/src/app/executionViewModel.ts` is the one place that decides what the page shows: the displayed state, the primary action, the service package notice, the foreign-service and configuration-drift warnings, the active profile's display name, the upgrade/repair visibility, and every mutation gate. `ExecutionPage.tsx` renders that projection in the real application and in the browser gallery alike; the gallery adapters only supply representative status DTOs.

Keep the existing adapter calls behind the buttons. `systemInjectionActive` is native truth from the verified service state, and `systemModesSupported` decides whether activation is safe to offer. Renaming, recoloring, or rearranging the card may change TSX, CSS, locale JSON, or the projection, but must not re-derive `canInstall`, `canStart`, `canRepair`, migration, notice, or primary-action decisions in the component or the fixtures. A new service action or a different safety policy is native behavior and is reviewed apart from the design change.

### Change the Overview or Diagnostics sections

`OverviewPage.tsx` shows the current MacType state and recent successful activity, not installation inventory. Its status card, four-column detail list, conditional Service shortcut, and activity disclosure are ordinary React and CSS; `features/overview/useOverviewModel.ts` reads the execution status and the recent activity and takes the three-way overview state from the same projection the Service page uses. Change composition in `OverviewPage.tsx`, layout in `.overview-service-*`, `.recent-activity`, and `.disclosure-actions`, and copy in the catalogs.

Keep the Service shortcut conditional: a healthy running state has no action, while inactive or problem states offer the route to service controls. The recent-activity list is collapsed by default, contains at most five successful events, and leaves errors to Diagnostics. Those are interaction and information-hierarchy invariants covered by the browser gallery.

Installation inventory, the components table, the always-visible event timeline, and the log-source disclosure live in `DiagnosticsPage.tsx`; `features/diagnostics/useDiagnosticsModel.ts` turns installation findings into labels and values. The log-source disclosure keeps Log folder on the left and Expand/Collapse on the right through the shared `.disclosure-actions` layout.

The data itself crosses the native seam: `runtimeAdapter.ts` exposes execution state, recent activity, and events, and the browser fixtures live under `runtimeAdapters/`. Do not edit Rust merely to rearrange, rename, recolor, or collapse these sections. A new persisted activity type, a different retention policy, or a new installation fact is native behavior and belongs in its own change.

### Change the event timeline

`features/events/EventTimeline.tsx` is the one timeline component, styled by the `.event-*` rules. Titles, times, and the localized reasons of `injection-failed` and `helper-broker-failed` come from `eventText.ts` through `event.*` and `event.reason.*` catalog keys; an unknown value keeps the backend spelling. The three view options (hide apply summaries, collapse repeated apply failures, hide routine app and preview events) live in `eventViewPreference.ts` under the `mactype-control-center.event-view` storage key; they affect only the display. Changing chips, search, row layout, or the details disclosure is frontend work; changing which events exist or how they are logged is native.

### Change the Tuner navigation, Guided setup, or All settings

The Tuner group in the navigation and both editing modes are frontend-owned. Change the group's order, icons, labels, or nesting in `App.tsx` and the `.nav-group*` and `.nav-subitem` rules. The two child entries reuse the `profiles` view with `profileMode` `"guided"` or `"all"`, so a menu redesign does not need a new view id or a Rust launch-parser change.

Guided setup and All settings are two presentations of the same profile document, not separate editors. `ProfilesPage.tsx` owns the selected mode and the shared save, save-as, designate, undo, redo, discard, preview, and profile-loading behavior through `features/profiles/useProfileDocument.ts`; keep those operations shared when changing either presentation.

- Change the nine guided steps (`start`, `rendering`, `quality`, `boldItalic`, `hinting`, `gamma`, `lcd`, `substitution`, `apply`), their order, and their INI-setting membership in `control-center/src/pages/profiles/guidedModel.ts`.
- Change step composition, the per-step undo/redo/discard/reset tools (`.guided-step-tools`), the previous/continue row, the start card, and the final save/designate card (`.guided-apply-card`) in `GuidedSettings.tsx`.
- Change All settings section composition in the editor components under `control-center/src/pages/profiles/`.
- Change the shared font-substitution UI in `FontSubstitutionEditor.tsx`; both modes consume it.
- Change visual hierarchy, the fixed progress controls, and the compact guided preview height with `.guided-*` and `.profile-page[data-mode="guided"]` in `app.css`.

Keep guided step buttons directly selectable. The bottom navigation shows only Continue on the first step, both controls in the middle, and only Previous on the final step. Save and designate are offered on the final step; font substitution has its own step. These are UX invariants.

### Change the preview panel or the native preview window

`ProfilePreviewPanel.tsx` owns the in-app preview: docked beside the settings form in the Tuner, or as a resizable bottom panel; the saved-versus-edited comparison; and the controls that open the helper's detached native window in its sample, ladder, compare, and listing modes. Change its markup and the `.preview-*` rules freely. Bitmaps are shown at device pixels; never scale a preview image with CSS transforms. Line breaking of the sample text is decided in `features/preview/wrapSample.ts`.

The native window draws its own chrome from values the frontend sends. Its palettes, metrics, and ladder sizes are in `shared/native-preview-contract.json`; `features/preview/nativeChrome.ts` turns the generated `nativePreview.ts` and the current theme into the object sent to the helper, and `nativePreviewLabels.ts` supplies the localized labels from the catalogs. Changing those values is design work. Adding a control to the native window or a new display mode is a helper change and goes in its own change.

### Change the profile list and the run profile controls

The Files view (`FileSettingsPage.tsx`) lists profiles as cards with thumbnails, marks the 실행 프로필 with `data-run-profile`, and offers open, import, export, duplicate, save, and designate. `features/profiles/useProfileDocument.ts` owns the open document and the designation for both this view and the Tuner, including the choice of which profile to open first (the remembered profile, then the applied or managed legacy profile, then the bundled default). Rearranging the cards, the legacy import banner (`.legacy-import-*`), or the action rows is frontend work; the rules for when designation is allowed stay in the hook.

### Add a navigation page without native behavior

For a frontend-only page:

1. Add the `ViewId` in `control-center/src/app/model.ts`.
2. Add the page component under `control-center/src/pages`.
3. Register its icon, order, and render branch in `App.tsx`.
4. Add `nav.<id>` to every locale catalog.

This does not require Rust. Only native command-line launch support for the new view would need a change to the Tauri launch parser.

### Change copy or add a locale

Never put translated text directly in TSX. Messages live in ten JSON files under `control-center/src/i18n`. Keys and `{placeholder}` names must match exactly across catalogs, and every Korean glyph must be covered by the bundled subset font.

When adding a locale, update `localeOptions`, `catalogs`, and locale detection in `i18n.ts`. RTL is selected in `I18nProvider.tsx`; use logical CSS properties such as `padding-inline-start` where possible and add `[dir="rtl"]` only when the visual direction genuinely changes.

### Change hints and icons

Section headings can carry a `Hint` (`control-center/src/components/Hint.tsx`) whose popover flips to stay inside the viewport; its text is a catalog key. Interface icons come from `lucide-react` and are selected in TSX. Decorative icons need `aria-hidden="true"`; an icon-only button needs a translated `aria-label`. Keep icon size and stroke weight consistent with neighboring controls.

## Browser preview without Rust

When the Tauri runtime is absent, `runtime()` returns the browser adapter, which supplies installation, profile, font, event, and service sample data. The real pages then run that data through the same `projectExecutionView` projection and the same hooks the installed application uses, so visual and interaction work needs neither a Rust build nor a running service.

```powershell
cd control-center
pnpm install --frozen-lockfile
pnpm dev
```

Open a state with query parameters, for example:

```text
http://localhost:1420/?view=files&lang=en
http://localhost:1420/?view=profiles&lang=zh-CN&theme=dark
http://localhost:1420/?view=execution&lang=ar
```

| Parameter | Values | Effect |
| --- | --- | --- |
| `view` | `files`, `profiles`, `execution`, `diagnostics`; anything else opens the overview | Initial view. `profiles` opens All settings; pick Tuner > Guided setup in the navigation for the guided mode. |
| `lang` | one of the ten locale ids | Wins over the stored locale and is persisted (`mactype-control-center.locale`). |
| `theme` | `light`, `dark` | Wins over the stored theme and is persisted (`mactype-control-center.theme`). |
| `system-service`, `service-runtime`, `service-package`, `service-fail`, `service-delay` | see `runtimeAdapters/browserGalleryExecution.ts` | Service installation, runtime state, package state, a failing action, or a slow action. |
| `legacy`, `legacy-state`, `legacy-tray`, `legacy-startup`, `legacy-retired`, `legacy-applied`, `legacy-profile` | see `browserGalleryExecution.ts` and `browserGalleryAdapter.ts` | Legacy MacType service and tray scenarios, including migration. |
| `raw-active`, `profile-unapplied`, `profile-runtime-missing`, `profile-read-only`, `profile-fail-setting`, `fresh`, `preview-delay` | flags | Profile and preview fixtures. |
| `events-empty`, `events-absent`, `events-unreadable` | flags | Event log fixtures for Diagnostics. |

A gallery URL also carries `gallery=1`; nothing reads it, it only marks the mode. Use the in-app theme button or `?theme=` to inspect dark mode. Browser mode is for design and interaction review; native Windows behavior stays behind the runtime adapter.

## Before sending a design change

Run the frontend-only checks from `control-center`:

```powershell
pnpm lint
pnpm build
```

Then serve the build with `pnpm preview` and open each view with `?gallery=` in all ten locales at 390, 768, and 1280 pixels, including at least one mobile layout, one dark state, and Arabic RTL. Watch for JavaScript errors, horizontal overflow, broken RTL, and missing translations.

## When a change needs native work

A design change has reached native scope when it needs one of the following:

- a new Tauri command or a different Rust DTO;
- filesystem, registry, service, tray, or process behavior;
- a new MacType INI setting;
- a new control or mode in the native preview window, or any other helper protocol change.

That is not a reason to stop. Put the native part in its own change with its own review, and land the visual part against the browser fixtures first if it can stand on its own. Maintainers can then review and revise the interface without rebuilding Rust for ordinary visual decisions.
