[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$builder = Get-Content -LiteralPath (Join-Path $root '.github\scripts\Build-ServiceRuntime.ps1') -Raw
$hostBuilder = Get-Content -LiteralPath (Join-Path $root 'service-runtime\host\build.rs') -Raw
$hostScm = Get-Content -LiteralPath (Join-Path $root 'service-runtime\host\src\scm.rs') -Raw
$workflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\build.yml') -Raw

foreach ($token in @(
    "-split '[+-]', 2",
    '$packageBaseVersion',
    'runtime package base version',
    '(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?'
)) {
    if (-not $builder.Contains($token)) {
        throw "Service runtime builder does not preserve immutable SemVer generations safely: $token"
    }
}

foreach ($token in @(
    'MACTYPE_SERVICE_RUNTIME_VERSION',
    '$env:MACTYPE_SERVICE_RUNTIME_VERSION = $Version',
    'finally'
)) {
    if (-not $builder.Contains($token)) {
        throw "Service runtime builder does not bind the host binary to the payload generation: $token"
    }
}

foreach ($token in @(
    'cargo:rerun-if-env-changed=MACTYPE_SERVICE_RUNTIME_VERSION',
    'MACTYPE_COMPILED_SERVICE_RUNTIME_VERSION',
    'CARGO_PKG_VERSION',
    '!matches!(version, "." | "..")'
)) {
    if (-not $hostBuilder.Contains($token)) {
        throw "Service host build contract does not preserve the requested generation or development fallback: $token"
    }
}

if (($hostScm | Select-String -Pattern 'env!\("CARGO_PKG_VERSION"\)' -AllMatches).Matches.Count -ne 0 -or
    ($hostScm | Select-String -Pattern 'service_runtime_version\(\)' -AllMatches).Matches.Count -lt 2) {
    throw 'Service health does not consistently report the compiled payload generation.'
}

foreach ($token in @(
    '${{ github.run_id }}',
    '${{ github.sha }}',
    '0.2.0+ci.',
    'artifacts/service-runtime-baseline',
    'artifacts/service-runtime-failing-upgrade',
    'artifacts/service-runtime-current',
    '$baselineRuntimeVersion',
    '$failingRuntimeVersion',
    '$currentRuntimeVersion'
)) {
    if (-not $workflow.Contains($token)) {
        throw "Main build does not bind service payload generations to run and commit identity: $token"
    }
}

if ($workflow -notmatch '(?s)Build-ServiceRuntime\.ps1[^\r\n]*service-runtime-baseline[^\r\n]*baselineRuntimeVersion' -or
    $workflow -notmatch '(?s)Build-ServiceRuntime\.ps1[^\r\n]*service-runtime-current[^\r\n]*currentRuntimeVersion') {
    throw 'Installer baseline and current payloads are not built as distinct immutable generations.'
}

Write-Host 'Service payload immutable-version policy passed.'
