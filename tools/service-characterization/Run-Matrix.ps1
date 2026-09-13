[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet(
        'M01', 'M02', 'M03', 'M04', 'M05', 'M06', 'M07', 'M08', 'M09', 'M10', 'M11',
        'M12', 'M13', 'M14', 'M15', 'M16', 'M17', 'M18', 'M19', 'M20', 'M21', 'M22'
    )]
    [string[]] $CaseId,

    [ValidateSet('Plan', 'Execute')]
    [string] $Mode = 'Plan',

    [Parameter(Mandatory)]
    [string] $EvidenceRoot,

    [Parameter(Mandatory)]
    [string] $SubjectVersion,

    [ValidateSet('Official', 'Open', 'Unspecified')]
    [string] $SourceKind = 'Unspecified',

    [string] $ProbeDirectory,

    [ValidateSet('Auto', 'x86', 'x64', 'Both')]
    [string] $Architecture = 'Auto',

    [ValidateRange(1, 20)]
    [int] $Repetitions = 3,

    [ValidateRange(0, 600000)]
    [int] $WaitMilliseconds = 3000,

    [string] $ServiceName = 'MacType',

    [switch] $ConfirmPrepared,

    [string] $ConditionNote
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$libraryRoot = Join-Path $PSScriptRoot 'lib'
Import-Module (Join-Path $libraryRoot 'CharacterizationIO.psm1') -Force
Import-Module (Join-Path $libraryRoot 'ProbeHarness.psm1') -Force
$definitions = Import-PowerShellDataFile (Join-Path $libraryRoot 'MatrixCases.psd1')

function ConvertTo-SafePathSegment {
    param([Parameter(Mandatory)] [string] $Value)

    $invalid = [System.IO.Path]::GetInvalidFileNameChars()
    $builder = [System.Text.StringBuilder]::new()
    foreach ($character in $Value.ToCharArray()) {
        if ($invalid -contains $character -or
            $character -eq [System.IO.Path]::DirectorySeparatorChar) {
            [void] $builder.Append('_')
        } else {
            [void] $builder.Append($character)
        }
    }
    $result = $builder.ToString().Trim()
    if ([string]::IsNullOrWhiteSpace($result)) {
        return 'unknown-version'
    }
    return $result
}

$safeVersion = ConvertTo-SafePathSegment $SubjectVersion
$timestamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$runNonce = [guid]::NewGuid().ToString('N').Substring(0, 8)

if ($Mode -eq 'Execute') {
    if (-not $ConfirmPrepared) {
        throw 'Execute mode requires -ConfirmPrepared after the documented precondition is established.'
    }
    if ([string]::IsNullOrWhiteSpace($ConditionNote)) {
        throw 'Execute mode requires -ConditionNote so evidence does not lose its experimental condition.'
    }
    if ([string]::IsNullOrWhiteSpace($ProbeDirectory) -or
        -not (Test-Path -LiteralPath $ProbeDirectory -PathType Container)) {
        throw 'Execute mode requires an existing -ProbeDirectory.'
    }
    $ProbeDirectory = (Resolve-Path -LiteralPath $ProbeDirectory).Path
}

foreach ($id in $CaseId) {
    $definition = $definitions[$id]
    $runDirectory = Join-Path $EvidenceRoot "$safeVersion\$id\$timestamp-$runNonce"
    New-Item -ItemType Directory -Force -Path $runDirectory | Out-Null
    $trialResults = [System.Collections.Generic.List[object]]::new()
    $selectedArchitectures = @(
        Get-SelectedArchitectures -Requested $Architecture `
            -CaseDefault $definition.architecture
    )
    $status = 'UNKNOWN'
    $beforeExitCode = $null
    $afterExitCode = $null

    if ($Mode -eq 'Execute') {
        $beforeExitCode = Get-ServiceQueryCapture -Name $ServiceName `
            -Path (Join-Path $runDirectory 'service-before.txt')
        foreach ($targetArchitecture in $selectedArchitectures) {
            $probeExecutable = Resolve-ProbeExecutable -Root $ProbeDirectory `
                -Kind $definition.probe -TargetArchitecture $targetArchitecture
            $oppositeTree = $null
            if ($definition.probe -eq 'spawn-tree' -and $id -in @('M06', 'M07')) {
                $oppositeArchitecture = if ($targetArchitecture -eq 'x64') {
                    'x86'
                } else {
                    'x64'
                }
                $oppositeTree = Resolve-ProbeExecutable -Root $ProbeDirectory `
                    -Kind 'spawn-tree' -TargetArchitecture $oppositeArchitecture
            }
            for ($iteration = 1; $iteration -le $Repetitions; $iteration++) {
                $trialDirectory = Join-Path $runDirectory `
                    "$targetArchitecture-$('{0:d2}' -f $iteration)"
                New-Item -ItemType Directory -Force -Path $trialDirectory | Out-Null
                $resultPath = Join-Path $trialDirectory 'probe.json'
                $trial = Invoke-ProbeTrial -Executable $probeExecutable `
                    -ProbeKind $definition.probe `
                    -TargetArchitecture $targetArchitecture `
                    -OutputPath $resultPath `
                    -TrialDirectory $trialDirectory `
                    -Wait $WaitMilliseconds `
                    -OppositeTreeExecutable $oppositeTree
                $trial | Add-Member -NotePropertyName iteration `
                    -NotePropertyValue $iteration
                $trialResults.Add($trial)
            }
        }
        $afterExitCode = Get-ServiceQueryCapture -Name $ServiceName `
            -Path (Join-Path $runDirectory 'service-after.txt')
        $validTrials = @($trialResults | Where-Object valid)
        if ($validTrials.Count -gt 0) {
            $status = 'OBSERVED'
        }
        $expectedCount = $selectedArchitectures.Count * $Repetitions
        $stable = $true
        foreach ($targetArchitecture in $selectedArchitectures) {
            $signatures = @(
                $validTrials | Where-Object architecture -eq $targetArchitecture |
                    Select-Object -ExpandProperty signature -Unique
            )
            if ($signatures.Count -ne 1) {
                $stable = $false
            }
        }
        if ($Repetitions -ge 3 -and $validTrials.Count -eq $expectedCount -and
            $stable) {
            $status = 'REPRODUCED'
        }
    }

    $manifest = [ordered]@{
        schemaVersion = 1
        tool = 'Run-Matrix'
        createdAtUtc = [DateTimeOffset]::UtcNow.ToString('o')
        mode = $Mode
        caseId = $id
        sourceKind = $SourceKind
        subjectVersion = $SubjectVersion
        characterizationStatus = $status
        question = $definition.question
        precondition = $definition.precondition
        conditionConfirmed = [bool] $ConfirmPrepared
        conditionNote = if ($ConditionNote) { $ConditionNote } else { $null }
        serviceName = $ServiceName
        serviceQueryBeforeExitCode = $beforeExitCode
        serviceQueryAfterExitCode = $afterExitCode
        probeKind = $definition.probe
        architectures = $selectedArchitectures
        requiredRepetitions = 3
        requestedRepetitions = $Repetitions
        waitMilliseconds = $WaitMilliseconds
        trials = $trialResults
    }
    Write-CharacterizationJson -Path (Join-Path $runDirectory 'run.json') `
        -Value $manifest
    Write-Host "$id $Mode -> $status ($runDirectory)"
}

exit 0
