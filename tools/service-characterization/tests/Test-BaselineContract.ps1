[CmdletBinding()]
param(
    [string] $OutputRoot = (Join-Path ([System.IO.Path]::GetTempPath()) 'mactype-service-baseline-contract')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptRoot = Split-Path -Parent $PSScriptRoot
$captureScript = Join-Path $scriptRoot 'Capture-ServiceBaseline.ps1'
$outputDirectory = Join-Path $OutputRoot ([guid]::NewGuid().ToString('N'))
$missingService = "MacTypeContract-$([guid]::NewGuid().ToString('N'))"

& $captureScript -OutputDirectory $outputDirectory -ServiceName $missingService -Phase test
if ($LASTEXITCODE -ne 0) {
    throw "Baseline capture returned $LASTEXITCODE for an absent service"
}

$metadataPath = Join-Path $outputDirectory 'baseline.json'
if (-not (Test-Path -LiteralPath $metadataPath -PathType Leaf)) {
    throw 'Baseline capture did not write baseline.json'
}
$metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
if ($metadata.schemaVersion -ne 1 -or $metadata.tool -ne 'Capture-ServiceBaseline') {
    throw 'Baseline metadata does not expose the versioned public contract'
}
if ($metadata.characterizationStatus -ne 'UNKNOWN') {
    throw 'An absent service must remain UNKNOWN rather than being inferred'
}
if ($metadata.serviceName -ne $missingService -or $metadata.phase -ne 'test') {
    throw 'Baseline metadata lost the requested service or phase'
}
if ($metadata.captures.Count -lt 8) {
    throw 'Baseline capture omitted required SCM observations'
}
foreach ($capture in $metadata.captures) {
    if ([string]::IsNullOrWhiteSpace($capture.name) -or
        [string]::IsNullOrWhiteSpace($capture.path) -or
        $null -eq $capture.exitCode) {
        throw 'Every baseline command must have a named artifact and exit code'
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $outputDirectory 'sc-qc.txt'))) {
    throw 'Failed SCM commands must still retain their raw output artifact'
}
if (-not (Test-Path -LiteralPath (Join-Path $outputDirectory 'cim-service.txt')) -or
    -not (Test-Path -LiteralPath (Join-Path $outputDirectory 'cim-service.json'))) {
    throw 'CIM evidence must retain both raw text and machine-readable JSON'
}

Write-Host 'Service baseline contract passed.'
