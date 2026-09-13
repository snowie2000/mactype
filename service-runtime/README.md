# MacType open service runtime

This workspace contains the Tauri-free machine service foundation.

In product terminology, **신식 서비스** is this open-source runtime and **레거시 서비스** is the original `MacType` service hosted by `MacTray.exe`. The latter is a migration subject and fallback only. The normative interface and status matrix are in `docs/open-service-contract.md`; operator procedures are in `docs/service-maintenance.md`.

- `contract` owns fixed service identity, versioned read-only health, broker verbs,
  protected path layout, runtime manifest verification, and profile generation logic.
- `host` owns the Windows SCM dispatcher, control handler, status reporting, protected
  active-profile verification, process observation, the fixed helper broker, and the
  fixed v1 health named pipe.
- `setup` owns fixed broker dispatch, protected payload staging, SCM mutation, ACL
  hardening, profile publication, activation, and rollback.
- `platform` owns every Win32 call the other crates make, the Control Center included.
  It is the only crate in either Rust tree that may contain `unsafe`; `host`, `setup`,
  and `control-center/src-tauri` forbid it at their roots.
  Callers receive owned handles, fixed rights sets, and bounds-checked values, and every
  `unsafe` block names in a `SAFETY:` comment the invariant the surrounding type keeps.

## Safety contract

- Production service identity is `MacTypeControlCenter`; the compiled CI adapter uses
  the separate fixed identity `MacTypeControlCenterTest`.
- No runtime command accepts a service name, executable path, DLL path, command line,
  or profile path. `publish-profile` alone reads bounded profile bytes from stdin.
- Service code and helpers are read only from
  `%ProgramFiles%\MacType Control Center\Service\bin\<version>`.
- The service opens each target with only the fixed injection rights, rechecks its
  creation time, and uses a `STARTUPINFOEX` handle list to give the selected x86/x64
  helper that process handle plus its bounded standard handles. Helpers never reopen
  an observed PID and fail closed unless the inherited handle repeats every identity,
  safety, architecture, and module-inventory check.
- Each helper is created suspended, assigned to a private Job Object with one active
  process and kill-on-close limits, rechecked against service stop, and only then
  resumed. A running helper may finish its remote cleanup after stop is requested;
  the 20-second absolute bound terminates its job and reports cleanup as unknown.
- Setup atomically materializes the active profile's complete bytes as the generated
  `%ProgramFiles%\MacType Control Center\Service\bin\<version>\MacType.ini`; the
  service never forwards an arbitrary profile path or `AlternativeFile` to MacType.
- Runtime and profile activation use fixed, durable recovery journals below the
  protected machine roots. Setup restores the previous pointer/configuration before
  the next mutating or start operation, and the host refuses `Ready` while recovery is
  pending.
- Active profile generations are read only from
  `%ProgramData%\MacType\ControlCenter\generations\<sha256>`.
- Runtime manifests accept only the five public runtime filenames defined in
  `contract/src/manifest.rs`, and every declared SHA-256 must match. Installed
  generations allow only those five immutable files plus the setup-generated
  `MacType.ini`.
- An SCM `Running` state is not health. Ready is reported separately through
  `\\.\pipe\MacTypeControlCenter.health.v1` after the protected active profile and
  all required readiness components validate, including exact byte and digest parity
  between the active protected profile and the DLL-adjacent `MacType.ini`.
- Existing services with a foreign ImagePath, account, service type, or start mode are
  never started, stopped, reconfigured, or removed.
- This workspace contains no Tauri or WebView dependency.
- Raw handles, pointers, and buffer lengths never leave `platform`. The service logic
  in `host` and `setup`, and the Control Center's machine integration, are written
  without `unsafe`, so a defect there cannot become memory unsafety; the boundary is
  enforced by `#![forbid(unsafe_code)]` in those crates and by clippy's
  undocumented-unsafe-block lint inside `platform`.

## Verification

```powershell
cargo fmt --manifest-path service-runtime/Cargo.toml --all -- --check
cargo test --manifest-path service-runtime/Cargo.toml --workspace --all-targets --features mactype-service-host/ci-test-adapter,mactype-service-setup/ci-test-adapter
cargo clippy --manifest-path service-runtime/Cargo.toml --workspace --all-targets --features mactype-service-host/ci-test-adapter,mactype-service-setup/ci-test-adapter -- -D warnings
```

Actual SCM install/start/Ready/stop/remove and protected ACL checks require an elevated,
disposable Windows runner. They must use the fixed CI adapter identity and must not run
against a developer workstation service.

The normal hosted lifecycle runs `scripts/ci/Test-OpenServiceWindows.ps1`. Reboot, AppInit, migration, and multi-user checks use the dispatch-only `.github/workflows/open-service-disposable-vm.yml`. An implemented verifier is not a PASS: its result remains `UNKNOWN` until the disposable-VM JSON artifact is retained.
