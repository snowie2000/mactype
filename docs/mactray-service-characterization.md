# MacTray legacy-service characterization

This document tracks the observable contract of the original MacTray-hosted Windows service. In product terminology, **레거시 서비스** means the original `MacType` service whose host is `MacTray.exe`. **신식 서비스** means the open-source service runtime owned by MacType Control Center. The two terms are intentionally not interchangeable; English prose may add “legacy service” or “new service” only as a parenthetical explanation.

The official binary is an observation subject only. The characterization tools do not install, stop, start, remove, patch, or decompile MacTray. No result in this document authorizes an implementation choice until the relevant claim is `REPRODUCED` or stronger.

## Evidence states

| State | Meaning |
| --- | --- |
| `UNKNOWN` | The fact has not been established. Absence of evidence remains UNKNOWN. |
| `OBSERVED` | One read-only observation of the official legacy service exists. |
| `REPRODUCED` | The same condition and outcome were recorded in at least three valid trials. |
| `OPEN_IMPLEMENTED` | An open-source candidate produced valid evidence for the same case. |
| `PARITY_PROVEN` | Reproduced official and open runs for the same case have equal probe signatures. |

`Run-Matrix.ps1` can promote a run from `UNKNOWN` to `OBSERVED` or `REPRODUCED` only after the operator explicitly confirms the documented precondition. `Compare-Results.ps1` can report `PARITY_PROVEN` only when an `Official` run and an `Open` run are both `REPRODUCED`, have the same case ID, and have equal architecture/probe/result signatures. The scripts cannot prove that an operator prepared the stated VM condition correctly; raw logs and the condition note remain part of review.

## Probe binaries

Configure and build each architecture separately from a Visual Studio developer environment:

```powershell
cmake -S tools/service-probe -B build/service-probe-x86 -A Win32
cmake --build build/service-probe-x86 --config Release

cmake -S tools/service-probe -B build/service-probe-x64 -A x64
cmake --build build/service-probe-x64 --config Release
```

The builds produce:

| Program | x86 | x64 | Purpose |
| --- | --- | --- | --- |
| Console | `probe-console32.exe` | `probe-console64.exe` | Windowless target used to test whether a message loop is required. |
| Window | `probe-window32.exe` | `probe-window64.exe` | Visible target with a normal Win32 message loop. |
| Spawn tree | `probe-spawn-tree32.exe` | `probe-spawn-tree64.exe` | Parent, child, and grandchild node capture; an opposite-architecture child executable can be selected. |

Console and window example:

```powershell
build/service-probe-x64/Release/probe-console64.exe `
  --out evidence/mactray-service/1.0.2023.7/M08/run-01/probe.json `
  --wait-ms 5000
```

Cross-architecture tree example:

```powershell
build/service-probe-x86/Release/probe-spawn-tree32.exe `
  --out evidence/mactray-service/1.0.2023.7/M06/run-01/tree.json `
  --child-exe build/service-probe-x64/Release/probe-spawn-tree64.exe `
  --grandchild-exe build/service-probe-x86/Release/probe-spawn-tree32.exe `
  --wait-ms 5000
```

Every process node writes versioned JSON containing PID, parent PID, session, integrity level, process/native architecture, creation and observation timestamps, observed MacType modules, module path/version, the first polling observation time, and a SHA-256 fingerprint of a fixed GDI DIB render. The version reader prefers the public `DllGetVersion` export and falls back to the file-version resource.

Each spawn-tree node completes its observation interval before creating the next node. This gives a parent time to receive MacType before it creates its child, which is required to distinguish direct service discovery from `HookChildProcesses` propagation.

`loadObservedAt` is the first 25 ms polling observation, not a claim about the exact loader event timestamp. The `modules` array retains every `MacType*.dll` seen during the observation interval, even if it unloads before the final sample. Render fingerprints are comparable only inside equivalent VM snapshots with the same OS build, fonts, DPI, and probe build.

## Baseline capture

Capture each lifecycle phase in a separate directory from an administrative PowerShell prompt:

```powershell
tools/service-characterization/Capture-ServiceBaseline.ps1 `
  -OutputDirectory evidence/mactray-service/1.0.2023.7/baseline/started `
  -ServiceName MacType `
  -Phase started
```

The capture retains raw output from `sc qc`, `queryex`, `qdescription`, `qfailure`, `qfailureflag`, `qtriggerinfo`, `qprivs`, and `sdshow`, plus CIM service data, registry export, MacTray SHA-256 and Authenticode metadata, and installation ACLs. A missing command or service is recorded with its exit code and does not turn UNKNOWN into a negative fact.

## Matrix execution

Planning is non-mutating and leaves every case UNKNOWN:

```powershell
$cases = 1..22 | ForEach-Object { 'M{0:d2}' -f $_ }
tools/service-characterization/Run-Matrix.ps1 `
  -Mode Plan `
  -CaseId $cases `
  -EvidenceRoot evidence/mactray-service `
  -SubjectVersion 1.0.2023.7 `
  -SourceKind Official
```

Execution also never mutates the service. The operator must establish the case precondition in a disposable VM and explicitly record it:

```powershell
tools/service-characterization/Run-Matrix.ps1 `
  -Mode Execute `
  -CaseId M08 `
  -EvidenceRoot evidence/mactray-service `
  -SubjectVersion 1.0.2023.7 `
  -SourceKind Official `
  -ProbeDirectory artifacts/service-probe `
  -ConfirmPrepared `
  -ConditionNote 'Service Ready; clean Windows 11 snapshot; standard user.'
```

Each case defaults to three repetitions. The run manifest contains the raw probe paths, stdout/stderr, before/after read-only `sc queryex` output, normalized signatures, and the status decision. Cases that require service start/stop, reboot, profile edits, Explorer restart, AppInit, multiple users, or WMI disruption remain operator-controlled to prevent the harness from unexpectedly changing a machine.

Compare official and open runs only after both were performed from equivalent snapshots:

```powershell
tools/service-characterization/Compare-Results.ps1 `
  -ReferenceRun evidence/mactray-service/1.0.2023.7/M08/<official-run>/run.json `
  -CandidateRun evidence/mactray-service/open/M08/<open-run>/run.json `
  -OutputFile evidence/mactray-service/comparisons/M08.json `
  -RequireParity
```

## Current read-only observations

The identity and WMI-subscription facts below were observed once on 2026-07-16. The
target-process behavior for M03, M04, M05, M08, and M09 was subsequently reproduced
in three read-only trials per architecture under the same running-service condition.
The raw evidence is stored under
`evidence/mactray-service/1.0.2023.7/<case>/20260716T073305634Z-7edc53a9`.

| Claim | State | Observation |
| --- | --- | --- |
| Official binary identity | `OBSERVED` | MacTray version `1.0.2023.7`; SHA-256 `C83029BF463644A38E38EB85C5F23BB02A8FC3A91FB0274A1C4689D89CF2CC88`. |
| Service host and child | `OBSERVED` | The `MacType` service process was PID 41280 as LocalSystem in session 0 and created `mt64agnt.exe` PID 41376 in session 0. PIDs describe that observation only. |
| Process creation observation | `OBSERVED` | WMI-Activity/Operational event 5860 recorded ClientProcessID 41280, user SYSTEM, and the temporary subscription `SELECT * FROM __InstanceCreationEvent WITHIN 1 WHERE TargetInstance ISA 'Win32_Process'`. |
| x86 and x64 target injection | `REPRODUCED` | Every one of 24 valid M03/M04/M05/M08/M09 trials observed the corresponding MacType modules in medium-integrity targets while the legacy service remained running. Each architecture produced one stable signature per case. |
| Windowless and windowed targets | `REPRODUCED` | Both console probes and Win32 message-loop probes received MacType in all three trials per architecture. A message loop is therefore not required for the tested targets. |
| First module observation | `REPRODUCED` | Across the 24 trials, the first 25 ms polling observation occurred 50–556 ms after probe start (386.1 ms average). This is an observed polling bound, not the exact injection timestamp. |
| Service stability during probes | `REPRODUCED` | Read-only `sc queryex` succeeded before and after every case; the harness did not start, stop, configure, or remove the service. |
| Exact EasyHook/helper contract | `UNKNOWN` | No observation establishes which agent/helper API performs injection or the exact x86/x64 division. |

The WMI event establishes that this observed MacTray process subscribed to process-creation events. It does **not** establish that `winmgmt` is a required SCM dependency, that WMI is the only discovery mechanism, or which process performs the subsequent injection.

## Contract questions

| Contract question | Current state | What is known |
| --- | --- | --- |
| How are new processes discovered? | `OBSERVED` | A temporary WMI `Win32_Process` creation subscription was observed once. Detection-to-injection latency and any additional mechanism are UNKNOWN. |
| Are already-running processes handled? | `UNKNOWN` | M02 has not been executed three times. |
| Who divides x86 and x64 work? | `OBSERVED` | Both architectures were injected reproducibly and an `mt64agnt.exe` child was observed, but the complete helper ownership contract remains UNKNOWN. |
| Is a per-user-session process required? | `UNKNOWN` | The observed service and agent were both in session 0; multi-session behavior is untested. |
| Is `winmgmt` required? | `UNKNOWN` | WMI use was observed, but operational necessity and SCM dependency behavior were not tested. |
| What do service stop and profile change mean? | `UNKNOWN` | M14-M16 have not established unload, future-injection, or profile reload semantics. |
| Is `HookChildProcesses` required? | `UNKNOWN` | M06, M07, and M17 have not been run against official MacTray under both values. |

## Mandatory matrix

| ID | Condition | Current status |
| --- | --- | --- |
| M01 | Start probes while the service is stopped | `UNKNOWN` |
| M02 | Start service after probes | `UNKNOWN` |
| M03 | Start probes after service readiness | `REPRODUCED` (x86 and x64; stable module/render signatures; first observation 50–556 ms across this matrix set) |
| M04 | x86 target | `REPRODUCED` (three valid trials with one stable signature) |
| M05 | x64 target | `REPRODUCED` (three valid trials with one stable signature; helper ownership remains UNKNOWN) |
| M06 | x86 parent to x64 child | `UNKNOWN` |
| M07 | x64 parent to x86 child | `UNKNOWN` |
| M08 | Windowless console probe | `REPRODUCED` (three valid trials per architecture) |
| M09 | Window/message-loop probe | `REPRODUCED` (three valid trials per architecture) |
| M10 | Explorer restart | `UNKNOWN` |
| M11 | Standard/admin integrity levels | `UNKNOWN` |
| M12 | Multiple user sessions/RDP | `UNKNOWN` |
| M13 | User switch/logoff/relogon | `UNKNOWN` |
| M14 | New probe after service stop | `UNKNOWN` |
| M15 | Existing probe after service stop | `UNKNOWN` |
| M16 | Profile change and service restart | `UNKNOWN` |
| M17 | `HookChildProcesses=0/1` | `UNKNOWN` |
| M18 | `winmgmt` stop/delay | `UNKNOWN` |
| M19 | Exclusion target | `UNKNOWN` |
| M20 | Forced service termination | `UNKNOWN` |
| M21 | Reboot and first logon | `UNKNOWN` |
| M22 | Existing AppInit injection | `UNKNOWN` |

The reproduced target-discovery cases justify implementing and testing the open
service's WMI observer and fixed-architecture brokers against the same probe
contract. They do not settle lifecycle, multi-session, AppInit, exclusion, or
failure-recovery behavior; those matrix cases remain explicitly UNKNOWN.

The separate 신식 서비스 implementation/result ledger is maintained in `docs/open-service-contract.md`. No unexecuted disposable-VM workflow changes an `UNKNOWN` entry in this characterization record.

## Harness maintenance

The probe command lines, executable names, exit codes, schema version 1 JSON fields,
and evidence file names are compatibility interfaces. Change them only together with
the x86 and x64 probe contract tests and all four service-characterization contract
tests. A refactor that does not intentionally revise evidence must leave those
interfaces byte-shape compatible apart from timestamps, process identifiers, and
other values that are inherently different between runs.

The C++ probe implementation is arranged by responsibility:

- `probe_common` parses the shared command line and owns the observation loop.
- `process_observation` captures process identity and loaded MacType modules.
- `render_probe` owns the fixed GDI render and SHA-256 fingerprint.
- `snapshot_json` owns schema version 1 serialization; `probe_io` publishes it
  atomically.
- `child_process` owns spawn-tree command-line quoting, process handles, timeout,
  and forced cleanup. It starts each child suspended, assigns it to a
  kill-on-close Job Object capped at the remaining child/grandchild depth, and
  reports `ERROR_TIMEOUT` only after terminating the assigned tree.

The PowerShell harness keeps the M01-M22 question, precondition, probe kind, and
architecture catalog in `lib/MatrixCases.psd1`. `Run-Matrix.ps1` owns status
promotion and evidence layout, while `lib/ProbeHarness.psm1` owns executable
resolution and trial execution. Shared UTF-8-without-BOM output lives in
`lib/CharacterizationIO.psm1`. Do not duplicate these contracts in individual
scripts; this section and the contract tests are the maintenance map.

Every PowerShell probe trial also has a finite deadline derived from the requested
wait. Console and window probes receive ten seconds of startup/exit allowance;
spawn trees receive allowance for all three observation levels plus twenty seconds.
A timeout records an invalid trial with exit code `ERROR_TIMEOUT` and kills the
entire process tree. It never contributes to `OBSERVED` or `REPRODUCED` evidence.
