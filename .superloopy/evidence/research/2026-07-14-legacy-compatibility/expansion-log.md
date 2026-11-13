# Expansion log

## Wave 1

- official release profile corpus lane: closed; upstream Git history contains no INI, pinned community corpus found
- Control Center code lane: closed; unchanged legacy line cache can hide a wrong codec choice
- legacy service lane: closed; SCM state and mutation boundaries identified
- Tauri single-instance lane: closed; official plugin and restore-window seam identified

## Wave 2

- corpus provenance expansion: closed; `luantu/MacType@f3e926f` has 70 profiles (48 UTF-16 LE, 16 UTF-8, 6 GBK/GB18030)
- corpus codec expansion: closed; all six GBK/GB18030 files are misclassified as Windows-1252 by the current score
- service state expansion: closed; absent, inaccessible, foreign, pending, and marked-for-delete states must remain distinct
- single-instance privilege expansion: closed; the main app must remain `asInvoker`, with privileged service work isolated before Tauri startup
- single-instance race expansion: closed; the plugin has a mutex-to-IPC-window startup gap, requiring a pre-Tauri startup gate and stampede test

## Convergence

No remaining lead changes the implementation boundary. The official installer corpus remains uninspected because doing so would require executing an untrusted third-party unpacker; it is recorded as an evidence limitation rather than silently assumed.
