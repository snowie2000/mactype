# Independent distribution

The installer does not require an existing MacType installation. CI builds the rendering core, manual loaders, fixed x86/x64 service helpers, Tauri Control Center, Win32 Preview Helper, Rust host/setup, and a newly authored default profile from public source. The **신식 서비스** is therefore independent of the **레거시 서비스** and MacTray.

## Installed manifest

- `MacType Control Center.exe`
- `mactype-preview32.exe`
- source-built `MacType.dll`, `MacType64.dll`, `MacType.Core.dll`, and `MacType64.Core.dll`
- source-built `MacLoader.exe` and `MacLoader64.exe`
- staged `service-runtime\mactype-service-setup.exe`
- staged `service-runtime\payload\manifest.json` and its fixed `mactype-service.exe`, `mactype-injector32.exe`, `mactype-injector64.exe`, `MacType.dll`, and `MacType64.dll`
- `MacType.ini` and `ini\Default.ini` authored in this repository
- English and Korean distribution translation catalogs
- GPL and third-party notices

The `Rel+Detours` core DLL is copied under the public runtime names `MacType.dll` and `MacType64.dll`. This build has no external EasyHook runtime dependency, so `EasyHK32.dll` and `EasyHK64.dll` are not redistributed. Delphi GUI files, MacTray, legacy updater files, existing profiles, and existing language resources are forbidden by CI. The administrator-elevated installer writes only to the fixed Program Files application root, then synchronously invokes the fixed, argument-free `bootstrap-install` broker. The broker verifies the immutable payload, publishes or preserves the protected ProgramData profile, configures the exact-owned SCM service, and returns success only when it is Auto/LocalSystem/Running with strict Ready health.

## Installation lifecycle gate

CI builds baseline, deliberate-failure, and current installers with the same stable AppId and fixed Program Files destination. It silently installs the baseline, validates the exact payload, common desktop shortcut, protected profile digest, runtime receipt, SCM identity, and Ready health. The deliberate-failure installer carries a valid, unique manifest but a test-only service executable that cannot reach Ready. CI requires a nonzero installer result and proves automatic restoration of the baseline service configuration, active runtime, protected profile, health, frontend entry point, and broker before continuing. This test-only installer is not uploaded. A distinct current generation must then upgrade successfully without changing protected profile bytes.

The normal Control Center remains an `asInvoker` desktop process. Installation is the administrative operation; later machine-service changes use the same short-lived protected broker and explicit UAC boundary. Neither installer nor bootstrap mutates AppInit or IFEO registry keys. A detected 레거시 서비스, AppInit conflict, foreign fixed-name service, or unknown machine state produces a successful `SkippedBlocked` result without changing that state. Exact-owned uninstall removes only receipted Program Files service runtime files and leaves the protected ProgramData profile in place. Theme, locale, recent/applied profile choices, and user-created or imported files under `%LOCALAPPDATA%\MacType\ControlCenter` are intentionally preserved across install, upgrade, and uninstall.
