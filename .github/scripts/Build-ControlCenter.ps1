[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')]
    [string] $Configuration = 'Release',
    [switch] $SkipInstall
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$helperBuild = Join-Path $root 'build\preview-helper'
$helperOutput = Join-Path $helperBuild "$Configuration\mactype-preview32.exe"

cmake -S (Join-Path $root 'preview-helper') -B $helperBuild -A Win32 -DBUILD_TESTING=ON
if ($LASTEXITCODE -ne 0) { throw "Preview helper configuration failed with exit code $LASTEXITCODE." }
cmake --build $helperBuild --config $Configuration --parallel
if ($LASTEXITCODE -ne 0) { throw "Preview helper build failed with exit code $LASTEXITCODE." }
ctest --test-dir $helperBuild -C $Configuration --output-on-failure
if ($LASTEXITCODE -ne 0) { throw "Preview helper tests failed with exit code $LASTEXITCODE." }

Push-Location (Join-Path $root 'control-center')
try {
    if (-not $SkipInstall) {
        pnpm install --frozen-lockfile
        if ($LASTEXITCODE -ne 0) { throw "Frontend dependency installation failed with exit code $LASTEXITCODE." }
    }
    pnpm build
    if ($LASTEXITCODE -ne 0) { throw "Frontend build failed with exit code $LASTEXITCODE." }
    pnpm tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed with exit code $LASTEXITCODE." }
}
finally {
    Pop-Location
}

$compiledApp = Join-Path $root 'control-center\src-tauri\target\release\mactype-control-center.exe'
if ($Configuration -eq 'Debug') {
    $compiledApp = Join-Path $root 'control-center\src-tauri\target\debug\mactype-control-center.exe'
}
if (-not (Test-Path -LiteralPath $compiledApp)) { throw "Tauri executable missing: $compiledApp" }
if (-not (Test-Path -LiteralPath $helperOutput)) { throw "Preview helper missing: $helperOutput" }

$applicationOutput = Join-Path $root 'artifacts\application'
New-Item -ItemType Directory -Path $applicationOutput -Force | Out-Null
$app = Join-Path $applicationOutput 'MacType Control Center.exe'
Copy-Item -LiteralPath $compiledApp -Destination $app -Force

[pscustomobject]@{ App = $app; PreviewHelper = $helperOutput }
