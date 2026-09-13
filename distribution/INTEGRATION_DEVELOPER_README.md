# MacType Control Center Integration/Developer Bundle

This bundle is for MacType maintainers, installer integrators, and Control Center developers. It is a directly usable development and integration layout, not the final-user installation artifact.

`installation-tree/` reproduces the complete application directory written by the standalone MacType Control Center installer. Keep the tree intact when integrating it into another installer:

- `MacType Control Center.exe`
- `mactype-preview32.exe`
- the x86/x64 MacType core and loader files
- `service-runtime/mactype-service-setup.exe`
- `service-runtime/payload/manifest.json`
- every file named by the runtime manifest
- profiles, language catalogs, and license notices

When this tree stays complete, `MacType Control Center.exe` can install and maintain the service directly from any directory. Every machine change still requires explicit UAC consent. The Control Center accepts only fixed service actions and does not accept arbitrary executable, DLL, service, or payload paths.

The Control Center first delegates to a complete registered installation when one is available. Otherwise it uses the current bundle after checking the executable, setup broker, runtime manifest, exact payload file set, manifest hashes, bounded file sizes, canonical paths, and reparse-point policy. The elevated process repeats the current-bundle validation before receiving profile data or changing machine state. A detached EXE or damaged bundle leaves service operations read-only without requesting UAC or attempting rollback.

An integrating installer must:

1. Install the complete `installation-tree/` contents under one protected Program Files application root.
2. Preserve the relative paths in this bundle.
3. Write that application root to the `InstallLocation` string value under `HKLM64\SOFTWARE\MacType\ControlCenter`.
4. Prevent unprivileged users from replacing the installed root or any required file.
5. Remove or update the receipt transactionally with the installed tree.

The final-user installer still has a separate fixed-layout contract. It canonicalizes the registered root, rejects reparse points, confirms that it remains below Program Files, and writes the protected installation receipt. Direct development-bundle execution does not weaken or change that installer policy.
