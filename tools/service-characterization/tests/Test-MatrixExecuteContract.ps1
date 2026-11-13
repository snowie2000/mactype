[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $ProbeDirectory,

    [string] $OutputRoot = (Join-Path ([System.IO.Path]::GetTempPath()) 'mactype-service-matrix-execute-contract')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptRoot = Split-Path -Parent $PSScriptRoot
$matrixScript = Join-Path $scriptRoot 'Run-Matrix.ps1'
$evidenceRoot = Join-Path $OutputRoot ([guid]::NewGuid().ToString('N'))
$missingService = "MacTypeContract-$([guid]::NewGuid().ToString('N'))"

& $matrixScript -Mode Execute -CaseId M08 -EvidenceRoot $evidenceRoot `
    -SubjectVersion 'probe-contract' -SourceKind Unspecified -ProbeDirectory $ProbeDirectory `
    -Architecture x64 -Repetitions 3 -WaitMilliseconds 25 `
    -ServiceName $missingService -ConfirmPrepared `
    -ConditionNote 'Harness contract test; the machine service state is outside this fixture.'
if ($LASTEXITCODE -ne 0) {
    throw "Matrix executor returned $LASTEXITCODE"
}

$manifestPath = Get-ChildItem -LiteralPath $evidenceRoot -Filter run.json -Recurse |
    Select-Object -ExpandProperty FullName -First 1
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.characterizationStatus -ne 'REPRODUCED') {
    throw "Three stable trials must be REPRODUCED, got $($manifest.characterizationStatus)"
}
if ($manifest.trials.Count -ne 3 -or @($manifest.trials | Where-Object valid).Count -ne 3) {
    throw 'Matrix executor did not retain three valid trial results'
}
if (@($manifest.trials | Select-Object -ExpandProperty signature -Unique).Count -ne 1) {
    throw 'Stable marker trials produced different comparison signatures'
}
if (-not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $manifestPath) 'service-before.txt')) -or
    -not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $manifestPath) 'service-after.txt'))) {
    throw 'Matrix executor did not retain before/after service observations'
}

Write-Host 'Service matrix execute contract passed.'

$timeoutFixture = Get-ChildItem -LiteralPath $ProbeDirectory `
    -Filter 'probe-timeout-fixture64.exe' -File -Recurse | Select-Object -First 1
if ($null -eq $timeoutFixture) {
    throw 'The timeout fixture was not found under ProbeDirectory.'
}
$timeoutProbeDirectory = Join-Path $evidenceRoot 'timeout-probes'
New-Item -ItemType Directory -Force -Path $timeoutProbeDirectory | Out-Null
Copy-Item -LiteralPath $timeoutFixture.FullName `
    -Destination (Join-Path $timeoutProbeDirectory 'probe-console64.exe')
$timeoutEvidence = Join-Path $evidenceRoot 'timeout-evidence'
$pwsh = (Get-Process -Id $PID).Path
$timeoutRun = Start-Process -FilePath $pwsh -ArgumentList @(
    '-NoProfile', '-File', $matrixScript,
    '-Mode', 'Execute', '-CaseId', 'M08',
    '-EvidenceRoot', $timeoutEvidence,
    '-SubjectVersion', 'timeout-contract',
    '-SourceKind', 'Unspecified',
    '-ProbeDirectory', $timeoutProbeDirectory,
    '-Architecture', 'x64', '-Repetitions', '1', '-WaitMilliseconds', '0',
    '-ServiceName', $missingService, '-ConfirmPrepared',
    '-ConditionNote', 'timeout-contract'
) -PassThru
if (-not $timeoutRun.WaitForExit(20000)) {
    $timeoutRun.Kill($true)
    throw 'Run-Matrix did not enforce a finite probe deadline.'
}
if ($timeoutRun.ExitCode -ne 0) {
    throw "Run-Matrix timeout fixture returned $($timeoutRun.ExitCode)"
}

$timeoutManifestPath = Get-ChildItem -LiteralPath $timeoutEvidence `
    -Filter run.json -File -Recurse | Select-Object -ExpandProperty FullName -First 1
$timeoutManifest = Get-Content -LiteralPath $timeoutManifestPath -Raw | ConvertFrom-Json
if ($timeoutManifest.characterizationStatus -ne 'UNKNOWN') {
    throw 'A timed-out probe must not produce observation evidence.'
}
$timeoutTrial = @($timeoutManifest.trials)[0]
if ($timeoutTrial.valid -or $timeoutTrial.exitCode -ne 1460 -or
    $timeoutTrial.parseError -notmatch 'timed out') {
    throw 'A timed-out probe must be recorded as an explicit invalid trial.'
}

$liveFixtures = [System.Collections.Generic.List[object]]::new()
Get-ChildItem -LiteralPath $timeoutEvidence -Filter '*.pid' -File -Recurse |
    ForEach-Object {
        $fixturePid = [int] (Get-Content -LiteralPath $_.FullName -Raw)
        $fixtureProcess = Get-Process -Id $fixturePid -ErrorAction SilentlyContinue
        if ($null -ne $fixtureProcess) {
            $liveFixtures.Add($fixtureProcess)
        }
    }
foreach ($fixtureProcess in $liveFixtures) {
    Stop-Process -InputObject $fixtureProcess -Force -ErrorAction SilentlyContinue
}
if ($liveFixtures.Count -ne 0) {
    throw 'Run-Matrix left a timed-out probe descendant running.'
}

Write-Host 'Service matrix timeout contract passed.'
