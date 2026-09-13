Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'OpenServicePolicyCommon.psm1') -Force

function Test-OpenServiceProductPolicy {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $Root)

    $failures = [System.Collections.Generic.List[string]]::new()
    $obsoleteBuildName = 'Build-' + 'LegacyCore.ps1'
    $obsoleteCoreTerm = 'legacy' + '-core'
    $obsoleteBuildPath = Join-Path $Root ".github\scripts\$obsoleteBuildName"
    $openBuildPath = Join-Path $Root '.github\scripts\Build-OpenCore.ps1'

    if (Test-Path -LiteralPath $obsoleteBuildPath) {
        $failures.Add(".github/scripts/$obsoleteBuildName still exists; public C/C++ output must be named Build-OpenCore.ps1.")
    }
    if (-not (Test-Path -LiteralPath $openBuildPath -PathType Leaf)) {
        $failures.Add('.github/scripts/Build-OpenCore.ps1 is missing.')
    } else {
        $openBuild = Get-Content -LiteralPath $openBuildPath -Raw
        foreach ($artifact in @(
            'MacType.dll',
            'MacType64.dll',
            'MacType.Core.dll',
            'MacType64.Core.dll',
            'MacLoader.exe',
            'MacLoader64.exe',
            'mactype-injector32.exe',
            'mactype-injector64.exe'
        )) {
            if (-not $openBuild.Contains($artifact)) {
                $failures.Add("Build-OpenCore.ps1 does not declare required artifact $artifact.")
            }
        }
        $boundaryCheckIndex = $openBuild.IndexOf('$artifactRoot.StartsWith($artifactBoundary')
        $cleanIndex = $openBuild.IndexOf('Remove-Item -LiteralPath $artifactRoot -Recurse -Force')
        if ($boundaryCheckIndex -lt 0 -or $cleanIndex -lt 0 -or $boundaryCheckIndex -gt $cleanIndex) {
            $failures.Add('Build-OpenCore.ps1 must validate the artifacts boundary before cleaning only artifacts/open-core.')
        }
        foreach ($token in @('Assert-ExactRelativeFileSet', '$ExpectedOpenCoreFiles', 'Unexpected open-core artifact set')) {
            if (-not $openBuild.Contains($token)) {
                $failures.Add("Build-OpenCore.ps1 does not enforce exact output contract '$token'.")
            }
        }
    }

    $serviceBuildPath = Join-Path $Root '.github\scripts\Build-ServiceRuntime.ps1'
    if (-not (Test-Path -LiteralPath $serviceBuildPath -PathType Leaf)) {
        $failures.Add('.github/scripts/Build-ServiceRuntime.ps1 is missing.')
    } else {
        $serviceBuild = Get-Content -LiteralPath $serviceBuildPath -Raw
        $boundaryCheckIndex = $serviceBuild.IndexOf('$output.StartsWith($artifactBoundary')
        $cleanIndex = $serviceBuild.IndexOf('Remove-Item -LiteralPath $output -Recurse -Force')
        if ($boundaryCheckIndex -lt 0 -or $cleanIndex -lt 0 -or $boundaryCheckIndex -gt $cleanIndex) {
            $failures.Add('Build-ServiceRuntime.ps1 must validate the artifacts boundary before cleaning only its output root.')
        }
        foreach ($token in @('Assert-ExactRelativeFileSet', '$ExpectedServiceRuntimeFiles', 'Unexpected service-runtime artifact set')) {
            if (-not $serviceBuild.Contains($token)) {
                $failures.Add("Build-ServiceRuntime.ps1 does not enforce exact output contract '$token'.")
            }
        }
    }

    $terminologyFiles = Get-OpenServiceRepositoryTextFiles -Root $Root `
        -RelativeRoots @('.github', 'scripts\ci')
    foreach ($file in $terminologyFiles) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
        if ($text.Contains($obsoleteBuildName) -or
            $text -match "(?i)(?:mactype-|artifacts[/\\]|\b)$([regex]::Escape($obsoleteCoreTerm))\b") {
            $failures.Add("$relative still uses the obsolete public-core term '$obsoleteCoreTerm'.")
        }
    }

    $explicitLegacyPath = '(?i)(?:' +
        '(?:^|[\\/])(?:legacy_mactray|legacy_migration|legacy_fallback|mactray_migration|mactray_fallback)(?:[\\/]|\.(?:rs|ts|tsx)$)' +
        '|^control-center[\\/]src-tauri[\\/]src[\\/]machine_integration[\\/]tests\.rs$' +
        '|^control-center[\\/]src[\\/]app[\\/]runtimeAdapters[\\/]browserGalleryExecution\.ts$' +
        '|^control-center[\\/]src[\\/]i18n[\\/][^\\/]+\.json$' +
        ')'
    $productFiles = Get-OpenServiceRepositoryTextFiles -Root $Root `
        -RelativeRoots @('control-center\src-tauri\src', 'control-center\src', 'installer') |
        Where-Object { $_.Extension -in @('.rs', '.ts', '.tsx', '.js', '.mjs', '.json', '.iss') }
    foreach ($file in $productFiles) {
        $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
        $text = Get-Content -LiteralPath $file.FullName -Raw
        if ($text -match '(?i)MacTray\.exe' -and $relative -notmatch $explicitLegacyPath) {
            $failures.Add("$relative contains MacTray.exe outside an explicitly named legacy detection/migration module.")
        }
    }

    $wp0TextFiles = Get-OpenServiceRepositoryTextFiles -Root $Root `
        -RelativeRoots @('control-center\src-tauri\src', 'control-center\src', 'docs') |
        Where-Object { $_.Extension -in @('.rs', '.ts', '.tsx', '.json', '.md') }
    foreach ($file in $wp0TextFiles) {
        $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
        $text = Get-Content -LiteralPath $file.FullName -Raw
        if ($text -match '(?i)/(?:UN)?INSTALL\b') {
            $failures.Add("$relative still documents or implements a Delphi self-registration command.")
        }
        if ($text -match '(?i)MacTray did not (?:install|remove)') {
            $failures.Add("$relative contains a stale MacTray self-registration error message.")
        }
        if ($file.Extension -eq '.rs' -and $text -match '(?m)system_mode_note:\s*"[^"]*[가-힣]') {
            $failures.Add("$relative hard-codes a Korean system_mode_note instead of returning an i18n state model.")
        }
    }

    $rustSource = Get-OpenServiceRepositoryTextFiles -Root $Root `
        -RelativeRoots @('control-center\src-tauri\src') |
        Where-Object Extension -eq '.rs'
    foreach ($file in $rustSource) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
        $trayRegion = Get-OpenServiceFunctionRegion -Text $text `
            -FunctionName 'ensure_system_injection_on_tray_start'
        if ($trayRegion -and
            $trayRegion -match '(?i)runas|ShellExecute|elevation::|activate_active_profile|publish[_a-z]*profile|\b(?:install|repair|migrate|rollback)\b') {
            $failures.Add("$relative allows the tray/login auto-start path to request elevation or mutate machine integration.")
        }
        if ($relative -notmatch $explicitLegacyPath -and
            $text -match 'legacy_mactray::activate_active_profile') {
            $failures.Add("$relative activates MacTray from a normal product path instead of an explicitly named legacy fallback/migration module.")
        }
        if ($relative -notmatch $explicitLegacyPath -and
            $text -match 'legacy_mactray::trusted_installation_root') {
            $failures.Add("$relative uses the MacTray installation root as a normal profile/runtime prerequisite.")
        }
        if ($relative -notmatch $explicitLegacyPath -and
            $text -match 'legacy_mactray::manage_legacy_service') {
            $failures.Add("$relative exposes legacy service mutation as a normal service command instead of migration-only behavior.")
        }
        if ([System.IO.Path]::GetFileName($file.FullName) -eq 'execution.rs') {
            $statusRegion = Get-OpenServiceFunctionRegion -Text $text -FunctionName 'status'
            if ($statusRegion -and
                $statusRegion -match 'system_injection_active' -and
                $statusRegion -match 'legacy_mactray' -and
                $statusRegion -notmatch '(?:open_service|system_service|service_contract)') {
                $failures.Add('execution.rs derives system injection success from legacy SCM Running instead of open-service Ready health.')
            }
        }
    }

    $legacyRoots = @(
        'control-center\src-tauri\src\legacy_mactray.rs',
        'control-center\src-tauri\src\machine_integration\legacy_mactray'
    )
    $legacySources = Get-OpenServiceRepositoryTextFiles -Root $Root -RelativeRoots $legacyRoots |
        Where-Object Extension -eq '.rs'
    foreach ($file in $legacySources) {
        $legacyText = Get-Content -LiteralPath $file.FullName -Raw
        $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
        foreach ($forbidden in @(
            '--legacy-service-broker',
            'PrivilegedAction',
            'privileged_action_from_arguments',
            'privileged_mutate',
            'TerminateProcess',
            'PROCESS_TERMINATE',
            'run_elevated',
            'activate_active_profile',
            'manage_legacy_service',
            'dispatch_privileged_command'
        )) {
            if ($legacyText -match [regex]::Escape($forbidden)) {
                $failures.Add("$relative still contains the retired normal-operation broker/API: $forbidden")
            }
        }
    }

    $libSource = Join-Path $Root 'control-center\src-tauri\src\lib.rs'
    if (Test-Path -LiteralPath $libSource -PathType Leaf) {
        $libText = Get-Content -LiteralPath $libSource -Raw
        if ($libText -match 'legacy_mactray::dispatch_privileged_command') {
            $failures.Add('lib.rs still falls back to the retired legacy MacTray privileged dispatcher.')
        }
    }

    foreach ($file in $rustSource) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        $publicRegion = Get-OpenServiceEnumRegion -Text $text -EnumName 'PublicMachineAction'
        if ($publicRegion -match '\bRollback\b') {
            $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
            $failures.Add("$relative PublicMachineAction exposes internal Rollback.")
        }
    }

    $typescriptSource = Get-OpenServiceRepositoryTextFiles -Root $Root `
        -RelativeRoots @('control-center\src') |
        Where-Object { $_.Extension -in @('.ts', '.tsx') }
    foreach ($file in $typescriptSource) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        $actionType = [regex]::Match(
            $text,
            '(?ms)\b(?:export\s+)?type\s+SystemServiceAction\s*=.*?;'
        )
        if ($actionType.Success -and $actionType.Value -match '["'']rollback["'']') {
            $relative = [System.IO.Path]::GetRelativePath($Root, $file.FullName)
            $failures.Add("$relative SystemServiceAction exposes internal rollback.")
        }
    }

    return $failures.ToArray()
}

Export-ModuleMember -Function 'Test-OpenServiceProductPolicy'
