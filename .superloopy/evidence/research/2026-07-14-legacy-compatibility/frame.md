# Research frame

Core question: How can the Control Center prove byte-preserving compatibility with real MacType profiles, import them safely, control the legacy service through its real SCM contract, and enforce one running instance?

## Axes

1. Official release profile corpus: locate the provenance-pinned INI payload, inspect bytes and encoding, and define a stable CI fixture contract.
2. Control Center code: identify the profile import/save public interface, execution status model, UI navigation, CI smoke seams, and testable behavior.
3. Legacy service: determine the service identity, binary command line, lifecycle, privilege requirements, and safe install/remove sequence from primary evidence.
4. Single instance: determine the supported Tauri 2 integration, plugin ordering, second-launch callback, window restoration behavior, and CI proof.

Codebase relevant: yes
External: yes
Browsing: yes
Verification likely: yes
Report requested: no; synthesis is an implementation evidence artifact

## Constraints

- Do not modify upstream MacType core sources or project files.
- Round-trip compatibility must be proved in CI before import UX work begins.
- Registry injection remains read-only; legacy service install/remove is in scope.
- UI must not require users to type file paths.
