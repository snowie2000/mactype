Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'OpenServicePolicyCommon.psm1') -Force

function Test-OpenServiceRuntimePolicy {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $Root)

    $failures = [System.Collections.Generic.List[string]]::new()
    $serviceRoot = Join-Path $Root 'service-runtime'
    $serviceManifest = Join-Path $serviceRoot 'Cargo.toml'
    if (-not (Test-Path -LiteralPath $serviceManifest -PathType Leaf)) {
        $failures.Add('service-runtime/Cargo.toml is missing; the Windows service must be a standalone non-Tauri workspace.')
    } else {
        $serviceFiles = Get-OpenServiceRepositoryTextFiles -Root $Root `
            -RelativeRoots @('service-runtime')
        $serviceText = ($serviceFiles |
            Where-Object { $_.Extension -in @('.rs', '.toml', '.json') } |
            ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
        $serviceProductionText = ($serviceFiles | Where-Object {
                $_.Extension -in @('.rs', '.toml', '.json') -and
                $_.FullName -notmatch '[\\/]tests[\\/]'
            } | ForEach-Object {
                Get-Content -LiteralPath $_.FullName -Raw
            }) -join "`n"

        foreach ($manifest in $serviceFiles | Where-Object Name -eq 'Cargo.toml') {
            $manifestText = Get-Content -LiteralPath $manifest.FullName -Raw
            if ($manifestText -match '(?im)^\s*(?:tauri|tauri-build|wry|tao|webview2(?:-com)?)\s*=') {
                $relative = [System.IO.Path]::GetRelativePath($Root, $manifest.FullName)
                $failures.Add("$relative gives the Windows service runtime a Tauri/WebView dependency.")
            }
        }

        foreach ($contract in @(
            @{ Label = 'fixed production service name'; Pattern = [regex]::Escape('MacTypeControlCenter') },
            @{ Label = 'isolated hosted-CI service name'; Pattern = [regex]::Escape('MacTypeControlCenterTest') },
            @{ Label = 'versioned health pipe'; Pattern = 'MacTypeControlCenter\\?\.health\\?\.v1|MacTypeControlCenter\.health\.v1' },
            @{ Label = 'protected Program Files service root'; Pattern = 'MacType Control Center[\\/]Service' },
            @{ Label = 'protected ProgramData profile root'; Pattern = 'MacType[\\/]ControlCenter[\\/]generations' },
            @{ Label = 'versioned machine manifest'; Pattern = 'manifest\.json' },
            @{ Label = 'active runtime pointer'; Pattern = 'current\.json' },
            @{ Label = 'digest-addressed profile filename'; Pattern = 'profile\.ini' },
            @{ Label = 'SHA-256 generation digest'; Pattern = '(?i)sha-?256|Sha256' }
        )) {
            if ($serviceText -notmatch $contract.Pattern) {
                $failures.Add("service-runtime does not declare the $($contract.Label) contract.")
            }
        }

        foreach ($verb in @(
            'install', 'upgrade', 'repair', 'remove', 'start', 'stop',
            'publish-profile', 'migrate-from-legacy', 'rollback'
        )) {
            $doubleQuoted = '"' + $verb + '"'
            $singleQuoted = "'" + $verb + "'"
            if (-not $serviceText.Contains($doubleQuoted) -and
                -not $serviceText.Contains($singleQuoted)) {
                $failures.Add("service-runtime setup parser does not declare fixed broker verb '$verb'.")
            }
        }

        foreach ($forbiddenOverride in @('--service-name', 'MACTYPE_SERVICE_NAME', 'LOCALAPPDATA', 'winmgmt')) {
            if ($serviceProductionText.Contains($forbiddenOverride)) {
                $failures.Add("service-runtime contains forbidden runtime override or user-writable root token '$forbiddenOverride'.")
            }
        }

        $windowsBrokerPath = Join-Path $serviceRoot 'setup\src\windows\broker.rs'
        if (-not (Test-Path -LiteralPath $windowsBrokerPath -PathType Leaf)) {
            $failures.Add('service-runtime Windows setup broker is missing.')
        } else {
            $windowsBroker = Get-Content -LiteralPath $windowsBrokerPath -Raw
            $dispatch = Get-OpenServiceFunctionRegion -Text $windowsBroker -FunctionName 'run'
            $matchIndex = if ($dispatch) { $dispatch.IndexOf('match command') } else { -1 }
            $recoveryCalls = [ordered]@{
                'runtime_recovery::recover(&paths, &manager)' = 'runtime_recovery::recover\s*\(\s*&paths\s*,\s*&manager\s*\)(?s:.*?)\?;'
                'ProfileStore::new(paths.clone()).recover_interrupted_activation()' = 'ProfileStore::new\s*\(\s*paths\.clone\(\)\s*\)\s*\.recover_interrupted_activation\s*\(\s*\)(?s:.*?)\?;'
            }
            foreach ($entry in $recoveryCalls.GetEnumerator()) {
                $recoveryMatch = if ($dispatch) {
                    [regex]::Match($dispatch, $entry.Value)
                } else {
                    [System.Text.RegularExpressions.Match]::Empty
                }
                $recoveryIndex = if ($recoveryMatch.Success) { $recoveryMatch.Index } else { -1 }
                if ($recoveryIndex -lt 0 -or $matchIndex -lt 0 -or $recoveryIndex -gt $matchIndex) {
                    $failures.Add("service-runtime setup broker does not run '$($entry.Key)' before dispatching every mutating verb.")
                }
            }
            if ($dispatch -match '(?s)if\s+!?matches!\s*\(\s*command.*?(?:recover_interrupted_activation|runtime_recovery::recover)') {
                $failures.Add('service-runtime setup broker conditionally skips durable activation recovery for one or more mutating verbs.')
            }
        }

        $windowsInstallerPath = Join-Path $serviceRoot 'setup\src\windows\installer.rs'
        if (-not (Test-Path -LiteralPath $windowsInstallerPath -PathType Leaf)) {
            $failures.Add('service-runtime Windows installer entry point is missing.')
        } else {
            $windowsInstaller = Get-Content -LiteralPath $windowsInstallerPath -Raw
            foreach ($entryPoint in @('run_bootstrap', 'run_uninstall')) {
                $entryRegion = Get-OpenServiceFunctionRegion -Text $windowsInstaller -FunctionName $entryPoint
                if (-not $entryRegion -or
                    $entryRegion -notmatch 'runtime_recovery::recover\s*\(\s*&paths\s*,\s*&manager\s*\)\s*\?;') {
                    $failures.Add("service-runtime Windows installer '$entryPoint' does not reconcile the runtime pointer with the exact service image before continuing.")
                }
            }
        }

        $twoPhaseSites = @(
            @{
                Path = Join-Path $serviceRoot 'setup\src\windows\broker\service.rs'
                Function = 'install'
                Recovery = 'runtime_recovery::recover'
            },
            @{
                Path = Join-Path $serviceRoot 'setup\src\windows\broker\service.rs'
                Function = 'upgrade'
                Recovery = 'restore_upgrade_state'
            },
            @{
                Path = Join-Path $serviceRoot 'setup\src\windows\broker\service.rs'
                Function = 'repair'
                Recovery = 'runtime_recovery::recover'
            },
            @{
                Path = Join-Path $serviceRoot 'setup\src\windows\installer\transaction.rs'
                Function = 'apply_transaction'
                Recovery = 'restore_after_failure'
            }
        )
        foreach ($site in $twoPhaseSites) {
            if (-not (Test-Path -LiteralPath $site.Path -PathType Leaf)) {
                $failures.Add("service-runtime two-phase activation source is missing: $($site.Path).")
                continue
            }
            $source = Get-Content -LiteralPath $site.Path -Raw
            $region = Get-OpenServiceFunctionRegion -Text $source -FunctionName $site.Function
            if (-not $region -or
                $region -notmatch '(?:deploy|repair_current)_with_prepare_and_health_check' -or
                -not $region.Contains($site.Recovery)) {
                $failures.Add("service-runtime two-phase activation '$($site.Function)' does not route every error through '$($site.Recovery)'.")
            }
        }

        foreach ($helper in @(
            @{ Path = Join-Path $serviceRoot 'setup\src\windows\broker\service.rs'; Function = 'restore_upgrade_state' },
            @{ Path = Join-Path $serviceRoot 'setup\src\windows\installer\transaction.rs'; Function = 'restore_after_failure' }
        )) {
            $source = Get-Content -LiteralPath $helper.Path -Raw
            $region = Get-OpenServiceFunctionRegion -Text $source -FunctionName $helper.Function
            if (-not $region -or -not $region.Contains('runtime_recovery::recover')) {
                $failures.Add("service-runtime rollback helper '$($helper.Function)' does not perform exact runtime/service recovery.")
            }
        }
    }

    $injectorProductionFiles = Get-OpenServiceRepositoryTextFiles -Root $Root `
        -RelativeRoots @('service-injector\include', 'service-injector\src')
    $injectorProductionText = ($injectorProductionFiles | ForEach-Object {
            Get-Content -LiteralPath $_.FullName -Raw
        }) -join "`n"
    foreach ($forbiddenToken in @('CreateToolhelp32Snapshot', 'TH32CS_SNAPMODULE', 'OpenProcess(')) {
        if ($injectorProductionText.Contains($forbiddenToken)) {
            $failures.Add("service-injector must inspect modules through the inherited process HANDLE; found '$forbiddenToken'.")
        }
    }
    $moduleInventoryPath = Join-Path $Root 'service-injector\src\module_inventory.cpp'
    if (-not (Test-Path -LiteralPath $moduleInventoryPath -PathType Leaf)) {
        $failures.Add('service-injector/src/module_inventory.cpp is missing.')
    } else {
        $moduleInventory = Get-Content -LiteralPath $moduleInventoryPath -Raw
        foreach ($requiredToken in @(
            'K32EnumProcessModulesEx',
            'K32GetModuleFileNameExW',
            'module_paths_equal',
            'HANDLE process'
        )) {
            if (-not $moduleInventory.Contains($requiredToken)) {
                $failures.Add("module inventory is missing inherited-HANDLE full-path contract '$requiredToken'.")
            }
        }
    }

    return $failures.ToArray()
}

Export-ModuleMember -Function 'Test-OpenServiceRuntimePolicy'
