# Open service contract

This document is the normative machine-runtime contract. In product language, **신식 서비스** means the open-source `MacTypeControlCenter` service owned by Control Center. **레거시 서비스** means the original `MacType` service hosted by `MacTray.exe`. MacTray is a migration subject and fallback only.

## Responsibility and trust

- Rust owns SCM lifecycle, protected installation, health, session/process observation, bounded retry, migration, repair, and rollback.
- Across both Rust trees, `mactype-service-platform` is the only crate that may contain `unsafe`; `host`, `setup`, and the Control Center crate forbid it at their roots. Win32 objects cross that boundary only as owned handles with fixed rights sets and as bounds-checked values.
- The public C++ fixed helper owns remote injection. The existing MacType rendering and hook implementation remains untouched.
- React consumes `ExecutionViewModel`; it never infers success from raw SCM `Running`.
- LocalSystem executes only files below `%ProgramFiles%\MacType Control Center\Service` and reads active profiles only below `%ProgramData%\MacType\ControlCenter`.
- No production command accepts an arbitrary service name, executable, DLL, command line, or profile path.

## Fixed SCM registration

The production service name is `MacTypeControlCenter`. Its ownership identity is the quoted fixed service binary below the protected `Service\bin\<version>\` layout followed by ` --service`, the `SERVICE_WIN32_OWN_PROCESS` service type, and the LocalSystem account (case-insensitive). A different image, service type, or account is foreign and cannot be mutated by the fixed setup broker.

The fixed configuration is automatic start, normal error control, display name `MacType Control Center Service`, no load order group and no dependencies. A nonzero tag counts as drift only when a load order group is present; without a group it is inert, and Windows offers no way to zero it. Differences in these configuration fields are `ServiceConfigurationDrift`, not foreign identity. An owned service remains eligible for maintenance and profile publication; Control Center blocks starting it while drift remains and offers repair or upgrade. Both restore and read back the fixed configuration through `reconfigure`. The captured-configuration comparison between preflight and mutation remains an exact tamper check, including configuration fields.

## Runtime and profile generations

A runtime generation is immutable and selected by `current.json`. A profile generation is the SHA-256-addressed `generations\<digest>\profile.ini` selected by `active.json`. Setup validates fixed filenames and hashes before activation, writes a durable recovery journal, switches the pointer atomically, and clears the journal only after success. Startup fails closed while recovery is pending or the generated DLL-adjacent `MacType.ini` differs from the active profile bytes, but the exact fixed runtime with only that generated profile absent reports terminal `Unknown` health with `runtime-profile-absent` and a clean stopped status; an absent `active.json` before any profile publication reports the same result with `active-profile-absent`, while a dangling pointer still fails closed.

## Observation and injection

`ProcessTargetValidator` inspects each observed PID and returns only a verified eligible identity or an explicit skip; PID mismatch and inspector infrastructure failure remain structured errors. `InjectionOrchestrator` consumes that decision and owns generation binding, PID-plus-creation-time deduplication, bounded retry and cancellation, the bounded per-target result record, generation-bound telemetry, and terminal health classification. After a creation-time recheck, the fixed broker opens one exact-rights injection handle and passes only that handle through a `STARTUPINFOEX` handle list. A fixed x86/x64 helper validates PID, creation time, session, protection, critical state, architecture, and loaded modules, then loads only its adjacent MacType DLL. A module inventory read that fails with `ERROR_PARTIAL_COPY` while the target's own loader is mid-update is retried inside a fixed 500 ms bound and only while the target is still alive; the orchestrator's bounded retry remains the outer layer. The helper never reopens a PID.

Each helper starts suspended, is assigned to a private Job Object with one active process and kill-on-close, and is resumed only after the service checks for stop. A stop request forbids new helpers and terminates an in-flight helper's entire Job within the dedicated cleanup-confirmation bound; confirmed termination is classified as cancellation, not degradation or retry. Failure to terminate or confirm cleanup remains fail-closed cleanup-unknown. Without cancellation, the helper's whole lifetime remains inside the 20-second absolute bound; expiry terminates the Job and reports cleanup as unknown.

Normal target skips and known pre-injection rejection—including a process disappearing, becoming inaccessible, losing a creation/session/architecture field, or having an unsupported architecture—are fail-closed target results. They are never automatic retries unless the orchestrator's explicit pre-injection allowlist says otherwise, and they do not degrade global Ready health. Unknown post-injection cleanup or an invalid helper response degrades the affected runtime generation because injection state can no longer be proven; a later clean success may restore Ready. A PID identity mismatch or an unknown inspector error violates the inspector interface and remains a structured error. Observer failure, protected-runtime failure, fixed-broker readiness failure, or another infrastructure failure may degrade or fail the service.

Session-change notifications enter a fixed-capacity nonblocking queue. A burst is drained without last-write-wins loss; queue overflow requests conservative invalidation of all process deduplication state rather than pretending that an unknown session event was handled.

## Health contract

SCM `Running` means only that the process is alive. Product UI may report system integration active only when protocol v1 health is `ready`, all required readiness fields are `ready`, an active profile digest is present, and no structured error exists. Migration removal additionally requires matching runtime/profile evidence from both x86 and x64 marker injections.

## Migration contract

Migration is explicit and reversible:

1. classify the 레거시 서비스 using the strict official layout;
2. back up SCM configuration, registry export, `MacType.ini`, and the selected profile under the protected migration root;
3. stop, but do not automatically delete, the 레거시 서비스;
4. publish the same profile to a protected generation and start the 신식 서비스;
5. require strict Ready, digest equality, and x86/x64 smoke evidence;
6. retain the protected backup so failure can restore both services and the original running state;
7. remove the 레거시 서비스 only in a separate explicitly confirmed operation.

AppInit conflict, a foreign service identity, an invalid protected path, missing architecture evidence, or false Ready cancels the operation and invokes rollback.

## M01–M22 evidence ledger

`PASS` means retained machine evidence exists. `IMPLEMENTED` means an executable verifier exists but is not proof that this repository snapshot passed on a disposable VM. `UNKNOWN` means the condition has not been run or has no adequate evidence; it is never interpreted as failure or success.

| Case | Condition | 레거시 서비스 evidence | 신식 서비스 verifier | Current disposable-VM result |
| --- | --- | --- | --- | --- |
| M01 | Probe starts while service is stopped | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M02 | Service starts after probe | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M03 | Probe starts after Ready | `PASS` | `IMPLEMENTED` | `UNKNOWN` |
| M04 | x86 target | `PASS` | `IMPLEMENTED` | `UNKNOWN` |
| M05 | x64 target | `PASS` | `IMPLEMENTED` | `UNKNOWN` |
| M06 | x86 parent creates x64 child | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M07 | x64 parent creates x86 child | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M08 | Windowless console target | `PASS` | `IMPLEMENTED` | `UNKNOWN` |
| M09 | Window/message-loop target | `PASS` | `UNKNOWN` | `UNKNOWN` |
| M10 | Explorer restart | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M11 | Standard/admin integrity | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M12 | Two users or RDP sessions | `UNKNOWN` | `IMPLEMENTED` (evidence validator) | `UNKNOWN` |
| M13 | Switch, logoff, relogon | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M14 | New probe after stop | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M15 | Existing probe after stop | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M16 | Profile change and restart | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M17 | `HookChildProcesses=0/1` | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M18 | `winmgmt` stop or delay | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M19 | Excluded target | `UNKNOWN` | `UNKNOWN` | `UNKNOWN` |
| M20 | Forced service termination | `UNKNOWN` | `IMPLEMENTED` | `UNKNOWN` |
| M21 | Reboot and first logon | `UNKNOWN` | `IMPLEMENTED` (Auto start only; first-logon coverage `UNKNOWN`) | `UNKNOWN` |
| M22 | Existing AppInit injection | `UNKNOWN` | `IMPLEMENTED` | `UNKNOWN` |

The 레거시 서비스 PASS entries are backed by three trials per architecture under `evidence/mactray-service/1.0.2023.7`. The 신식 서비스 hosted lifecycle and manual disposable-VM interfaces live in `scripts/ci/Test-OpenServiceWindows.ps1` and `scripts/ci/Test-OpenServiceDisposableVm.ps1`. No manual dispatch result is claimed until its JSON artifact is retained and reviewed.
