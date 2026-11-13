# MacTray service evidence

This directory stores machine-readable characterization evidence, not conclusions written from memory. Use a disposable VM and preserve the official installer version, installer SHA-256, VM/OS build, account class, case precondition, timestamps, raw command output, probe JSON, and run manifest together.

Terminology is fixed: **레거시 서비스** is the original `MacType`/`MacTray.exe` service observed here; **신식 서비스** is the open-source Control Center service. Evidence from one must never be relabeled as evidence for the other.

Expected layout:

```text
evidence/mactray-service/
  <official-version>/
    baseline/
      pre-install/
      post-install/
      started/
      stopped/
      removed/
    M01/
      <run-id>/
        run.json
        service-before.txt
        service-after.txt
        x86-01/
          probe.json
          stdout.txt
          stderr.txt
        ...
  open/
    <runtime-version>/
      M01/
        <run-id>/
  comparisons/
    M01.json
```

Evidence promotion rules:

1. A plan, missing file, failed command, or absent service remains `UNKNOWN`.
2. One valid run with a confirmed condition may be `OBSERVED`.
3. At least three valid, stable repetitions under the same condition are required for `REPRODUCED`.
4. An open build with valid results may be `OPEN_IMPLEMENTED`; this does not imply parity.
5. Only reproduced official/open runs of the same case with equal signatures may be `PARITY_PROVEN`.

Do not overwrite prior runs. Do not manually edit probe JSON or raw logs. If public publication would expose a real username, machine name, or unrelated process data, repeat the experiment in a disposable VM with non-sensitive identities instead of redacting the only raw copy.

The repository retains reproduced 레거시 서비스 runs for M03, M04, M05, M08, and M09 under `1.0.2023.7`. Other cases remain `UNKNOWN`. 신식 서비스 dispatch results belong under `open/<runtime-version>/<case>/<run-id>` only after the disposable VM actually ran; an implemented workflow or missing artifact is still `UNKNOWN`.
