# Control Center build quick reference

The canonical instructions, including the original MacType build notes, the exact local Tauri/native-service/Inno sequence, and the one-click Actions build, are in [`doc/HOWTOBUILD.md`](doc/HOWTOBUILD.md).

## Prerequisites

- Windows 10 or 11 x64
- Visual Studio 2022 with MSVC x86/x64 desktop tools, CMake, and the Windows SDK
- stable Rust with `rustfmt` and `clippy`
- Node.js 24 and pnpm 11.7.0
- Inno Setup 6 when producing the installer

Velopack is intentionally not used.

## Manual build

From an x64 Visual Studio developer PowerShell:

```powershell
pnpm --dir control-center install --frozen-lockfile
.github/scripts/Build-OpenCore.ps1
.github/scripts/Build-ControlCenter.ps1 -Configuration Release -SkipInstall
.github/scripts/Build-ServiceRuntime.ps1 `
  -CoreRoot artifacts/open-core `
  -OutputRoot artifacts/service-runtime `
  -Version 0.2.0
```

`Build-ControlCenter.ps1` builds the frontend, Preview Helper, and Tauri executable. The stable frontend filename is `artifacts\application\MacType Control Center.exe`. `Build-ServiceRuntime.ps1` builds the Rust host/setup and stages the fixed public x86/x64 helper and DLL payload; it does not install or start the Control Center native service.

Before packaging, run:

```powershell
scripts/ci/Test-DistributionPolicy.ps1
scripts/ci/Test-OpenServiceContract.ps1
scripts/ci/Test-NewCppStyle.ps1
cargo test --manifest-path service-runtime/Cargo.toml --workspace --all-targets --all-features
```

The exact Inno Setup command and artifact layout are maintained in `.github/workflows/build.yml`. Use that workflow as the packaging reference instead of copying a versioned command into multiple documents.

## CI build (“one click”)

1. Open GitHub Actions.
2. Select **Build MacType Control Center**.
3. Choose **Run workflow** (`workflow_dispatch`) on the desired branch.
4. Enter the installer version and download `mactype-control-center-<version>` after both jobs are Green.

The artifact contains the stable-name installer, checksum, Control Center executable, Preview Helper, and staged service payload. This branch has no automatic pull-request, push, merge, or release workflow; the build runs only when a maintainer explicitly dispatches it.

Machine-mutating reboot, AppInit, migration, and multi-session verification is deliberately excluded from this maintainer convenience workflow. Follow `docs/service-maintenance.md` and use a disposable machine when performing those checks; never run them on a maintainer workstation.
