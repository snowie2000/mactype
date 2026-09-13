[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$required = @(
    'distribution\MacType.ini',
    'distribution\ini\Default.ini',
    'distribution\languages\en.json',
    'distribution\languages\ko.json',
    'distribution\THIRD_PARTY_NOTICES.md',
    'LICENSE'
)
foreach ($relative in $required) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $relative) -PathType Leaf)) {
        throw "Distribution source file is missing: $relative"
    }
}

$binary = Get-ChildItem -LiteralPath (Join-Path $root 'distribution') -Recurse -File | Where-Object { $_.Extension -in @('.exe', '.dll') }
if ($binary) { throw "Prebuilt binary is forbidden in distribution/: $($binary.FullName)" }

$english = Get-Content -LiteralPath (Join-Path $root 'distribution\languages\en.json') -Raw | ConvertFrom-Json -AsHashtable
$korean = Get-Content -LiteralPath (Join-Path $root 'distribution\languages\ko.json') -Raw | ConvertFrom-Json -AsHashtable
if (Compare-Object ($english.Keys | Sort-Object) ($korean.Keys | Sort-Object)) {
    throw 'English and Korean distribution translation keys differ.'
}

$profile = Get-Content -LiteralPath (Join-Path $root 'distribution\ini\Default.ini') -Raw
foreach ($section in @('[General]', '[DirectWrite]', '[Individual]', '[Exclude]', '[ExcludeModule]')) {
    if (-not $profile.Contains($section)) { throw "Default profile is missing section $section" }
}

$buildScript = Get-Content -LiteralPath (Join-Path $root '.github\scripts\Build-OpenCore.ps1') -Raw
foreach ($commit in @(
    'ef771574d04721baf45a1b66bfb4692193603088',
    'a457397ffa9d20e8df43e2c143c60da78c16c059',
    'd644ce94e8c7f7f5a31591577c78134ea3ac1fae',
    '667359c7967249dd9d28d8f8cef65b60e7e2d963'
)) {
    if (-not $buildScript.Contains($commit)) { throw "Core dependency is not pinned: $commit" }
}

$installer = Get-Content -LiteralPath (Join-Path $root 'installer\mactype-control-center.iss') -Raw
foreach ($legacy in @('MacTray.exe', 'MacTuner.exe', 'MacWiz.exe', 'VisTuner.exe', 'EasyHK32.dll', 'EasyHK64.dll')) {
    if ($installer.Contains($legacy)) { throw "Installer references forbidden legacy binary: $legacy" }
}
if (-not $installer.Contains('MacType64.dll') -or -not $installer.Contains('MacLoader64.exe')) {
    throw 'Installer does not contain the independent x86/x64 core set.'
}

foreach ($machinePayloadToken in @(
    '{#ServiceRuntimeRoot}',
    '{app}\service-runtime',
    'mactype-service-setup.exe',
    'payload\manifest.json',
    'payload\files\mactype-service.exe',
    'payload\files\mactype-injector32.exe',
    'payload\files\mactype-injector64.exe',
    'payload\files\MacType.dll',
    'payload\files\MacType64.dll'
)) {
    if (-not $installer.Contains($machinePayloadToken)) {
        throw "Installer does not stage the fixed open-service payload token: $machinePayloadToken"
    }
}
foreach ($protectedInstallerToken in @(
    'DefaultDirName={autopf}\MacType Control Center',
    'PrivilegesRequired=admin',
    'UsePreviousAppDir=no',
    'bootstrap-install',
    'uninstall-owned',
    'UninstallNeedRestart',
    'DeferredRuntimeCleanup',
    'PrepareToInstall',
    'ewWaitUntilTerminated',
    'runasoriginaluser',
    '{cm:LaunchProgram,MacType Control Center}',
    '{autodesktop}\MacType Control Center'
)) {
    if (-not $installer.Contains($protectedInstallerToken)) {
        throw "Installer does not enforce protected machine bootstrap token: $protectedInstallerToken"
    }
}
foreach ($forbiddenInstallerToken in @(
    'PrivilegesRequired=lowest',
    '{localappdata}',
    'Root: HKCU',
    "ShellExec('runas'",
    'Description: "MacType Control Center 실행"'
)) {
    if ($installer.Contains($forbiddenInstallerToken)) {
        throw "Admin installer retains a user-writable/elevated broker hazard: $forbiddenInstallerToken"
    }
}
$uninstallDeleteSection = [regex]::Match(
    $installer,
    '(?ms)^\[UninstallDelete\]\s*(?<body>.*?)(?=^\[[^]]+\]|\z)'
)
if (-not $uninstallDeleteSection.Success -or
    $uninstallDeleteSection.Groups['body'].Value -notmatch '(?m)^Type:\s*dirifempty;\s*Name:\s*"\{app\}"\s*$') {
    throw 'Installer does not safely remove the exact application root when final cleanup leaves it empty.'
}
if ($uninstallDeleteSection.Groups['body'].Value -match '(?im)^Type:\s*filesandordirs;\s*Name:\s*"\{app\}\\Service(?:\\|"|\*)') {
    throw 'Installer must not recursively delete the protected Service tree without broker receipts.'
}
if ($installer -match '(?im)\bsc(?:\.exe)?\s+(?:create|config|start|stop|delete)\b') {
    throw 'Installer must mutate SCM only through the fixed protected setup broker.'
}

$installerTest = Get-Content -LiteralPath (Join-Path $root 'scripts\ci\Test-InstallerWindows.ps1') -Raw
foreach ($installerTestToken in @(
    'CommonDesktopDirectory',
    'Arbitrary-directory install',
    'Assert-ReadyOpenService',
    'Assert-BaselineRestoredAfterFailedUpgrade',
    'Deliberately failing protected upgrade',
    'Upgrade reused an immutable runtime version',
    'Uninstall with missing protected broker',
    'CI foreign fixed-name service',
    'CI legacy MacTray service',
    'Assert-UserMarkers'
)) {
    if (-not $installerTest.Contains($installerTestToken)) {
        throw "Installer E2E omits required machine integration scenario: $installerTestToken"
    }
}

$distributionDocs = @{
    'docs\control-center-ci.md' = @(
        'PrivilegesRequired=admin',
        'fixed Program Files',
        'test-only failing upgrade',
        'LocalAppData theme, locale, recent-profile, applied-profile, and profile files'
    )
    'docs\independent-distribution.md' = @(
        'administrator-elevated installer',
        'bootstrap-install',
        'Auto/LocalSystem/Running',
        '%LOCALAPPDATA%'
    )
}
$staleInstallerClaims = @(
    'PrivilegesRequired=lowest',
    'installer is per-user',
    'per-user installer only stages',
    'never registers or starts SCM',
    'request no elevation'
)
foreach ($entry in $distributionDocs.GetEnumerator()) {
    $text = Get-Content -LiteralPath (Join-Path $root $entry.Key) -Raw
    foreach ($token in $entry.Value) {
        if (-not $text.Contains($token)) {
            throw "Distribution documentation is missing current installer contract '$token': $($entry.Key)"
        }
    }
    foreach ($claim in $staleInstallerClaims) {
        if ($text.Contains($claim)) {
            throw "Distribution documentation retains stale installer behavior '$claim': $($entry.Key)"
        }
    }
}

$trackedFiles = @(& git -C $root ls-files)
if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate tracked files for desktop-only distribution policy.' }

$forbiddenSiteArtifacts = $trackedFiles | Where-Object {
    (Test-Path -LiteralPath (Join-Path $root $_)) -and (
        $_ -match '(?i)(?:^|/)(?:robots\.txt|sitemap(?:[-._][^/]*)?|seo(?:[-._][^/]*)?)$' -or
        $_ -match '(?i)(?:^|/)lighthouse(?:/|[-._])' -or
        $_ -eq 'scripts/ci/Assert-Lighthouse.mjs'
    )
}
if ($forbiddenSiteArtifacts) {
    throw "Desktop distribution contains website-only artifacts: $($forbiddenSiteArtifacts -join ', ')"
}

$workflowTokens = @('light' + 'house', 'robots' + '.txt', 'site' + 'map', 'S' + 'EO')
foreach ($workflow in Get-ChildItem -LiteralPath (Join-Path $root '.github\workflows') -File | Where-Object Extension -in @('.yml', '.yaml')) {
    $workflowText = Get-Content -LiteralPath $workflow.FullName -Raw
    foreach ($token in $workflowTokens) {
        if ($workflowText -match "(?i)\b$([regex]::Escape($token))\b") {
            throw "Desktop CI workflow contains website-only token '$token': $($workflow.Name)"
        }
    }
}

$frontendEvidenceRoot = Join-Path $root '.superloopy\evidence\frontend\2026-07-12-control-center'
$frontendEvidenceContracts = @{
    'PERF.md' = @('Design-system compliance', 'React Doctor')
    'SUPERLOOPY_EVIDENCE.md' = @('VISUAL_QA.md', 'DESIGN_TOKENS.md', 'TARGET_SPEC.md', 'screenshots/*.png')
}
foreach ($entry in $frontendEvidenceContracts.GetEnumerator()) {
    $path = Join-Path $frontendEvidenceRoot $entry.Key
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Desktop frontend evidence is missing: $($entry.Key)"
    }
    $text = Get-Content -LiteralPath $path -Raw
    if ($text -match '(?i)lighthouse') {
        throw "Desktop frontend evidence contains a website-only Lighthouse reference: $($entry.Key)"
    }
    foreach ($token in $entry.Value) {
        if (-not $text.Contains($token)) {
            throw "Desktop frontend evidence is missing '$token': $($entry.Key)"
        }
    }
}

Write-Host 'Independent distribution policy passed.'
