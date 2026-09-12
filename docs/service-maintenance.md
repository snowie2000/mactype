# Service maintenance

This runbook covers the **신식 서비스** (`MacTypeControlCenter`) and its isolated CI identity (`MacTypeControlCenterTest`). The **레거시 서비스** is the original `MacType`/`MacTray.exe` service and is touched only by the migration workflow.

## Routine inspection

1. Query SCM state, ImagePath, account, start mode, and PID. Do not equate `Running` with Ready.
2. Read the bounded protected `%ProgramFiles%\MacType Control Center\Service\health.json` snapshot or the versioned health pipe.
3. Require protocol 1, `health=ready`, four Ready components, an active profile digest, and no `lastError` before reporting system integration active.
4. Compare `current.json`, `active.json`, and the DLL-adjacent `MacType.ini`. A recovery journal or byte mismatch is a repair condition, not a cosmetic warning. A runtime whose only missing file is the generated `MacType.ini` is reported as `runtime-profile-absent` with terminal `Unknown` health and a clean stop, not as a failure.

The setup interface has fixed verbs only: `install`, `upgrade`, `repair`, `remove`, `start`, `stop`, `publish-profile`, `rollback`, and `restore-runtime`. Never add a service-name or path override for operator convenience.

The 신식 서비스 recovery policy retries after 5 seconds and then 30 seconds. `SERVICE_FAILURE_ACTIONS_FLAG` enables those actions for non-crash `SERVICE_STOPPED` errors with a nonzero exit code, including initialization failures.

Because of that flag, a requested stop must never report a nonzero exit code, or recovery would resurrect a service the operator deliberately stopped. When the protected runtime contains the five fixed binaries and only the generated DLL-adjacent `MacType.ini` is absent, startup treats it as the supported stopped state. A missing `active.json`, when no profile has been published yet, ends the same way with `last_error.code=active-profile-absent`; a dangling pointer or pending activation journal still fails closed. In either stopped state, the service publishes terminal `Unknown` health, records one Info `service-start-skipped` event, and reports a clean `SERVICE_STOPPED`. This prevents boot and explicit-start recovery loops while an installation has no published profile, for example a fresh install before the first profile is published. Any other runtime file-set mismatch still fails with exit codes 1066/1 and remains eligible for recovery retries. Once the stop event is signalled, an error raised while winding down is carried in the terminal `Unknown` health snapshot's `last_error` and the service still reports a clean `SERVICE_STOPPED`. For the same reason the driver loop tolerates up to twenty consecutive health-publication failures: periodic telemetry is observability, and losing it must not end a run that is still injecting correctly.

## Service identity and configuration drift

Ownership requires three facts: ImagePath is the quoted fixed service binary below the protected `Service\bin\<version>\` layout followed by ` --service`, the service type is `SERVICE_WIN32_OWN_PROCESS`, and the account is LocalSystem (case-insensitive). Start type, error control, display name, load order group, tag, and dependencies are configuration, not identity.

A change to those configuration fields leaves the service owned. Control Center shows 복구 필요 for a current installation or 업데이트 필요 for an outdated installation, keeps stop available for a running service, and blocks start until the configuration is restored. `repair` and `upgrade` call `reconfigure` to restore the fixed configuration and verify it by reading SCM back. A raw `sc config`, such as changing the start type to demand, no longer strands the installation as foreign or makes the next installer upgrade skip it. A foreign image, service type, or account still prevents ownership. If SCM retains a nonzero tag after the group is cleared, read-back rejects the repair and names `tag`; `ChangeServiceConfigW` has no input parameter that assigns an explicit tag value.

## Build and local non-mutating checks

```powershell
cargo fmt --manifest-path service-runtime/Cargo.toml --all -- --check
cargo test --manifest-path service-runtime/Cargo.toml --workspace --all-targets --all-features
cargo clippy --manifest-path service-runtime/Cargo.toml --workspace --all-targets --all-features -- -D warnings
scripts/ci/Test-OpenServiceContract.ps1
scripts/ci/Test-DistributionPolicy.ps1
```

The service-injector must also pass x86 and x64 Release build, CTest, and MSVC `/analyze`. `lint.yml` is the merge-blocking reference implementation.

## Disposable VM workflow

`.github/workflows/open-service-disposable-vm.yml` is `workflow_dispatch`-only and requires a self-hosted Windows x64 runner labeled `mactype-disposable-vm`. Type `I_UNDERSTAND_DISPOSABLE_VM`; never add `push`, `pull_request`, `schedule`, or `workflow_call` triggers.

Recommended order:

1. `lifecycle` verifies install, Ready, x86/x64 markers, service crash/restart, running/stopped repair, profile rollback, stop, and remove.
2. `prepare-reboot` repeats the lifecycle and leaves `MacTypeControlCenterTest` Auto/Ready with a protected receipt.
3. Reboot the disposable VM outside the job.
4. `verify-after-reboot` requires a later boot, a new PID, Auto start, and unchanged Ready profile.
5. `verify-appinit-conflict` temporarily changes both registry views, requires fail-closed `appinit-conflict`, restores the exact prior values, and requires Ready again.
6. `verify-multi-session` validates operator-supplied schema-1 marker evidence from at least two interactive sessions, with exactly one x86 and one x64 result per session.
7. Perform the consented migration in the product UI, then run `verify-migration` to validate protected receipt hashes, stopped 레거시 서비스, Ready, profile equality, and both architecture smoke results.
8. Run `cleanup` before discarding the VM.

Reboot, multi-session, AppInit, and migration remain `UNKNOWN` until their dispatch artifact exists. A skipped job, absent fixture, missing JSON, or written test plan is not PASS.

## Repair and rollback

- `repair` preserves the caller's running/stopped state. A running service is stopped, repaired from the fixed payload, restarted, and required to reach Ready.
- `rollback` changes the active profile generation and keeps the displaced generation as the next rollback target.
- `restore-runtime` uses the protected migration runtime pin; it is not a general version selector.
- If an activation or repair journal exists, every mutating verb first runs durable recovery. Do not delete journals manually.
- A foreign SCM identity is not repairable by this program. Preserve it and report the exact mismatch; configuration drift on an owned service is repairable.

## Incident handling

| Symptom | Safe response |
| --- | --- |
| SCM Running, health initializing | Wait only for the bounded Ready deadline; then collect health and setup output. |
| `appinit-conflict` | Disable the conflicting registry mode through an explicit user action, then start again. Never silently edit AppInit. |
| helper timeout or cleanup unknown | Treat the target result as terminal, retain logs, and verify the Job ended. Do not retry the same target automatically. |
| one target inaccessible or unsupported | Keep global Ready if infrastructure remains healthy; the target is skipped fail-closed. |
| observer or protected-runtime failure | Mark global health degraded/failed and stop claiming active integration. |
| migration failure | Restore the pinned runtime, original 레거시 서비스 configuration, and prior running state from the protected receipt. |

Record unresolved machine behavior as `UNKNOWN`, with the missing precondition or evidence named explicitly.
