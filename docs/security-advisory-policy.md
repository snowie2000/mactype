# Rust advisory exceptions

Dependabot alert #1 reports `RUSTSEC-2024-0429` / `GHSA-wrw7-89jp-8q8g` for
`glib 0.18.5`. The affected iterator can dereference a null pointer after an
unsound C out-parameter call.

The package is not linked into either supported MacType Control Center target.
It is resolved only through Tauri's Linux GTK backend, while this program and
its preview helper are Windows-only. Tauri 2.11.5 and its current development
branch still require the GTK 0.18 family, so forcing `glib 0.20` would create an
invalid dependency graph instead of fixing a shipped binary.

`scripts/ci/Test-RustAdvisoryScope.ps1` therefore fails CI if the vulnerable
crate becomes reachable from either `x86_64-pc-windows-msvc` or
`i686-pc-windows-msvc`. The machine-readable exception is in
`security/rust-advisory-exceptions.json` and expires on 2027-01-31. Remove the
exception and update the GTK dependency family as soon as Tauri supports it.

References:

- [GitHub advisory GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g)
- [Upstream glib fix](https://github.com/gtk-rs/gtk-rs-core/pull/1343)
- [Tauri development manifest](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/Cargo.toml)
