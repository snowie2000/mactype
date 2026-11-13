# How to build

 1. **Compiler / IDE**

    Visual Studio 2019 with v142 toolkit has been tested and is working. Toolkits down to v120 should be able to compile the code, but be aware that the `_xp` ones might refuse to use the Windows 10 SDK.

 2. **Dependencies**

    Mactype depends on
     - [Freetype](https://www.freetype.org/download.html)
       - For the lastest version of Mactype, a customized version of FreeType is required, which can be obtained from https://github.com/snowie2000/freetype    
     - [EasyHook](http://easyhook.github.io/) / [Detours](https://github.com/microsoft/Detours)
     - [IniParser (fork)](https://github.com/snowie2000/IniParser)
     - [wow64ext (fork)](https://github.com/snowie2000/rewolf-wow64ext)
     - Windows SDK (10.0.14393.0 or later)

 3. **Building dependencies**

    - FreeType

        Apply `glyph_to_bitmapex.diff` before building.

        Always build multi-thread release.

        Remember to enable options you want in ftoptions.h

        Compile freetype as freetype.lib for x86 and freetype64.lib for x64

        Static library is preferred, you are free to build freetype as independent dlls with better interchangeability but you will lose some compatibility in return, for some programs are delivered with their own copies of freetype which will conflict with your file.

        Set `FREETYPE_PATH` environment variable to root of freetype source.

    - iniParser

        Build as iniparser.lib and iniparser64.lib. Set `INI_PARSER_PATH` environment variable to root of IniParser project.

    - wow64ext

        Build as wow64ext.lib. x64 library is not required. Shared library also works if you prefer that.

    - EasyHook

        Only EasyHookDll project is required.

        Build it as easyhook32.lib and easyhook64.lib, or get the binary distributions.

        Dll filename is not important but you'd better give it a special name to avoid dll confliction as stated above. Do not forget to modify filename in `hook.cpp` of MacType.

    - Detours

        Since Microsoft Detours is now free and opensource, it is back to be supported and recommended.

        Follow the official guide to build detours.lib and detours64.lib and put them in the root of MacType.

        Detours lib are static libraries, so name confiction is not a thing.

    - Windows SDK

        Actually it's not something you need to build, but the installation is tricky.

        One word to rule them all: download **ALL COMPONENTS**  in the installation list! Unless you want to waste several hours looking for these mysterious dependencies it pops to you. Don't worry, you will have a second chance to choose which component you want to install after download.

 4. **Build**

    Last but easiest step: Put all `.lib` files you built earlier into a `lib` folder in the root of MacType, click build and enjoy.

## Building the Control Center, native service, and installer

The new application can be built without opening the legacy Visual Studio solution by hand. The maintained scripts build the source-based x86/x64 open core, fixed injectors, 32-bit Preview Helper, React frontend, Tauri executable, native Control Center service, immutable service payload, and Inno Setup installer. They do not modify an installed copy of MacType.

### Prerequisites

- Windows 10 or 11
- Visual Studio 2022 Build Tools with **Desktop development with C++**, MSVC, CMake, and a Windows SDK
- Node.js 24
- pnpm 11.7.0
- the stable Rust MSVC toolchain
- Inno Setup 6 for the installable package
- Microsoft Edge WebView2 Runtime, which is included with supported Windows versions

After installing Node.js and `rustup`, enable the repository's pnpm and Rust versions:

```powershell
corepack enable
corepack prepare pnpm@11.7.0 --activate
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
```

Run the following commands from an x64 Visual Studio 2022 Developer PowerShell. `Build-OpenCore.ps1` downloads pinned public dependencies and builds the x86/x64 core and fixed injectors. `Build-ControlCenter.ps1` builds and tests the Preview Helper, runs the frontend production build, invokes `pnpm tauri build --no-bundle`, and copies the Tauri executable to its stable public filename. `Build-ServiceRuntime.ps1` builds the service host and setup broker and stages the immutable payload used by the installer.

```powershell
pnpm --dir control-center install --frozen-lockfile

pwsh -NoProfile -File .github/scripts/Build-OpenCore.ps1
pwsh -NoProfile -File .github/scripts/Build-ControlCenter.ps1 `
  -Configuration Release `
  -SkipInstall
pwsh -NoProfile -File .github/scripts/Build-ServiceRuntime.ps1 `
  -CoreRoot artifacts/open-core `
  -OutputRoot artifacts/service-runtime `
  -Version 0.2.0
```

The service payload version must have the same base version as the two packages in `service-runtime/Cargo.toml`; the build script rejects a mismatch. Pre-release or build metadata may be appended when an immutable payload identity is needed.

If you are changing only the web interface and want to see the individual Tauri commands, the application portion is equivalent to:

```powershell
Push-Location control-center
pnpm build
pnpm tauri build --no-bundle
Pop-Location
```

Use `Build-ControlCenter.ps1` for a distributable build because it also builds and verifies the required Preview Helper and copies the application to its stable filename.

The build outputs are:

- `artifacts/application/MacType Control Center.exe`
- `build/preview-helper/Release/mactype-preview32.exe`
- `artifacts/open-core/` with the exact source-built core and injector set
- `artifacts/service-runtime/mactype-service-setup.exe`
- `artifacts/service-runtime/payload/` with the service, fixed helpers, DLLs, and hash manifest

### Build the Inno Setup installer locally

After all four build groups above succeed, package the exact same inputs used by the manual Actions workflow:

```powershell
$root = (Resolve-Path .).Path
$appVersion = '0.1.0'

& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' `
  /DAppVersion="$appVersion" `
  /DSourceRoot="$root" `
  /DOutputRoot="$root\artifacts\installer" `
  /DAppExe="$root\artifacts\application\MacType Control Center.exe" `
  /DPreviewExe="$root\build\preview-helper\Release\mactype-preview32.exe" `
  /DCoreRoot="$root\artifacts\open-core" `
  /DServiceRuntimeRoot="$root\artifacts\service-runtime" `
  installer\mactype-control-center.iss
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE." }

$installer = Get-Item 'artifacts\installer\MacType Control Center.exe'
$hash = (Get-FileHash -Algorithm SHA256 $installer).Hash.ToLowerInvariant()
"$hash  $($installer.Name)" | Set-Content -Encoding ascii artifacts\installer\SHA256SUMS.txt
```

This produces the stable-name installer `artifacts/installer/MacType Control Center.exe` and `SHA256SUMS.txt`. The installer contains the native service runtime and bootstraps it transactionally; MacTray is not required for a new installation.

For local testing, launch the Control Center from the repository root so it can find the development copy of the Preview Helper:

```powershell
& '.\artifacts\application\MacType Control Center.exe'
```

The Control Center can discover an existing MacType installation, or let the user select one through the interface.

For frontend-only design work that does not require Rust or native Windows integration, see the [Control Center design maintenance guide](../docs/design-maintenance.md).

## Build an installer with GitHub Actions (one click)

Maintainers do not need to reproduce the open-core, Tauri, Rust service, and installer toolchain locally. The repository contains one manual Windows workflow that creates the complete installable package on GitHub-hosted runners.

1. Open the repository's **Actions** tab.
2. Select **Build MacType Control Center**.
3. Select **Run workflow**.
4. Choose the branch to build.
5. Enter a version such as `0.1.0` or `0.1.0-preview.1`.
6. Select **Run workflow** and wait for both build jobs to finish.
7. Wait for **Build MacType x86/x64 open core** and **Build Control Center, native service, and installer** to turn Green.
8. Open the completed run and download `mactype-control-center-<version>` from **Artifacts**.

The artifact contains the stable-name Inno Setup installer, `SHA256SUMS.txt`, the standalone `MacType Control Center.exe`, the Preview Helper, and the complete staged native-service payload. It is retained for 14 days. The workflow builds the x86/x64 open core and fixed injectors from source before packaging, so no prebuilt core or MacTray files need to be prepared by the maintainer.

The workflow is defined in [`.github/workflows/build.yml`](../.github/workflows/build.yml). It has only a `workflow_dispatch` trigger. Opening, updating, or merging a pull request does not start it, and the workflow never publishes a GitHub Release.

## FAQ

Q: Where are the sources of loader and tuner in the repo?

A: I'm sorry, but they are still closed-source right now. Since you have the mactype source and will surely have a good understanding of how mactype works, I believe it's not a big challenge to write a loader for it.
If you wrote a great loader or something else wonderful, please post an issue or a pull request. Hope we can make MacType better!
