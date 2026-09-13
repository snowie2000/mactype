[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet(
        'lifecycle',
        'prepare-reboot',
        'verify-after-reboot',
        'verify-appinit-conflict',
        'verify-migration',
        'verify-multi-session',
        'cleanup'
    )]
    [string] $Scenario,

    [Parameter(Mandatory)]
    [string] $SetupExecutable,

    [Parameter(Mandatory)]
    [string] $ServiceExecutable,

    [Parameter(Mandatory)]
    [string] $OpenCoreRoot,

    [Parameter(Mandatory)]
    [string] $Marker32,

    [Parameter(Mandatory)]
    [string] $Marker64,

    [string] $SessionEvidencePath = '',

    [string] $EvidenceDirectory = (Join-Path $env:RUNNER_TEMP 'open-service-disposable-evidence')
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'lib\OpenServiceTestSupport.psm1') -Force

$serviceName = 'MacTypeControlCenterTest'
$productionServiceName = 'MacTypeControlCenter'
$healthPath = Join-Path $env:ProgramFiles 'MacType Control Center\Service\health.json'
$rebootReceiptPath = Join-Path $env:ProgramData 'MacType\ControlCenter\disposable-vm-reboot.json'
$migrationRoot = Join-Path $env:ProgramData 'MacType\ControlCenter\legacy-migration'
$results = [System.Collections.Generic.List[object]]::new()

function Add-Result([string] $Id, [string] $Status, [string] $Detail) {
    $results.Add([ordered]@{
        id = $Id
        status = $Status
        detail = $Detail
    })
}

function Write-Evidence {
    New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
    $stamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
    $path = Join-Path $EvidenceDirectory "$Scenario-$stamp.json"
    $document = [ordered]@{
        schema = 1
        scenario = $Scenario
        recordedAt = [DateTime]::UtcNow.ToString('O')
        runner = $env:RUNNER_NAME
        machine = $env:COMPUTERNAME
        results = $results
    } | ConvertTo-Json -Depth 8
    [System.IO.File]::WriteAllText($path, $document, [System.Text.UTF8Encoding]::new($false))
    Write-Host "Disposable VM evidence written to $path"
}

function Require-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'The disposable VM verifier requires an elevated self-hosted runner token.'
    }
}

function Get-AppInitSnapshot([Microsoft.Win32.RegistryView] $View) {
    $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
        [Microsoft.Win32.RegistryHive]::LocalMachine,
        $View
    )
    try {
        $key = $base.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows', $true)
        if (-not $key) { throw "Could not open AppInit registry view $View." }
        try {
            $names = @($key.GetValueNames())
            $loadExists = $names -contains 'LoadAppInit_DLLs'
            $dllExists = $names -contains 'AppInit_DLLs'
            [pscustomobject]@{
                View = $View
                LoadExists = $loadExists
                LoadValue = if ($loadExists) { $key.GetValue('LoadAppInit_DLLs', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } else { $null }
                LoadKind = if ($loadExists) { $key.GetValueKind('LoadAppInit_DLLs').ToString() } else { $null }
                DllExists = $dllExists
                DllValue = if ($dllExists) { $key.GetValue('AppInit_DLLs', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } else { $null }
                DllKind = if ($dllExists) { $key.GetValueKind('AppInit_DLLs').ToString() } else { $null }
            }
        } finally {
            $key.Dispose()
        }
    } finally {
        $base.Dispose()
    }
}

function Set-AppInitConflict([Microsoft.Win32.RegistryView] $View) {
    $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $View)
    try {
        $key = $base.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows', $true)
        if (-not $key) { throw "Could not open AppInit registry view $View." }
        try {
            $key.SetValue('LoadAppInit_DLLs', 1, [Microsoft.Win32.RegistryValueKind]::DWord)
            $key.SetValue('AppInit_DLLs', 'C:\DisposableVm\MacType.dll', [Microsoft.Win32.RegistryValueKind]::String)
        } finally {
            $key.Dispose()
        }
    } finally {
        $base.Dispose()
    }
}

function Restore-AppInitSnapshot($Snapshot) {
    $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $Snapshot.View)
    try {
        $key = $base.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows', $true)
        if (-not $key) { throw "Could not restore AppInit registry view $($Snapshot.View)." }
        try {
            foreach ($entry in @(
                @{ Name = 'LoadAppInit_DLLs'; Exists = $Snapshot.LoadExists; Value = $Snapshot.LoadValue; Kind = $Snapshot.LoadKind },
                @{ Name = 'AppInit_DLLs'; Exists = $Snapshot.DllExists; Value = $Snapshot.DllValue; Kind = $Snapshot.DllKind }
            )) {
                if ($entry.Exists) {
                    $kind = [Enum]::Parse([Microsoft.Win32.RegistryValueKind], $entry.Kind)
                    $key.SetValue($entry.Name, $entry.Value, $kind)
                } else {
                    $key.DeleteValue($entry.Name, $false)
                }
            }
        } finally {
            $key.Dispose()
        }
    } finally {
        $base.Dispose()
    }
}

function Test-MigrationEvidence {
    $currentPath = Join-Path $migrationRoot 'current.json'
    if (-not (Test-Path -LiteralPath $currentPath -PathType Leaf)) {
        Add-Result 'migration' 'UNKNOWN' 'No protected migration receipt exists; perform the consented Control Center migration in this disposable VM first.'
        return
    }
    $current = Get-Content -LiteralPath $currentPath -Raw | ConvertFrom-Json
    if ($current.schema -ne 'mactype-control-center/legacy-migration' -or $current.version -ne 3 -or
        $current.generation -notmatch '^migration-[0-9]+-[0-9]+$') {
        throw 'The protected migration current pointer is invalid.'
    }
    $generationRoot = Join-Path $migrationRoot $current.generation
    $receiptPath = Join-Path $generationRoot 'receipt.json'
    $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
    if ($receipt.schema -ne $current.schema -or $receipt.version -ne 3 -or $receipt.generation -cne $current.generation) {
        throw 'The protected migration receipt does not match its pointer.'
    }
    foreach ($file in @($receipt.files) + @($receipt.serviceRegistry)) {
        $relative = if ($file.backupFile) { [string] $file.backupFile } else { [string] $file.exportFile }
        if ($relative -notmatch '^[A-Za-z0-9._-]+$') { throw 'Migration receipt contains an unsafe backup filename.' }
        $path = Join-Path $generationRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Migration backup is missing: $relative" }
        if ((Get-Item -LiteralPath $path).Length -ne [long] $file.byteLength -or
            (Get-LowerFileSha256 -Path $path) -cne [string] $file.sha256) {
            throw "Migration backup digest or length differs from its receipt: $relative"
        }
    }
    $stages = @($receipt.completedStages)
    if ($stages -notcontains 'backup-prepared' -or
        ($stages -notcontains 'legacy-stopped' -and $stages -notcontains 'legacy-removed')) {
        throw 'Migration receipt does not prove that the legacy service was backed up and stopped.'
    }
    $legacy = Get-Service -Name 'MacType' -ErrorAction SilentlyContinue
    if ($legacy -and $legacy.Status -ne 'Stopped') { throw 'The legacy service is still running after migration.' }
    $ready = Assert-OpenServiceStrictReady -ServiceName $productionServiceName `
        -HealthPath $healthPath
    $x86 = $ready.Health.injection.x86
    $x64 = $ready.Health.injection.x64
    if ($x86.successCount -le 0 -or $x64.successCount -le 0 -or
        $x86.lastSuccess.profileDigest -cne $ready.Health.activeProfileDigest -or
        $x64.lastSuccess.profileDigest -cne $ready.Health.activeProfileDigest -or
        $x86.lastSuccess.runtimeGenerationId -cne $x64.lastSuccess.runtimeGenerationId) {
        throw 'Migration does not have matching Ready, profile, and x86/x64 smoke evidence.'
    }
    Add-Result 'migration' 'PASS' 'Protected backup hashes, stopped legacy service, Ready health, and matching x86/x64 smoke evidence were verified.'
}

function Test-MultiSessionEvidence {
    if ([string]::IsNullOrWhiteSpace($SessionEvidencePath) -or -not (Test-Path -LiteralPath $SessionEvidencePath -PathType Leaf)) {
        Add-Result 'multi-user-session' 'UNKNOWN' 'No two-session marker evidence was supplied. Keep this UNKNOWN until two interactive sessions produce the documented JSON.'
        return
    }
    $evidence = Get-Content -LiteralPath $SessionEvidencePath -Raw | ConvertFrom-Json
    if ($evidence.schema -ne 1) { throw 'Multi-session evidence has an unsupported schema.' }
    $records = @($evidence.records)
    $sessions = @($records | Where-Object { $_.sessionId -gt 0 } | Select-Object -ExpandProperty sessionId -Unique)
    if ($sessions.Count -lt 2) { throw 'Multi-session evidence contains fewer than two interactive sessions.' }
    foreach ($session in $sessions) {
        foreach ($architecture in @('x86', 'x64')) {
            $record = @($records | Where-Object { $_.sessionId -eq $session -and $_.architecture -eq $architecture })
            if ($record.Count -ne 1 -or -not $record[0].mactypeModuleLoaded -or
                $record[0].profileDigest -notmatch '^sha256:[0-9a-f]{64}$' -or
                $record[0].runtimeGenerationId -notmatch '^[0-9a-f]{64}$') {
                throw "Session $session lacks one valid $architecture injection record."
            }
        }
    }
    Add-Result 'multi-user-session' 'PASS' "Verified x86 and x64 injection records in $($sessions.Count) interactive sessions."
}

if ($env:GITHUB_ACTIONS -ne 'true' -or $env:MACTYPE_DISPOSABLE_VM_CONFIRM -cne 'I_UNDERSTAND_DISPOSABLE_VM') {
    throw 'This mutating verifier runs only from the confirmed disposable-VM workflow dispatch.'
}
Require-Administrator
if ($Scenario -ne 'verify-migration' -and (Get-Service -Name $productionServiceName -ErrorAction SilentlyContinue)) {
    throw 'The disposable CI scenarios refuse to coexist with the production open-service identity.'
}

try {
    switch ($Scenario) {
        'lifecycle' {
            & (Join-Path $PSScriptRoot 'Test-OpenServiceWindows.ps1') `
                -SetupExecutable $SetupExecutable `
                -ServiceExecutable $ServiceExecutable `
                -OpenCoreRoot $OpenCoreRoot `
                -Marker32 $Marker32 `
                -Marker64 $Marker64
            Add-Result 'service-crash-restart' 'PASS' 'SCM crash-once recovery completed with a new service PID and Ready health.'
            Add-Result 'repair' 'PASS' 'Running and stopped repair behavior passed.'
            Add-Result 'rollback' 'PASS' 'Profile generation rollback restored exact protected bytes.'
        }
        'prepare-reboot' {
            & (Join-Path $PSScriptRoot 'Test-OpenServiceWindows.ps1') `
                -SetupExecutable $SetupExecutable `
                -ServiceExecutable $ServiceExecutable `
                -OpenCoreRoot $OpenCoreRoot `
                -Marker32 $Marker32 `
                -Marker64 $Marker64 `
                -LeaveInstalledForReboot
            $ready = Assert-OpenServiceStrictReady -ServiceName $serviceName `
                -HealthPath $healthPath
            if ($ready.Service.StartMode -ne 'Auto') { throw 'The CI service is not configured for automatic start.' }
            $boot = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('O')
            $receipt = [ordered]@{
                schema = 1
                bootedAt = $boot
                servicePid = [uint32] $ready.Service.ProcessId
                serviceVersion = [string] $ready.Health.serviceVersion
                profileDigest = [string] $ready.Health.activeProfileDigest
                preparedAt = [DateTime]::UtcNow.ToString('O')
            } | ConvertTo-Json
            [System.IO.File]::WriteAllText($rebootReceiptPath, $receipt, [System.Text.UTF8Encoding]::new($false))
            Add-Result 'reboot-auto-start-prepared' 'PASS' 'The isolated service was left Ready and Auto with a protected pre-reboot receipt. Reboot the VM before the verification dispatch.'
        }
        'verify-after-reboot' {
            if (-not (Test-Path -LiteralPath $rebootReceiptPath -PathType Leaf)) { throw 'The pre-reboot receipt is missing.' }
            $receipt = Get-Content -LiteralPath $rebootReceiptPath -Raw | ConvertFrom-Json
            if ($receipt.schema -ne 1) { throw 'The pre-reboot receipt schema is invalid.' }
            $currentBoot = (Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime()
            if ($currentBoot -le [DateTime]::Parse([string] $receipt.bootedAt).ToUniversalTime()) {
                throw 'Windows has not rebooted since the prepare-reboot phase.'
            }
            $ready = Assert-OpenServiceStrictReady -ServiceName $serviceName `
                -HealthPath $healthPath
            if ($ready.Service.StartMode -ne 'Auto' -or $ready.Service.ProcessId -eq $receipt.servicePid -or
                $ready.Health.serviceVersion -cne $receipt.serviceVersion -or
                $ready.Health.activeProfileDigest -cne $receipt.profileDigest) {
                throw 'Post-reboot service identity, Auto start, or Ready profile continuity failed.'
            }
            Add-Result 'reboot-auto-start' 'PASS' 'A later Windows boot produced a new service PID with Auto start and the same strict Ready profile.'
        }
        'verify-appinit-conflict' {
            $null = Assert-OpenServiceStrictReady -ServiceName $serviceName `
                -HealthPath $healthPath
            $snapshots = @(
                Get-AppInitSnapshot ([Microsoft.Win32.RegistryView]::Registry32)
                Get-AppInitSnapshot ([Microsoft.Win32.RegistryView]::Registry64)
            )
            try {
                $null = Invoke-OpenServiceSetupLogged `
                    -SetupExecutable $SetupExecutable -Verb 'stop'
                Set-AppInitConflict ([Microsoft.Win32.RegistryView]::Registry32)
                Set-AppInitConflict ([Microsoft.Win32.RegistryView]::Registry64)
                $null = Invoke-OpenServiceSetupLogged `
                    -SetupExecutable $SetupExecutable -Verb 'start' -ExpectFailure
                $health = Read-OpenServiceHealthSnapshot -Path $healthPath
                if ($health.health -ne 'failed' -or $health.lastError.code -ne 'appinit-conflict') {
                    throw 'Enabled MacType AppInit did not fail closed with appinit-conflict health.'
                }
                Add-Result 'appinit-conflict' 'PASS' 'Both registry views were enabled with MacType AppInit and the open service failed closed.'
            } finally {
                foreach ($snapshot in $snapshots) { Restore-AppInitSnapshot $snapshot }
                $null = Invoke-OpenServiceSetupLogged `
                    -SetupExecutable $SetupExecutable -Verb 'start'
                $null = Assert-OpenServiceStrictReady -ServiceName $serviceName `
                    -HealthPath $healthPath
            }
        }
        'verify-migration' { Test-MigrationEvidence }
        'verify-multi-session' { Test-MultiSessionEvidence }
        'cleanup' {
            if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
                try {
                    $null = Invoke-OpenServiceSetupLogged `
                        -SetupExecutable $SetupExecutable -Verb 'stop'
                } catch { Write-Warning $_ }
                $null = Invoke-OpenServiceSetupLogged `
                    -SetupExecutable $SetupExecutable -Verb 'remove'
            }
            Remove-Item -LiteralPath $rebootReceiptPath -Force -ErrorAction SilentlyContinue
            Add-Result 'cleanup' 'PASS' 'The isolated service and disposable reboot receipt were removed.'
        }
    }
    Write-Evidence
} catch {
    Add-Result $Scenario 'FAIL' $_.Exception.Message
    Write-Evidence
    throw
}
