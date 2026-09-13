# MacType service injector

This directory builds the source-owned injection boundary used by the Control Center service. It does not link to or modify the MacType core, EasyHook, MacTray, or Delphi artifacts.

The two production executables are fixed by architecture:

- `mactype-injector32.exe` loads only the adjacent `MacType.dll` into an x86 target.
- `mactype-injector64.exe` loads only the adjacent `MacType64.dll` into an x64 target.

The broker invocation is deliberately narrow and order-sensitive:

```text
mactype-injector64.exe --process-handle <inherited decimal HANDLE> --pid <u32> --creation-time <u64 FILETIME> --session-id <u32> --generation-id <64 hexadecimal characters>
```

No DLL path, executable path, service name, or other runtime selector is accepted. The service opens the target with the fixed injection rights, rechecks its creation time, and passes only that handle through a `STARTUPINFOEX` handle list. The helper never reopens a PID: it owns and closes the inherited handle and uses that handle for identity checks and module enumeration. A fixed MacType module is considered loaded only when its normalized, case-insensitive full path exactly matches the adjacent DLL; a same-named DLL from another directory is not accepted. Closed, non-inherited, mismatched, session-0, protected, critical, architecture-mismatched, and already injected targets fail closed with explicit results.

Standard output contains one JSON object of at most 1,024 bytes. Its schema is:

```json
{"schemaVersion":1,"status":"injected","code":"module-loaded","pid":1234,"sessionId":2,"generationId":"<sha256>","module":"MacType64.dll","windowsError":0,"cleanupComplete":true}
```

Exit code `0` means injected or intentionally skipped, `2` means rejected identity/input, `3` means an injection failure, and `4` means the load exceeded both its deadline and bounded cleanup grace. After a remote thread completes, the helper cross-checks its return value against a fresh inventory of the fixed adjacent module. A load that completes during cleanup grace is a verified `module-loaded-late` success. A zero return value with no module is a definitive `module-load-failed` result.

If the remote thread outlives cleanup grace, its result cannot be read, the module inventory cannot be read, or the two observations conflict, the helper returns a code ending in `-cleanup-unknown` with `cleanupComplete:false`. The service treats that result as terminal for the process identity and degrades the active generation; it is never retried automatically. The helper never terminates a target thread merely to force cleanup.

With `BUILD_TESTING=ON`, CTest builds isolated x86 or x64 marker targets and validates real DLL loading, duplicate detection, rejection of a same-basename module from another directory, creation-time/session/architecture rejection, arbitrary runtime selector rejection, bounded JSON, and verified late loading after the initial deadline.
