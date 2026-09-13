[CmdletBinding()]
param(
    [string] $OutputRoot = (Join-Path ([System.IO.Path]::GetTempPath()) 'mactype-service-matrix-contract')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptRoot = Split-Path -Parent $PSScriptRoot
$matrixScript = Join-Path $scriptRoot 'Run-Matrix.ps1'
$evidenceRoot = Join-Path $OutputRoot ([guid]::NewGuid().ToString('N'))

& $matrixScript -Mode Plan -CaseId M01, M22 -EvidenceRoot $evidenceRoot `
    -SubjectVersion 'UNOBSERVED' -SourceKind Official
if ($LASTEXITCODE -ne 0) {
    throw "Matrix planner returned $LASTEXITCODE"
}

$manifests = @(Get-ChildItem -LiteralPath $evidenceRoot -Filter run.json -Recurse)
if ($manifests.Count -ne 2) {
    throw "Expected two planned run manifests, found $($manifests.Count)"
}
$plans = @($manifests | ForEach-Object {
    Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json
})
if (($plans.caseId | Sort-Object) -join ',' -ne 'M01,M22') {
    throw 'Matrix planner did not preserve the requested case IDs'
}
foreach ($plan in $plans) {
    if ($plan.schemaVersion -ne 1 -or $plan.tool -ne 'Run-Matrix') {
        throw 'Matrix plan does not expose a versioned public contract'
    }
    if ($plan.characterizationStatus -ne 'UNKNOWN' -or $plan.mode -ne 'Plan') {
        throw 'Unexecuted matrix plans must remain UNKNOWN'
    }
    if ($plan.requiredRepetitions -ne 3 -or $plan.trials.Count -ne 0) {
        throw 'Matrix plan must require three repetitions and contain no fabricated trials'
    }
    if ([string]::IsNullOrWhiteSpace($plan.question) -or
        [string]::IsNullOrWhiteSpace($plan.precondition)) {
        throw 'Every matrix case must explain its question and precondition'
    }
}

Write-Host 'Service matrix plan contract passed.'
