# Legacy execution-mode findings

Phase 4 treats the public source and the installed binaries as evidence, not as a license to reproduce undocumented Delphi behavior.

## Manual mode

`MacLoader.exe` and `MacLoader64.exe` are built from the public `run.cpp`. The 32-bit loader inspects the target PE, delegates a 64-bit target to `MacLoader64.exe`, and calls Detours `DetourCreateProcessWithDllEx` with `MacType.dll` or `MacType64.dll`. The Control Center therefore exposes a manual launcher that starts `MacLoader.exe` directly with an existing `.exe` path and an argument array. It never constructs a shell command.

## Tray mode

The legacy `MacTray.exe` implementation is not in the public core and is not shipped or launched by the Control Center. Tauri owns the new notification icon, show/hide/quit menu, close-to-tray behavior, and optional per-user startup value at `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. This is the supported non-admin session mode.

## Legacy service

An official installation may register a service named `MacType` whose image is `MacTray.exe -service`. The Control Center reads both the Service Control Manager configuration and runtime state, and only treats a service as owned when its executable resolves to the verified `MacTray.exe` under Program Files and its service type, account, startup mode, argument, error mode, and `winmgmt` dependency match the official layout. A matching historical unquoted command line is shown as a repairable compatibility state; foreign, inaccessible, and deletion-pending services are never mutated.

Installing and removing this legacy mode remains a migration-only interoperability feature, not part of the independent distribution. The normal Control Center always runs as the current user. A user-initiated action starts a short-lived `runas` broker which accepts only fixed migration verbs, revalidates the trusted binary and current SCM state, and calls the Windows Service Control Manager directly through `CreateServiceW`, `DeleteService`, `StartServiceW`, and `ControlService`. It never asks the Delphi executable to register or unregister itself. The status is queried again after every operation. An existing AppInit MacType configuration blocks mutation to avoid double injection.

## AppInit registry mode

The official project removed registry mode from its wizard because an incorrect configuration can prevent Windows from booting. Its manual guide modifies both 64-bit and 32-bit `AppInit_DLLs`, enables `LoadAppInit_DLLs`, weakens `RequireSignedAppInit_DLLs`, and changes the system `PATH`. The Control Center detects an existing MacType AppInit entry read-only and deliberately provides no apply action. The narrowly scoped service broker cannot write AppInit, IFEO, or arbitrary registry values.

Primary references:

- `run.cpp` in this repository
- <https://github.com/snowie2000/mactype/wiki/Enable-registry-mode-manually>
- <https://github.com/snowie2000/mactype/wiki/HookChildProcesses>
