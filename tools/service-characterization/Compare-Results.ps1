[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $ReferenceRun,

    [Parameter(Mandatory)]
    [string] $CandidateRun,

    [Parameter(Mandatory)]
    [string] $OutputFile,

    [switch] $RequireParity
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Import-Module (Join-Path $PSScriptRoot 'lib\CharacterizationIO.psm1') -Force

function Read-RunManifest {
    param([Parameter(Mandatory)] [string] $Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Run manifest was not found: $Path"
    }
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $manifest = Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json
    if ($manifest.schemaVersion -ne 1 -or $manifest.tool -ne 'Run-Matrix') {
        throw "Unsupported run manifest contract: $resolved"
    }
    if ($manifest.characterizationStatus -notin @(
        'UNKNOWN', 'OBSERVED', 'REPRODUCED', 'OPEN_IMPLEMENTED', 'PARITY_PROVEN'
    )) {
        throw "Invalid characterization status in $resolved"
    }
    return [pscustomobject]@{
        Path = $resolved
        Manifest = $manifest
        Sha256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash
    }
}

function Get-CanonicalSignatures {
    param([Parameter(Mandatory)] $Run)

    $values = [System.Collections.Generic.List[string]]::new()
    foreach ($trial in $Run.Manifest.trials) {
        if (-not $trial.valid -or [string]::IsNullOrWhiteSpace($trial.signature)) {
            continue
        }
        $values.Add("$($trial.architecture)|$($trial.probeKind)|$($trial.signature)")
    }
    return @($values | Sort-Object -Unique)
}

$reference = Read-RunManifest -Path $ReferenceRun
$candidate = Read-RunManifest -Path $CandidateRun
$referenceSignatures = @(Get-CanonicalSignatures -Run $reference)
$candidateSignatures = @(Get-CanonicalSignatures -Run $candidate)
$differences = [System.Collections.Generic.List[object]]::new()

foreach ($signature in $referenceSignatures) {
    if ($signature -notin $candidateSignatures) {
        $differences.Add([ordered]@{ side = 'reference-only'; signature = $signature })
    }
}
foreach ($signature in $candidateSignatures) {
    if ($signature -notin $referenceSignatures) {
        $differences.Add([ordered]@{ side = 'candidate-only'; signature = $signature })
    }
}

$sameCase = $reference.Manifest.caseId -eq $candidate.Manifest.caseId
$signaturesMatch = $sameCase -and $referenceSignatures.Count -gt 0 -and
    $candidateSignatures.Count -gt 0 -and $differences.Count -eq 0
$bothReproduced = $reference.Manifest.characterizationStatus -eq 'REPRODUCED' -and
    $candidate.Manifest.characterizationStatus -eq 'REPRODUCED'

$comparisonStatus = 'UNKNOWN'
if ($referenceSignatures.Count -gt 0 -and $candidateSignatures.Count -gt 0) {
    $comparisonStatus = 'OBSERVED'
    if ($candidate.Manifest.sourceKind -eq 'Open') {
        $comparisonStatus = 'OPEN_IMPLEMENTED'
    }
    if ($signaturesMatch -and $bothReproduced) {
        if ($reference.Manifest.sourceKind -eq 'Official' -and
            $candidate.Manifest.sourceKind -eq 'Open') {
            $comparisonStatus = 'PARITY_PROVEN'
        } else {
            $comparisonStatus = 'REPRODUCED'
        }
    }
}

if (-not $sameCase) {
    $differences.Add([ordered]@{
        side = 'metadata'
        signature = "caseId:$($reference.Manifest.caseId)!=$($candidate.Manifest.caseId)"
    })
}

$comparison = [ordered]@{
    schemaVersion = 1
    tool = 'Compare-Results'
    comparedAtUtc = [DateTimeOffset]::UtcNow.ToString('o')
    comparisonStatus = $comparisonStatus
    caseId = if ($sameCase) { $reference.Manifest.caseId } else { $null }
    sameCase = $sameCase
    signaturesMatch = $signaturesMatch
    reference = [ordered]@{
        path = $reference.Path
        sha256 = $reference.Sha256
        sourceKind = $reference.Manifest.sourceKind
        subjectVersion = $reference.Manifest.subjectVersion
        characterizationStatus = $reference.Manifest.characterizationStatus
        signatures = $referenceSignatures
    }
    candidate = [ordered]@{
        path = $candidate.Path
        sha256 = $candidate.Sha256
        sourceKind = $candidate.Manifest.sourceKind
        subjectVersion = $candidate.Manifest.subjectVersion
        characterizationStatus = $candidate.Manifest.characterizationStatus
        signatures = $candidateSignatures
    }
    differences = $differences
}

$parent = Split-Path -Parent $OutputFile
if ($parent) {
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
}
Write-CharacterizationJson -Path $OutputFile -Value $comparison -Depth 10
Write-Host "Comparison -> $comparisonStatus ($OutputFile)"

if ($RequireParity -and $comparisonStatus -ne 'PARITY_PROVEN') {
    exit 2
}
exit 0
