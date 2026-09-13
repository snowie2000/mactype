[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $BaselineInstaller,
    [Parameter(Mandatory)]
    [string] $FailingUpgradeInstaller,
    [Parameter(Mandatory)]
    [string] $Installer
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
. (Join-Path $PSScriptRoot 'lib\InstallerWindowsAssertions.ps1')

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$resolvedBaselineInstaller = (Resolve-Path -LiteralPath $BaselineInstaller).Path
$resolvedFailingUpgradeInstaller = (Resolve-Path -LiteralPath $FailingUpgradeInstaller).Path
$resolvedInstaller = (Resolve-Path -LiteralPath $Installer).Path
$applicationRoot = [IO.Path]::GetFullPath((Join-Path $env:ProgramFiles 'MacType Control Center')).TrimEnd('\')
$serviceRoot = Join-Path $applicationRoot 'Service'
$profileRoot = [IO.Path]::GetFullPath((Join-Path $env:ProgramData 'MacType\ControlCenter')).TrimEnd('\')
$distributionDefaultProfilePath = Join-Path $root 'distribution\ini\Default.ini'
$openServiceName = 'MacTypeControlCenter'
$legacyServiceName = 'MacType'
$commonDesktopShortcut = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::CommonDesktopDirectory)) 'MacType Control Center.lnk'
$invalidInstallRoot = Join-Path $env:RUNNER_TEMP ("mactype-forbidden-dir-" + [Guid]::NewGuid().ToString('N'))
$userMarkerRoot = Join-Path $env:LOCALAPPDATA ("MacType\ControlCenter\installer-preservation-" + [Guid]::NewGuid().ToString('N'))
$userMarkers = [ordered]@{
    'theme.txt' = 'dark'
    'locale.txt' = 'zh-TW'
    'recent-profile.txt' = 'C:\Users\Example\Recent.ini'
    'applied-profile.txt' = 'C:\Users\Example\Applied.ini'
    'profiles\existing.ini' = "[General]`r`nGammaValue=1.4`r`n"
}
$silentArguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-')
$cleanHostedRunnerConfirmed = $false
$openFixtureCreated = $false
$legacyFixtureCreated = $false
$installerTouchedMachine = $false
$installerDiagnosticRoot = Join-Path $env:RUNNER_TEMP `
    ("mactype-installer-logs-" + [Guid]::NewGuid().ToString('N'))
$installerDiagnosticLogs = [Collections.Generic.List[string]]::new()
$installerDiagnosticSequence = 0

function New-InstallerDiagnosticLog {
    param([Parameter(Mandatory)] [string] $Label)

    $script:installerDiagnosticSequence += 1
    $safeLabel = [regex]::Replace($Label.ToLowerInvariant(), '[^a-z0-9]+', '-').Trim('-')
    $path = Join-Path $script:installerDiagnosticRoot `
        ('{0:D2}-{1}.log' -f $script:installerDiagnosticSequence, $safeLabel)
    [void] $script:installerDiagnosticLogs.Add($path)
    return $path
}

function Invoke-InstallerExpectedSuccess {
    param(
        [Parameter(Mandatory)] [string] $File,
        [Parameter(Mandatory)] [string[]] $Arguments,
        [Parameter(Mandatory)] [string] $Label
    )

    Invoke-ExpectedSuccess -File $File -Arguments $Arguments -Label $Label `
        -DiagnosticLogPath (New-InstallerDiagnosticLog -Label $Label)
}

function Invoke-InstallerExpectedFailure {
    param(
        [Parameter(Mandatory)] [string] $File,
        [Parameter(Mandatory)] [string[]] $Arguments,
        [Parameter(Mandatory)] [string] $Label
    )

    Invoke-ExpectedFailure -File $File -Arguments $Arguments -Label $Label `
        -DiagnosticLogPath (New-InstallerDiagnosticLog -Label $Label)
}

try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if ($env:GITHUB_ACTIONS -cne 'true' -or -not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Installer integration tests require a clean elevated GitHub-hosted Windows runner.'
    }
    foreach ($path in @($applicationRoot, $profileRoot, $commonDesktopShortcut)) {
        if (Test-Path -LiteralPath $path) { throw "Hosted runner is not clean at fixed test path: $path" }
    }
    foreach ($name in @($openServiceName, $legacyServiceName)) {
        if (Get-FixedService -Name $name) { throw "Hosted runner already contains protected test service name: $name" }
    }
    $cleanHostedRunnerConfirmed = $true
    New-Item -ItemType Directory -Path $installerDiagnosticRoot -Force | Out-Null

    Initialize-UserMarkers -UserMarkerRoot $userMarkerRoot -UserMarkers $userMarkers
    Assert-UserMarkers -UserMarkerRoot $userMarkerRoot -UserMarkers $userMarkers

    Invoke-InstallerExpectedFailure -File $resolvedBaselineInstaller -Arguments ($silentArguments + "/DIR=$invalidInstallRoot") -Label 'Arbitrary-directory install'
    if ((Test-Path -LiteralPath $invalidInstallRoot) -or (Test-Path -LiteralPath $applicationRoot) -or (Get-FixedService -Name $openServiceName)) {
        throw 'Rejected /DIR attempt mutated files or SCM.'
    }

    New-ForeignService -Name $openServiceName -DisplayName 'CI foreign fixed-name service'
    $openFixtureCreated = $true
    $foreignSnapshot = Get-ServiceSnapshot -Name $openServiceName
    Invoke-InstallerExpectedSuccess -File $resolvedInstaller -Arguments ($silentArguments + '/TASKS=!desktopicon') -Label 'Install with foreign fixed-name service'
    if ((Get-ServiceSnapshot -Name $openServiceName) -cne $foreignSnapshot -or (Test-Path -LiteralPath $serviceRoot)) {
        throw 'SkippedBlocked foreign-service install mutated the foreign service or runtime.'
    }
    $foreignUninstaller = Get-ChildItem -LiteralPath $applicationRoot -File -Filter 'unins*.exe' | Select-Object -First 1
    Invoke-InstallerExpectedSuccess -File $foreignUninstaller.FullName -Arguments $silentArguments -Label 'Uninstall beside foreign fixed-name service'
    if ((Get-ServiceSnapshot -Name $openServiceName) -cne $foreignSnapshot) {
        throw 'Uninstall changed or removed the foreign fixed-name service.'
    }
    Remove-TestService -Name $openServiceName
    $openFixtureCreated = $false

    New-ForeignService -Name $legacyServiceName -DisplayName 'CI legacy MacTray service'
    $legacyFixtureCreated = $true
    $legacySnapshot = Get-ServiceSnapshot -Name $legacyServiceName
    Invoke-InstallerExpectedSuccess -File $resolvedInstaller -Arguments ($silentArguments + '/TASKS=!desktopicon') -Label 'Install with legacy service conflict'
    if ((Get-ServiceSnapshot -Name $legacyServiceName) -cne $legacySnapshot -or (Get-FixedService -Name $openServiceName) -or (Test-Path -LiteralPath $serviceRoot)) {
        throw 'SkippedBlocked legacy-service install mutated legacy state or installed the open service.'
    }
    $legacyUninstaller = Get-ChildItem -LiteralPath $applicationRoot -File -Filter 'unins*.exe' | Select-Object -First 1
    Invoke-InstallerExpectedSuccess -File $legacyUninstaller.FullName -Arguments $silentArguments -Label 'Uninstall beside legacy service'
    if ((Get-ServiceSnapshot -Name $legacyServiceName) -cne $legacySnapshot) {
        throw 'Uninstall changed or removed the legacy service.'
    }
    Remove-TestService -Name $legacyServiceName
    $legacyFixtureCreated = $false

    $installerTouchedMachine = $true
    Invoke-InstallerExpectedSuccess -File $resolvedBaselineInstaller -Arguments ($silentArguments + '/TASKS=desktopicon') -Label 'Protected baseline install'
    Assert-RequiredApplicationFiles -ApplicationRoot $applicationRoot
    $payload = Assert-ServicePayload -ApplicationRoot $applicationRoot
    $baseline = Assert-ReadyOpenService `
        -PayloadManifest $payload `
        -OpenServiceName $openServiceName `
        -ServiceRoot $serviceRoot `
        -ProfileRoot $profileRoot `
        -DistributionDefaultProfilePath $distributionDefaultProfilePath
    $obsoleteRuntimePath = Join-Path $applicationRoot 'service-runtime\obsolete-from-prior-version.bin'
    [IO.File]::WriteAllText(
        $obsoleteRuntimePath,
        'obsolete app-side runtime payload',
        [Text.UTF8Encoding]::new($false)
    )
    $baselineApplicationSnapshot = Get-TreeSnapshot `
        -Path $applicationRoot `
        -ExcludedRoot $serviceRoot
    $baselineServiceSnapshot = Get-ServiceSnapshot -Name $openServiceName
    $baselineRuntimeSnapshot = Get-TreeSnapshot -Path (Join-Path $serviceRoot ("bin\" + $baseline.RuntimeVersion))
    Assert-CommonDesktopShortcut -CommonDesktopShortcut $commonDesktopShortcut -ApplicationRoot $applicationRoot
    Assert-UserMarkers -UserMarkerRoot $userMarkerRoot -UserMarkers $userMarkers

    Invoke-InstallerExpectedFailure -File $resolvedFailingUpgradeInstaller -Arguments ($silentArguments + '/TASKS=desktopicon') -Label 'Deliberately failing protected upgrade'
    Assert-BaselineRestoredAfterFailedUpgrade `
        -Baseline $baseline `
        -BaselineApplicationSnapshot $baselineApplicationSnapshot `
        -BaselineServiceSnapshot $baselineServiceSnapshot `
        -BaselineRuntimeSnapshot $baselineRuntimeSnapshot `
        -ApplicationRoot $applicationRoot `
        -OpenServiceName $openServiceName `
        -ServiceRoot $serviceRoot `
        -ProfileRoot $profileRoot
    $brokerAfterFailure = Join-Path $applicationRoot 'service-runtime\mactype-service-setup.exe'
    Invoke-ExpectedSuccess -File $brokerAfterFailure -Arguments @('start') -Label 'Protected broker after failed upgrade'
    Assert-BaselineRestoredAfterFailedUpgrade `
        -Baseline $baseline `
        -BaselineApplicationSnapshot $baselineApplicationSnapshot `
        -BaselineServiceSnapshot $baselineServiceSnapshot `
        -BaselineRuntimeSnapshot $baselineRuntimeSnapshot `
        -ApplicationRoot $applicationRoot `
        -OpenServiceName $openServiceName `
        -ServiceRoot $serviceRoot `
        -ProfileRoot $profileRoot
    Assert-CommonDesktopShortcut -CommonDesktopShortcut $commonDesktopShortcut -ApplicationRoot $applicationRoot
    Assert-UserMarkers -UserMarkerRoot $userMarkerRoot -UserMarkers $userMarkers

    Invoke-InstallerExpectedSuccess -File $resolvedInstaller -Arguments ($silentArguments + '/TASKS=desktopicon') -Label 'Protected upgrade install'
    Assert-RequiredApplicationFiles -ApplicationRoot $applicationRoot
    $payload = Assert-ServicePayload -ApplicationRoot $applicationRoot
    if (Test-Path -LiteralPath $obsoleteRuntimePath) {
        throw 'Successful upgrade retained an obsolete app-side service runtime file.'
    }
    $upgrade = Assert-ReadyOpenService `
        -PayloadManifest $payload `
        -OpenServiceName $openServiceName `
        -ServiceRoot $serviceRoot `
        -ProfileRoot $profileRoot `
        -DistributionDefaultProfilePath $distributionDefaultProfilePath
    if ($upgrade.RuntimeVersion -ceq $baseline.RuntimeVersion) {
        throw 'Upgrade reused an immutable runtime version instead of publishing a new generation.'
    }
    if ($upgrade.ActivePointerBytes -cne $baseline.ActivePointerBytes -or
        $upgrade.ActiveGeneration -cne $baseline.ActiveGeneration -or
        $upgrade.ProfileGenerationSnapshot -cne $baseline.ProfileGenerationSnapshot) {
        throw 'Upgrade changed the protected active profile or its generation bytes.'
    }
    Assert-CommonDesktopShortcut -CommonDesktopShortcut $commonDesktopShortcut -ApplicationRoot $applicationRoot
    Assert-UserMarkers -UserMarkerRoot $userMarkerRoot -UserMarkers $userMarkers

    $uninstaller = Get-ChildItem -LiteralPath $applicationRoot -File -Filter 'unins*.exe' | Select-Object -First 1
    if (-not $uninstaller) { throw 'Uninstaller was not created.' }
    $broker = Join-Path $applicationRoot 'service-runtime\mactype-service-setup.exe'
    $disabledBroker = "$broker.disabled-for-ci"
    $serviceBeforeFailedUninstall = Get-ServiceSnapshot -Name $openServiceName
    Move-Item -LiteralPath $broker -Destination $disabledBroker
    try {
        Invoke-InstallerExpectedFailure -File $uninstaller.FullName -Arguments $silentArguments -Label 'Uninstall with missing protected broker'
    }
    finally {
        if (Test-Path -LiteralPath $disabledBroker -PathType Leaf) {
            Move-Item -LiteralPath $disabledBroker -Destination $broker
        }
    }
    if ((Get-ServiceSnapshot -Name $openServiceName) -cne $serviceBeforeFailedUninstall -or
        -not (Test-Path -LiteralPath $applicationRoot -PathType Container) -or
        -not (Test-Path -LiteralPath $serviceRoot -PathType Container)) {
        throw 'Failed uninstall hid or removed an owned service/runtime orphan.'
    }

    Invoke-InstallerExpectedSuccess -File $uninstaller.FullName -Arguments $silentArguments -Label 'Owned uninstall'
    if (Get-FixedService -Name $openServiceName) { throw 'Owned uninstall left the open service registered.' }
    if (Test-Path -LiteralPath $applicationRoot) {
        $unexpectedApplicationEntries = @(
            Get-ChildItem -LiteralPath $applicationRoot -Force |
                Where-Object { $_.FullName -cne $serviceRoot }
        )
        if ($unexpectedApplicationEntries.Count -ne 0) {
            $remainingApplicationTree = Get-BoundedTreeInventory -Path $applicationRoot
            throw "Owned uninstall left non-runtime application files behind.`nRemaining tree:`n$remainingApplicationTree"
        }
        $pendingDeletes = @(
            (Get-ItemProperty `
                -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' `
                -Name PendingFileRenameOperations `
                -ErrorAction SilentlyContinue).PendingFileRenameOperations
        ) -join "`n"
        if ($pendingDeletes -notmatch [regex]::Escape($serviceRoot) -or
            $pendingDeletes -notmatch [regex]::Escape($applicationRoot)) {
            $remainingApplicationTree = Get-BoundedTreeInventory -Path $applicationRoot
            throw "Owned uninstall left a runtime tree without bounded reboot cleanup registrations.`nRemaining tree:`n$remainingApplicationTree"
        }
    }
    if (Test-Path -LiteralPath $commonDesktopShortcut) { throw 'Owned uninstall left the common desktop shortcut behind.' }
    if ([Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $profileRoot 'active.json'))) -cne $baseline.ActivePointerBytes -or
        (Get-TreeSnapshot -Path $baseline.ProfileGenerationRoot) -cne $baseline.ProfileGenerationSnapshot) {
        throw 'Owned uninstall removed or changed the protected ProgramData profile.'
    }
    Assert-UserMarkers -UserMarkerRoot $userMarkerRoot -UserMarkers $userMarkers

    Assert-UserMarkers -UserMarkerRoot $userMarkerRoot -UserMarkers $userMarkers
    Write-Host 'PASS: fixed Program Files install, strict Ready, profile-preserving upgrade, visible failure, exact-owned or reboot-deferred uninstall, and blocked conflict preservation'
}
catch {
    $diagnostics = @(
        foreach ($path in $installerDiagnosticLogs) {
            "===== $path ====="
            Read-InstallerDiagnosticLog -Path $path
        }
    ) -join "`n"
    throw [InvalidOperationException]::new(
        "$($_.Exception.Message)`nInstaller diagnostic logs:`n$diagnostics",
        $_.Exception
    )
}
finally {
    if ($cleanHostedRunnerConfirmed) {
        if ($openFixtureCreated) { Remove-TestService -Name $openServiceName }
        if ($legacyFixtureCreated) { Remove-TestService -Name $legacyServiceName }
        $installedOpenService = Get-FixedService -Name $openServiceName
        $installedImage = if ($installedOpenService) { Get-ServiceExecutablePath -ImagePath $installedOpenService.PathName } else { '' }
        if ($installedOpenService -and $installerTouchedMachine -and $installedImage.StartsWith($serviceRoot, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-TestService -Name $openServiceName
        }
        if (Test-Path -LiteralPath $applicationRoot) {
            $uninstaller = Get-ChildItem -LiteralPath $applicationRoot -File -Filter 'unins*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($uninstaller) { [void](Invoke-ProcessExit -File $uninstaller.FullName -Arguments $silentArguments) }
        }
        foreach ($path in @($applicationRoot, $profileRoot, $invalidInstallRoot, $userMarkerRoot)) {
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue }
        }
        if (Test-Path -LiteralPath $commonDesktopShortcut) {
            Remove-Item -LiteralPath $commonDesktopShortcut -Force -ErrorAction SilentlyContinue
        }
        if (Test-Path -LiteralPath $installerDiagnosticRoot) {
            Remove-Item -LiteralPath $installerDiagnosticRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
