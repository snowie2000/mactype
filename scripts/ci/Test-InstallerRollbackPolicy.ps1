[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$workflow = Get-Content -LiteralPath (Join-Path $root '.github\workflows\build.yml') -Raw
$installerTest = Get-Content -LiteralPath (Join-Path $root 'scripts\ci\Test-InstallerWindows.ps1') -Raw
$installerDefinitionPath = Join-Path $root 'installer\mactype-control-center.iss'
$innoFailureContractPath = Join-Path $root 'scripts\ci\Test-InnoInstallerFailureContract.ps1'
$installerHelperPath = Join-Path $root 'scripts\ci\lib\InstallerWindowsAssertions.ps1'
$snapshotContractPath = Join-Path $root 'scripts\ci\Test-InstallerSnapshotContract.ps1'
$fixturePath = Join-Path $root '.github\scripts\Build-FailingServiceRuntimeFixture.ps1'

foreach ($token in @(
    '$failingRuntimeVersion',
    'artifacts/service-runtime-failing-upgrade',
    'Build-FailingServiceRuntimeFixture.ps1',
    'artifacts/installer-failing-upgrade',
    '-FailingUpgradeInstaller'
)) {
    if (-not $workflow.Contains($token)) {
        throw "Hosted installer CI omits the rollback fixture contract: $token"
    }
}

if (-not (Test-Path -LiteralPath $installerHelperPath -PathType Leaf)) {
    throw 'Installer E2E bounded-process helper is missing.'
}

$installerDefinition = Get-Content -LiteralPath $installerDefinitionPath -Raw
foreach ($token in @(
    'ExecAndLogOutput',
    'ExtractTemporaryFiles',
    'ExtractedCount <> 7',
    'BootstrapBeforeFileInstall',
    'PrepareToInstall',
    'service-runtime.setup-backup',
    'RestoreApplicationBroker',
    'RestoreLegacyTrayStartupAfterBootstrapFailure',
    'BrokerFatalLegacyTrayBlocked',
    'ExecAsOriginalUser',
    '--restore-current-user-legacy-tray-autostart',
    '--control-center-service-broker restore-legacy-tray-autostart',
    '"outcome":"applied"',
    '"reason":"legacy-service"',
    '"reason":"appinit"',
    '"reason":"foreign-open-service"'
)) {
    if (-not $installerDefinition.Contains($token)) {
        throw "Installer fatal-bootstrap classification/rollback contract is missing: $token"
    }
}
$bootstrapCapture = [regex]::Match(
    $installerDefinition,
    '(?ms)^procedure\s+CaptureBrokerOutput\b.*?^end;'
)
if (-not $bootstrapCapture.Success -or
    $bootstrapCapture.Value -notmatch '(?s)"reason":"legacy-tray-mode".*BrokerFatalLegacyTrayBlocked\s*:=\s*True') {
    throw 'Installer does not classify a legacy tray-mode bootstrap blocker as fatal.'
}
$stagedBootstrap = [regex]::Match(
    $installerDefinition,
    '(?ms)^function\s+RunStagedBootstrap\b.*?^end;'
)
if (-not $stagedBootstrap.Success -or
    $stagedBootstrap.Value -notmatch 'if\s+BrokerFatalLegacyTrayBlocked\s+then' -or
    $stagedBootstrap.Value -notmatch 'MacTray tray mode') {
    throw 'Installer does not propagate the legacy tray-mode blocker as an installation failure.'
}
$fixedBrokerCall = [regex]::Match(
    $installerDefinition,
    '(?ms)^procedure\s+RunFixedBrokerOrFail\b.*?^end;'
)
if (-not $fixedBrokerCall.Success) {
    throw 'Installer fixed-broker failure propagation procedure is missing.'
}
foreach ($token in @(
    'ExecAndLogOutput',
    '@CaptureBrokerOutput',
    'BrokerFailure'
)) {
    if (-not $fixedBrokerCall.Value.Contains($token)) {
        throw "Owned uninstall broker failures omit bounded diagnostics: $token"
    }
}
if ($installerDefinition -match 'AfterInstall:\s*BootstrapMachineService' -or
    $installerDefinition -match '(?s)procedure\s+CurStepChanged\b.*?ssPostInstall.*?RunFixedBrokerOrFail') {
    throw 'Installer must complete required bootstrap before the Files phase begins.'
}
$installDeleteSection = [regex]::Match(
    $installerDefinition,
    '(?ms)^\[InstallDelete\]\s*(?<body>.*?)(?=^\[[^]]+\])'
)
if (-not $installDeleteSection.Success -or
    $installDeleteSection.Groups['body'].Value -notmatch '(?m)^Type:\s*filesandordirs;\s*Name:\s*"\{app\}\\service-runtime"\s*$') {
    throw 'Installer must remove the prior app-side runtime only after PrepareToInstall succeeds.'
}
$uninstallDeleteSection = [regex]::Match(
    $installerDefinition,
    '(?ms)^\[UninstallDelete\]\s*(?<body>.*?)(?=^\[[^]]+\]|\z)'
)
if (-not $uninstallDeleteSection.Success -or
    $uninstallDeleteSection.Groups['body'].Value -notmatch '(?m)^Type:\s*dirifempty;\s*Name:\s*"\{app\}"\s*$') {
    throw 'Installer must remove the exact application root when the protected bootstrap made it pre-exist and uninstall leaves it empty.'
}
if ($uninstallDeleteSection.Groups['body'].Value -match '(?im)^Type:\s*filesandordirs;\s*Name:\s*"\{app\}(?:[\\/]|"|\*)') {
    throw 'Installer must never recursively delete the application root or its descendants during final cleanup.'
}
if (-not (Test-Path -LiteralPath $innoFailureContractPath -PathType Leaf) -or
    -not $workflow.Contains('scripts/ci/Test-InnoInstallerFailureContract.ps1')) {
    throw 'Hosted Windows CI does not execute the real Inno required-failure rollback contract.'
}
$innoFailureContract = Get-Content -LiteralPath $innoFailureContractPath -Raw
foreach ($token in @(
    'RaiseException',
    'ExtractTemporaryFiles',
    'service-runtime.setup-backup',
    'staged broker exit code 23',
    'Compile product Inno staging contract',
    'Installation process succeeded.',
    'PrepareToInstall failed:',
    'ExitCode -ne 7',
    'baseline-payload',
    'MacTypeInnoEmptyRootCleanupContract',
    'foreign-marker.txt',
    'Empty-root cleanup fixture left its pre-existing application root behind.',
    'Empty-root cleanup fixture recursively deleted a foreign application-root file.'
)) {
    if (-not $innoFailureContract.Contains($token)) {
        throw "Real Inno regression fixture is missing required RED/GREEN evidence: $token"
    }
}
$installerHelper = Get-Content -LiteralPath $installerHelperPath -Raw
foreach ($token in @(
    'BoundedProcessRunner',
    'InstallerProcessTimeoutMilliseconds',
    'StandardOutput',
    'StandardError',
    'DiagnosticLogPath',
    'Read-InstallerDiagnosticLog'
)) {
    if (-not $installerHelper.Contains($token)) {
        throw "Installer E2E process execution is not bounded with diagnostic capture: $token"
    }
}
if ($installerHelper -match '(?is)Start-Process\b.*?-Wait\b') {
    throw 'Installer E2E must not wait indefinitely with Start-Process -Wait.'
}

if (-not (Test-Path -LiteralPath $fixturePath -PathType Leaf)) {
    throw 'The deliberate failing-upgrade payload builder is missing.'
}
$fixture = Get-Content -LiteralPath $fixturePath -Raw
foreach ($token in @('test-only', 'mactype-service-setup.exe', 'mactype-service.exe', 'Get-FileHash')) {
    if (-not $fixture.Contains($token)) {
        throw "Failing-upgrade fixture is not a valid manifest-preserving start failure: $token"
    }
}

foreach ($token in @(
    '[string] $FailingUpgradeInstaller',
    'Deliberately failing protected upgrade',
    'Assert-BaselineRestoredAfterFailedUpgrade',
    '$baselineApplicationSnapshot',
    '$baselineServiceSnapshot',
    'obsolete-from-prior-version.bin',
    '-ExcludedRoot $serviceRoot',
    'Invoke-InstallerExpectedFailure',
    'Installer diagnostic logs:'
)) {
    if (-not $installerTest.Contains($token)) {
        throw "Installer E2E does not prove automatic rollback at the installer boundary: $token"
    }
}

if (-not (Test-Path -LiteralPath $snapshotContractPath -PathType Leaf)) {
    throw 'Installer immutable app-side snapshot contract test is missing.'
}
foreach ($token in @(
    '-ExcludedRoot $ServiceRoot',
    'Get-TreeSnapshotDifference',
    'Get-BoundedTreeInventory',
    'Wait-PathAbsent'
)) {
    if (-not $installerHelper.Contains($token)) {
        throw "Installer app-side rollback diagnostics omit: $token"
    }
}

foreach ($token in @(
    "-Label 'Owned uninstall'",
    'PendingFileRenameOperations',
    'Owned uninstall left non-runtime application files behind',
    'bounded reboot cleanup registrations'
)) {
    if (-not $installerTest.Contains($token)) {
        throw "Installer E2E does not prove bounded immediate-or-reboot cleanup after the detached Inno uninstall phase: $token"
    }
}
foreach ($token in @('DirectorySeparatorChar', 'AltDirectorySeparatorChar', 'GetRelativePath')) {
    if (-not $installerHelper.Contains($token)) {
        throw "Installer snapshot exclusion is not host-platform path aware: $token"
    }
}
if ($installerHelper -match "TrimEnd\('\\\\'\)" -or
    $installerHelper -match "StartsWith\('\.\.\\\\'") {
    throw 'Installer snapshot exclusion hard-codes a Windows path boundary in cross-platform policy code.'
}

$uploadSteps = [regex]::Matches(
    $workflow,
    '(?ms)^\s+- uses: actions/upload-artifact@[^\r\n]*\r?\n.*?(?=^\s+- (?:uses:|name:)|^  [a-zA-Z0-9_-]+:|\z)'
)
foreach ($step in $uploadSteps) {
    if ($step.Value.Contains('failing-upgrade')) {
        throw 'The deliberate failing-upgrade fixture must never be uploaded as a release artifact.'
    }
}

& $snapshotContractPath

Write-Host 'Installer failed-upgrade rollback policy passed.'
