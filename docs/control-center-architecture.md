# Control Center architecture

The x64 Tauri process never loads `MacType.dll`. It owns profile files, validation, the child-process lifecycle, and the WebView. `mactype-preview32.exe` is the only process allowed to load the selected installation's x86 DLL.

## Machine integration terminology

`MachineIntegration` is the deep module behind the frontend system-integration seam. Its frontend adapter produces `ExecutionViewModel`; SCM flags, protected storage, migration receipts, helpers, and registry inspection remain implementation details. **신식 서비스** means the open-source `MacTypeControlCenter` runtime. **레거시 서비스** means the original `MacType` service hosted by `MacTray.exe`; it is a migration subject and fallback only.

## Preview boundary

The parent starts the Helper directly with `std::process::Command`, redirected stdin/stdout/stderr, and no shell. MTPC v1 frames have fixed little-endian headers and bounded JSON/PNG lengths. A reader thread consumes responses while stderr is retained in a 100-line diagnostic buffer. Requests time out after two seconds; the parent terminates and restarts the Helper once before returning an error.

The Helper validates an x86 PE image, `MacType.ini`, and `CreateControlCenter`. `EasyHK32.dll` is optional because the independent `Rel+Detours` package has no external EasyHook runtime dependency. Current public installers do not all export `DllGetVersion`, so that export is reported as an optional capability and `IControlCenter::GetVersion` is the authoritative core version. All `IControlCenter` mutation and GDI rendering remains in the x86 process.

Preview pixels are rendered into a top-down 32-bit DIB and encoded through WIC. PNG bytes cross only the binary frame section. Tauri writes them under app-local data and the WebView reads the narrowly scoped asset URL; no base64 image is retained in application state.

## Profile boundary

Rust owns a line-preserving INI document. It retains BOM, encoding, line endings, blank lines, comments, unknown entries, and ordering. Only the value slice of a changed key is rewritten. Save compares the original SHA-256, flushes a same-directory temporary file, keeps one backup, and uses `ReplaceFileW` on Windows.

The editor keeps the selected source profile as the editing identity. A writable profile in the installation `ini` directory or the user-owned `%LOCALAPPDATA%\MacType\ControlCenter\profiles` directory is saved back to that same file. Installed read-only profiles and external files require an explicit Import or Save As operation; both create a user-owned copy without elevation. A profile selected by an existing installation's `MacType.ini` is opened directly when it already belongs to a known profile directory, so the UI does not ask the user to import a profile it can already edit.

User-facing and persisted source identities are portable (`ini\Default.ini` or `Profiles\Foo.ini`) for files in those profile directories. Absolute paths are retained only for actual file I/O and reveal-in-Explorer actions. Applying is allowed only from a saved document and publishes the exact saved bytes into the service's protected generation; the generated `profile.ini`, digest, and runtime path remain internal implementation details. Imports are strictly decoded as INI documents, copied byte-for-byte, and receive a collision-safe name.

Scalar settings use the public core's `[General]` keys. Structured `[Individual]`, font include/exclude, and module include/exclude sections retain their surrounding comments while edited entries are validated and replaced. The legacy-codec gate vendors a pinned, licensed 70-profile community corpus and requires correct encoding detection, byte-identical no-edit round trips, edit/save/reopen behavior, and line-ending/BOM preservation without network access.

`shared/settings-schema.json` is the source for generated Rust, TypeScript, and C++ setting definitions. CI regenerates and rejects drift.

## Localization boundary

The React frontend owns ten complete runtime catalogs: Korean, English, Simplified Chinese, Traditional Chinese, Japanese, French, German, Spanish, Portuguese, and Arabic. An explicit `?lang=` value takes precedence and is persisted per user; otherwise the stored preference or the browser language selects the initial locale. Chinese script and regional subtags are normalized separately (`zh-Hant`, Taiwan, Hong Kong, and Macao select Traditional Chinese), while unsupported locales fall back to English.

Changing the language updates visible text, the document title, accessibility labels, the HTML language and direction, and the native Tauri tray menu without restarting. Arabic sets native right-to-left document direction and direction-aware navigation and editor borders. CI requires exact catalog key and placeholder parity, coverage for all generated settings, native tray-menu tests, and real browser rendering of every view, viewport, and locale.

## Execution boundary

The open Tauri executable owns the notification icon and close-to-tray lifecycle. Its optional login startup entry is user-scoped. The GUI remains `asInvoker`; machine mutation is isolated in a one-shot fixed-verb broker dispatched before Tauri starts. AppInit registry mode remains read-only in normal UI flows. Manual mode invokes the public `MacLoader.exe` directly with an executable path and argument vector; no shell string is accepted.

The 신식 서비스 is Tauri-free. Rust owns protected runtime/profile generations, SCM state, Ready health, WMI process observation, retry and deduplication policy, repair, and rollback. `ProcessTargetValidator` classifies an observed PID and returns only a verified eligible identity or an explicit skip. `InjectionOrchestrator` binds that identity to one generation and owns deduplication, retry, cancellation, bounded result history, telemetry, and terminal health classification. Service stop cancels an in-flight helper through its private Job Object instead of waiting for the normal 20-second bound. A fixed x86/x64 helper receives one inherited process handle and loads only its adjacent public MacType DLL. Normal target skips, pre-injection rejection, and confirmed service-stop cancellation preserve global Ready. Unknown post-injection cleanup or an invalid helper response degrades the generation until a later verified success; observer or protected-runtime failures may fail it.

The 레거시 서비스 is detected through a strict official-layout adapter. Its configuration and profile can be backed up and restored only by the explicit migration flow. Normal startup, login startup, tray actions, profile application, and 신식 서비스 repair never execute or require MacTray. The normative safety contract is `docs/open-service-contract.md`.

The official single-instance plugin is registered before every other Tauri plugin. On Windows, a pre-Tauri per-session startup mutex serializes cold starts until the first process has created its IPC window and completed setup, closing the plugin's mutex-to-window race. Later launches send their arguments to the existing process, which shows, unminimizes, and focuses the main window. The privileged service broker exits before this gate and therefore never participates in GUI instance arbitration.

On `WM_ENDSESSION(TRUE)`, subclasses on the main, Preview Studio, and hidden tray windows synchronously record the Info event `control-center` / `session-ending` with `reason=wm-endsession`, then exit the process with code 0 before tao 0.35.3 can enter its `Destroyed` state and continue dispatching messages. The window subclass leaves `WM_QUERYENDSESSION` and canceled `WM_ENDSESSION(FALSE)` messages unchanged. This applies to Windows session shutdown and Restart Manager shutdown alike; remove or amend the application-side workaround once Tauri ships tao >= 0.37.

## Maintenance notes

Cross-module contracts belong in this architecture document, `docs/control-center-ci.md`, or `docs/legacy-behavior-notes.md` rather than being repeated beside each implementation. Source comments are reserved for local invariants and platform or compatibility traps that are easy to violate while editing. Generated files retain only their generated-file warning; routine control flow and temporary implementation history should remain uncommented.
