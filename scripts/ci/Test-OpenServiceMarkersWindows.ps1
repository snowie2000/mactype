[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Marker32,

    [Parameter(Mandatory)]
    [string] $Marker64,

    [Parameter(Mandatory)]
    [string] $ExpectedRuntimeRoot
)

$ErrorActionPreference = 'Stop'
$markerWaitMilliseconds = 30000
$resultRoot = Join-Path $env:RUNNER_TEMP "mactype-marker-results-$PID"
New-Item -ItemType Directory -Path $resultRoot -Force | Out-Null
$resolvedRuntimeRoot = (Resolve-Path -LiteralPath $ExpectedRuntimeRoot).Path.TrimEnd('\')

function Test-Marker([string] $Executable, [string] $Architecture) {
    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        throw "Required $Architecture marker target is missing: $Executable"
    }

    $resultPath = Join-Path $resultRoot "$Architecture.json"
    & $Executable --out $resultPath --wait-ms $markerWaitMilliseconds
    if ($LASTEXITCODE -ne 0) {
        throw "$Architecture marker target exited with code $LASTEXITCODE."
    }
    if (-not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
        throw "$Architecture marker target did not write $resultPath."
    }

    $result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($result.schemaVersion -ne 1) { throw "$Architecture marker returned an unsupported schemaVersion." }
    if ($result.probeKind -ne 'console') { throw "$Architecture marker returned probeKind '$($result.probeKind)'." }
    if ($result.architecture -ne $Architecture) { throw "$Architecture marker reported architecture '$($result.architecture)'." }
    if (-not $result.mactypeModuleLoaded) { throw "Open service did not hook the $Architecture marker target." }
    if ([string]::IsNullOrWhiteSpace($result.mactypeModulePath)) { throw "$Architecture marker omitted the loaded MacType module path." }
    if ($result.renderFingerprint -notmatch '^sha256:[0-9a-f]{64}$') { throw "$Architecture marker returned an invalid render fingerprint." }
    if (-not $result.modules -or $result.modules.Count -eq 0) { throw "$Architecture marker returned no module inventory." }

    $expectedModule = if ($Architecture -eq 'x86') { 'MacType.dll' } else { 'MacType64.dll' }
    if ([System.IO.Path]::GetFileName([string]$result.mactypeModulePath) -ne $expectedModule) {
        throw "$Architecture marker loaded '$($result.mactypeModulePath)' instead of $expectedModule."
    }

    $resolvedModulePath = (Resolve-Path -LiteralPath ([string]$result.mactypeModulePath)).Path
    $resolvedModuleRoot = [System.IO.Path]::GetDirectoryName($resolvedModulePath).TrimEnd('\')
    if (-not $resolvedModuleRoot.Equals($resolvedRuntimeRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Architecture marker loaded MacType from '$resolvedModuleRoot' instead of the active protected runtime '$resolvedRuntimeRoot'."
    }

    Write-Host "$Architecture marker was hooked by $expectedModule."
    return [pscustomobject]@{
        architecture = $Architecture
        pid = [uint32] $result.pid
        sessionId = [uint32] $result.sessionId
        modulePath = $resolvedModulePath
    }
}

$results = @(
    Test-Marker -Executable $Marker32 -Architecture 'x86'
    Test-Marker -Executable $Marker64 -Architecture 'x64'
)
Write-Host 'Open service x86/x64 marker hook contract passed.'
$results
