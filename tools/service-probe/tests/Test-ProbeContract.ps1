[CmdletBinding()]
param(
    [ValidateSet('Win32', 'x64')]
    [string] $Architecture = 'x64',

    [string] $BuildRoot = (Join-Path ([System.IO.Path]::GetTempPath()) 'mactype-service-probe-contract')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-Equal {
    param(
        [Parameter(Mandatory)] $Actual,
        [Parameter(Mandatory)] $Expected,
        [Parameter(Mandatory)] [string] $Message
    )

    if ($Actual -ne $Expected) {
        throw "$Message (expected '$Expected', got '$Actual')"
    }
}

$sourceRoot = Split-Path -Parent $PSScriptRoot
$buildDirectory = Join-Path $BuildRoot $Architecture
$resultDirectory = Join-Path $buildDirectory 'contract-results'
$suffix = if ($Architecture -eq 'x64') { '64' } else { '32' }

& cmake -S $sourceRoot -B $buildDirectory -A $Architecture
if ($LASTEXITCODE -ne 0) {
    throw "CMake configure failed with exit code $LASTEXITCODE"
}

& cmake --build $buildDirectory --config Release --target probe-console probe-window probe-spawn-tree probe-timeout-fixture
if ($LASTEXITCODE -ne 0) {
    throw "CMake build failed with exit code $LASTEXITCODE"
}

New-Item -ItemType Directory -Force $resultDirectory | Out-Null
$resultPath = Join-Path $resultDirectory 'console.json'
$probePath = Join-Path $buildDirectory "Release\probe-console$suffix.exe"
& $probePath --out $resultPath --wait-ms 25
if ($LASTEXITCODE -ne 0) {
    throw "Console probe failed with exit code $LASTEXITCODE"
}

$result = Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
Assert-Equal $result.schemaVersion 1 'Probe JSON schema version changed unexpectedly'
Assert-Equal $result.probeKind 'console' 'Console probe reported the wrong kind'
Assert-Equal $result.architecture $(if ($Architecture -eq 'x64') { 'x64' } else { 'x86' }) 'Probe reported the wrong architecture'
if ($result.sessionId -is [string] -or $null -eq $result.sessionId) {
    throw 'Session ID must be a machine-readable JSON number'
}

if ($result.pid -le 0 -or $result.parentPid -le 0) {
    throw 'Probe must report positive process and parent process IDs'
}

if ($result.renderFingerprint -notmatch '^sha256:[0-9a-f]{64}$') {
    throw "Render fingerprint is not a SHA-256 value: $($result.renderFingerprint)"
}

if ($null -eq $result.modules -or $result.modules.GetType().Name -ne 'Object[]') {
    throw 'Probe must report modules as a JSON array'
}

$requiredProperties = @(
    'mactypeModuleLoaded', 'mactypeModulePath', 'mactypeVersion',
    'versionSource', 'loadObservedAt'
)
foreach ($property in $requiredProperties) {
    if ($property -notin $result.PSObject.Properties.Name) {
        throw "Probe JSON omitted required property '$property'"
    }
}
if ($result.mactypeModuleLoaded -and [string]::IsNullOrWhiteSpace($result.mactypeModulePath)) {
    throw 'A loaded MacType module must report its path'
}

if ([string]::IsNullOrWhiteSpace($result.startedAt) -or [string]::IsNullOrWhiteSpace($result.observedAt)) {
    throw 'Probe timestamps must be populated'
}

Write-Host "Service probe contract passed for $Architecture."

$windowResultPath = Join-Path $resultDirectory 'window.json'
$windowProbePath = Join-Path $buildDirectory "Release\probe-window$suffix.exe"
$windowProcess = Start-Process -FilePath $windowProbePath -ArgumentList @(
    '--out', "`"$windowResultPath`"", '--wait-ms', '25'
) -Wait -PassThru
Assert-Equal $windowProcess.ExitCode 0 'Window probe failed'
$windowResult = Get-Content -LiteralPath $windowResultPath -Raw | ConvertFrom-Json
Assert-Equal $windowResult.probeKind 'window' 'Window probe reported the wrong kind'
Assert-Equal $windowResult.architecture $result.architecture 'Window and console probes disagree on architecture'

$treeResultPath = Join-Path $resultDirectory 'tree.json'
$treeProbePath = Join-Path $buildDirectory "Release\probe-spawn-tree$suffix.exe"
& $treeProbePath --out $treeResultPath --wait-ms 25
if ($LASTEXITCODE -ne 0) {
    throw "Spawn-tree probe failed with exit code $LASTEXITCODE"
}
$treeResult = Get-Content -LiteralPath $treeResultPath -Raw | ConvertFrom-Json
Assert-Equal $treeResult.probeKind 'spawn-tree' 'Spawn-tree manifest reported the wrong kind'
Assert-Equal $treeResult.nodes.Count 3 'Spawn-tree manifest must contain parent, child, and grandchild artifacts'
foreach ($node in $treeResult.nodes) {
    if (-not $node.present) {
        throw "Spawn-tree node artifact was not written: $($node.role)"
    }
    $nodeResult = Get-Content -LiteralPath $node.artifact -Raw | ConvertFrom-Json
    Assert-Equal $nodeResult.probeKind 'spawn-tree-node' "Spawn-tree $($node.role) reported the wrong kind"
    Assert-Equal $nodeResult.role $node.role "Spawn-tree $($node.role) reported the wrong role"
    Assert-Equal $nodeResult.treeLevel $node.level "Spawn-tree $($node.role) reported the wrong level"
}

Write-Host "Window and spawn-tree contracts passed for $Architecture."

$timeoutManifest = Join-Path $resultDirectory 'timeout-tree.json'
$timeoutFixture = Join-Path $buildDirectory "Release\probe-timeout-fixture$suffix.exe"
$timeoutProcess = Start-Process -FilePath $treeProbePath -ArgumentList @(
    '--out', "`"$timeoutManifest`"", '--wait-ms', '0',
    '--child-exe', "`"$timeoutFixture`""
) -PassThru
if (-not $timeoutProcess.WaitForExit(30000)) {
    $timeoutProcess.Kill($true)
    throw 'Spawn-tree launcher exceeded its finite timeout contract.'
}
Assert-Equal $timeoutProcess.ExitCode 4 'A timed-out spawn tree must fail its contract'
$timeoutResult = Get-Content -LiteralPath $timeoutManifest -Raw | ConvertFrom-Json
Assert-Equal $timeoutResult.childLaunched $true 'Timeout fixture was not launched'
Assert-Equal $timeoutResult.childExitCode 1460 'A child timeout must be explicit ERROR_TIMEOUT'

$fixturePidPaths = @(
    "$timeoutManifest.fixture-child.pid",
    "$timeoutManifest.fixture-descendant.pid"
)
$liveFixtureProcesses = [System.Collections.Generic.List[object]]::new()
foreach ($pidPath in $fixturePidPaths) {
    if (-not (Test-Path -LiteralPath $pidPath -PathType Leaf)) {
        throw "Timeout fixture did not write its process ID: $pidPath"
    }
    $fixturePid = [int] (Get-Content -LiteralPath $pidPath -Raw)
    $fixtureProcess = Get-Process -Id $fixturePid -ErrorAction SilentlyContinue
    if ($null -ne $fixtureProcess) {
        $liveFixtureProcesses.Add($fixtureProcess)
    }
}
foreach ($fixtureProcess in $liveFixtureProcesses) {
    Stop-Process -InputObject $fixtureProcess -Force -ErrorAction SilentlyContinue
}
if ($liveFixtureProcesses.Count -ne 0) {
    throw 'Timed-out spawn-tree descendants survived their launcher.'
}

Write-Host "Spawn-tree timeout cleanup contract passed for $Architecture."
