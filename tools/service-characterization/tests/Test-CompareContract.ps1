[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $ProbeDirectory,

    [string] $OutputRoot = (Join-Path ([System.IO.Path]::GetTempPath()) 'mactype-service-compare-contract')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptRoot = Split-Path -Parent $PSScriptRoot
$matrixScript = Join-Path $scriptRoot 'Run-Matrix.ps1'
$compareScript = Join-Path $scriptRoot 'Compare-Results.ps1'
$root = Join-Path $OutputRoot ([guid]::NewGuid().ToString('N'))
$referenceRoot = Join-Path $root 'reference'
$candidateRoot = Join-Path $root 'candidate'
$missingService = "MacTypeContract-$([guid]::NewGuid().ToString('N'))"
$common = @{
    Mode = 'Execute'
    CaseId = 'M08'
    SubjectVersion = 'contract-fixture'
    ProbeDirectory = $ProbeDirectory
    Architecture = 'x64'
    Repetitions = 3
    WaitMilliseconds = 25
    ServiceName = $missingService
    ConfirmPrepared = $true
    ConditionNote = 'Status-machine fixture labels only; this is not service parity evidence.'
}

& $matrixScript @common -EvidenceRoot $referenceRoot -SourceKind Official
if ($LASTEXITCODE -ne 0) { throw 'Reference matrix fixture failed' }
& $matrixScript @common -EvidenceRoot $candidateRoot -SourceKind Open
if ($LASTEXITCODE -ne 0) { throw 'Candidate matrix fixture failed' }

$referenceRun = Get-ChildItem -LiteralPath $referenceRoot -Filter run.json -Recurse |
    Select-Object -ExpandProperty FullName -First 1
$candidateRun = Get-ChildItem -LiteralPath $candidateRoot -Filter run.json -Recurse |
    Select-Object -ExpandProperty FullName -First 1
$comparisonPath = Join-Path $root 'comparison.json'

& $compareScript -ReferenceRun $referenceRun -CandidateRun $candidateRun `
    -OutputFile $comparisonPath
if ($LASTEXITCODE -ne 0) { throw 'Comparison tool failed' }

$comparison = Get-Content -LiteralPath $comparisonPath -Raw | ConvertFrom-Json
if ($comparison.schemaVersion -ne 1 -or $comparison.tool -ne 'Compare-Results') {
    throw 'Comparison output does not expose the versioned public contract'
}
if ($comparison.comparisonStatus -ne 'PARITY_PROVEN') {
    throw "Stable contract fixtures must exercise PARITY_PROVEN, got $($comparison.comparisonStatus)"
}
if (-not $comparison.signaturesMatch -or $comparison.differences.Count -ne 0) {
    throw 'Identical stable run signatures were reported as different'
}
if ($comparison.reference.sha256 -notmatch '^[0-9A-F]{64}$' -or
    $comparison.candidate.sha256 -notmatch '^[0-9A-F]{64}$') {
    throw 'Comparison output must identify both source manifests by SHA-256'
}

Write-Host 'Service characterization comparison contract passed.'
