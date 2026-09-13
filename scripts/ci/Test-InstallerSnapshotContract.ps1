[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\InstallerWindowsAssertions.ps1')

$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) `
    "mactype-installer-snapshot-$PID-$([Guid]::NewGuid().ToString('N'))"
$applicationRoot = Join-Path $temporaryRoot 'MacType Control Center'
$serviceRoot = Join-Path $applicationRoot 'Service'
$applicationPath = Join-Path $applicationRoot 'MacType Control Center.exe'
$brokerPath = Join-Path (Join-Path $applicationRoot 'service-runtime') 'mactype-service-setup.exe'
$healthPath = Join-Path $serviceRoot 'health.json'

try {
    New-Item -ItemType Directory -Path (Split-Path -Parent $brokerPath), $serviceRoot -Force | Out-Null
    [IO.File]::WriteAllText($applicationPath, 'baseline-app', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($brokerPath, 'baseline-broker', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($healthPath, 'baseline-health', [Text.UTF8Encoding]::new($false))

    $baseline = Get-TreeSnapshot -Path $applicationRoot -ExcludedRoot $serviceRoot

    [IO.File]::WriteAllText($healthPath, 'restarted-health', [Text.UTF8Encoding]::new($false))
    $candidatePath = Join-Path (Join-Path (Join-Path $serviceRoot 'bin') '0.3.0') `
        'mactype-service.exe'
    New-Item -ItemType Directory -Path (Split-Path -Parent $candidatePath) -Force | Out-Null
    [IO.File]::WriteAllText($candidatePath, 'failed-candidate', [Text.UTF8Encoding]::new($false))

    $afterServiceMutation = Get-TreeSnapshot -Path $applicationRoot -ExcludedRoot $serviceRoot
    if ($afterServiceMutation -cne $baseline) {
        throw 'Protected service state leaked into the immutable app-side snapshot.'
    }

    $expectedHash = (Get-FileHash -LiteralPath $applicationPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($applicationPath, 'changed-app', [Text.UTF8Encoding]::new($false))
    $actualHash = (Get-FileHash -LiteralPath $applicationPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $changed = Get-TreeSnapshot -Path $applicationRoot -ExcludedRoot $serviceRoot
    $difference = Get-TreeSnapshotDifference -ExpectedSnapshot $baseline -ActualSnapshot $changed
    $expectedDifference = "changed|MacType Control Center.exe|expected=12|$expectedHash|actual=11|$actualHash"
    if ($difference -cne $expectedDifference) {
        throw "App-side snapshot diagnostics omitted the exact path or hashes.`nExpected: $expectedDifference`nActual: $difference"
    }

    $leftoverRoot = Join-Path $temporaryRoot 'leftover'
    $emptyDirectory = Join-Path $leftoverRoot 'empty-directory'
    $leftoverFile = Join-Path (Join-Path $leftoverRoot 'nested') 'leftover.bin'
    New-Item -ItemType Directory -Path $emptyDirectory, (Split-Path -Parent $leftoverFile) -Force | Out-Null
    [IO.File]::WriteAllText($leftoverFile, 'owned-leftover', [Text.UTF8Encoding]::new($false))
    $leftoverHash = (Get-FileHash -LiteralPath $leftoverFile -Algorithm SHA256).Hash.ToLowerInvariant()
    $inventory = Get-BoundedTreeInventory -Path $leftoverRoot -MaximumEntries 8
    foreach ($expectedEntry in @(
        'directory|.',
        'directory|empty-directory',
        'directory|nested',
        "file|nested$([IO.Path]::DirectorySeparatorChar)leftover.bin|14|$leftoverHash"
    )) {
        if (($inventory -split "`n") -cnotcontains $expectedEntry) {
            throw "Uninstall leftover inventory omitted: $expectedEntry`n$inventory"
        }
    }

    $boundedInventory = Get-BoundedTreeInventory -Path $leftoverRoot -MaximumEntries 1
    if ($boundedInventory -notmatch '(?m)^truncated\|maximum-entries=1$') {
        throw "Uninstall leftover inventory is not bounded.`n$boundedInventory"
    }

    $delayedRoot = Join-Path $temporaryRoot 'delayed-delete'
    New-Item -ItemType Directory -Path $delayedRoot -Force | Out-Null
    $deleteJob = Start-Job -ScriptBlock {
        param($Target)
        Start-Sleep -Milliseconds 300
        Remove-Item -LiteralPath $Target -Recurse -Force
    } -ArgumentList $delayedRoot
    try {
        Wait-PathAbsent -Path $delayedRoot -TimeoutMilliseconds 5000
        if (Test-Path -LiteralPath $delayedRoot) {
            throw 'Bounded path wait returned before delayed deletion completed.'
        }
    }
    finally {
        Wait-Job -Job $deleteJob -Timeout 5 | Out-Null
        Receive-Job -Job $deleteJob -ErrorAction SilentlyContinue | Out-Null
        Remove-Job -Job $deleteJob -Force -ErrorAction SilentlyContinue
    }

    $persistentRoot = Join-Path $temporaryRoot 'persistent-leftover'
    New-Item -ItemType Directory -Path (Join-Path $persistentRoot 'empty') -Force | Out-Null
    try {
        Wait-PathAbsent -Path $persistentRoot -TimeoutMilliseconds 100
        throw 'Persistent uninstall residue was accepted.'
    }
    catch {
        if ($_.Exception.Message -notmatch '(?s)did not disappear.*directory\|empty') {
            throw "Persistent uninstall residue omitted its bounded inventory: $($_.Exception.Message)"
        }
    }

    Write-Host 'Installer immutable app-side snapshot contract passed.'
}
finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
