# Synthesis

## Profile corpus

The requested `snowie2000/mactype/ini` corpus does not exist in the public Git tree or its history. The implementation therefore uses the public Chinese-community distribution `luantu/MacType` at commit `f3e926f75fe134ab1438b792925c082679c715d3`, without calling it official upstream data. Its 70 INIs comprise 48 UTF-16 LE BOM files, 16 UTF-8 files, and six BOM-less GBK/GB18030 files.

All six legacy files were misclassified as Windows-1252. The existing original-line cache made unchanged byte round trips pass despite the wrong decoder. Sufficient proof therefore requires pinned hashes, selected-codec assertions, unchanged byte equality, a CJK edit, save, reopen, and stable re-encoding.

The official v1.2025.6.9 installer was hash-verified but not unpacked: available trusted extractors did not support its Inno Setup version, and an untrusted third-party binary was not executed.

## Legacy service

The installed product and official documentation identify service name `MacType`, executable `MacTray.exe -service`, automatic start, LocalSystem, and `winmgmt` dependency. Public MacTray/installer source is unavailable, so mutation must be conservative.

The UI state cannot collapse SCM errors to `installed=false`. It must distinguish absent, inaccessible, owned, compatible-but-unquoted, foreign, pending, and delete-pending states. Only `ERROR_SERVICE_DOES_NOT_EXIST` proves absence. Foreign configuration and registry-mode conflict block mutations.

Normal UI execution remains non-elevated. A one-shot elevated broker dispatches before Tauri initialization, validates a canonical MacTray under trusted Program Files, performs the official silent install/uninstall contract where available, then re-queries SCM state. Start and stop use bounded polling, removal succeeds only after SCM reports the service absent, and query failures preserve Win32 error codes. CI exercises the pure configuration/capability classifier and a browser-side service transition adapter; it never mutates the real `MacType` service.

Primary references: Microsoft `OpenServiceW`, `QueryServiceStatusEx`, `CreateServiceW`, `StartServiceW`, `ControlService`, `DeleteService`, service state transitions, and service security documentation.

## Single instance

Tauri plugin `single-instance` 2.4.3 must be registered before other plugins. Its callback restores the existing main window with show, unminimize, and focus operations; the same helper serves the tray Show action.

The official Windows implementation has two boundaries requiring project-side policy:

1. A high-integrity first GUI instance can make a medium-integrity second instance fail mutex access and UIPI message delivery. The main GUI must remain `asInvoker`; elevated service work is isolated in the pre-Tauri broker.
2. A cold-start gap exists between mutex creation and IPC HWND creation. A pre-Tauri per-session startup gate serializes Builder entry until setup completes. `WAIT_ABANDONED` is accepted because secondary processes exit inside the plugin.

CI launches eight suspended processes, resumes them as a barrier, and asserts exactly one survivor with seven zero exits and seven successful restoration callbacks. Static policy checks assert `asInvoker`, `uiAccess=false`, and Inno `PrivilegesRequired=lowest`.

Primary references: official Tauri plugin source at commit `cad301fcc1f3ebad1eaef552c886b0bc8580c3fe`, Tauri 2 single-instance documentation, and Microsoft MIC, mutex, UIPI, process creation, and wait documentation.

## Implementation order

1. Merge the corpus and codec proof after all CI gates are Green.
2. Add the file-settings/import UX and consensual installed-profile discovery.
3. Add the explicit service state model and elevated one-shot broker.
4. Add single-instance restoration, startup gate, and stampede CI.
5. Run gallery, lint, build, Rust, policy, and release gates before merging.
