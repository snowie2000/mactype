[CmdletBinding()]
param(
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

    [switch] $LeaveInstalledForReboot
)

$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'lib\OpenServiceTestSupport.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'lib\OpenServiceAclFixture.psm1') -Force

$serviceName = 'MacTypeControlCenterTest'
$productionServiceName = 'MacTypeControlCenter'
$machineRoot = Join-Path $env:ProgramFiles 'MacType Control Center\Service'
$profileRoot = Join-Path $env:ProgramData 'MacType\ControlCenter'

if ($env:GITHUB_ACTIONS -ne 'true' -or [string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    throw 'This test mutates Windows SCM and protected machine paths; run it only on an isolated disposable GitHub Actions Windows runner.'
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The hosted open-service lifecycle test requires an elevated Windows runner token.'
}

foreach ($path in @($SetupExecutable, $ServiceExecutable, $Marker32, $Marker64)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required CI executable is missing: $path" }
}
if (-not (Test-Path -LiteralPath $OpenCoreRoot -PathType Container)) { throw "Open core artifact root is missing: $OpenCoreRoot" }

if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
    throw "The isolated hosted-CI service name already exists: $serviceName"
}
if (Get-Service -Name $productionServiceName -ErrorAction SilentlyContinue) {
    throw "The GitHub-hosted runner unexpectedly contains the production service: $productionServiceName"
}

$stagingRoot = Join-Path $env:RUNNER_TEMP "mactype-open-service-$PID-$([guid]::NewGuid().ToString('N'))"
$payloadRoot = Join-Path $stagingRoot 'payload'
$payloadFiles = Join-Path $payloadRoot 'files'
New-Item -ItemType Directory -Path $payloadFiles -Force | Out-Null

$stagedSetup = Join-Path $stagingRoot 'mactype-service-setup.exe'
Copy-Item -LiteralPath $SetupExecutable -Destination $stagedSetup

$payloadSources = [ordered]@{
    'mactype-service.exe'    = $ServiceExecutable
    'mactype-injector32.exe' = (Join-Path $OpenCoreRoot 'mactype-injector32.exe')
    'mactype-injector64.exe' = (Join-Path $OpenCoreRoot 'mactype-injector64.exe')
    'MacType.dll'            = (Join-Path $OpenCoreRoot 'MacType.dll')
    'MacType64.dll'          = (Join-Path $OpenCoreRoot 'MacType64.dll')
}

$manifestFiles = [ordered]@{}
foreach ($entry in $payloadSources.GetEnumerator()) {
    if (-not (Test-Path -LiteralPath $entry.Value -PathType Leaf)) {
        throw "Required open-service payload is missing: $($entry.Value)"
    }
    $destination = Join-Path $payloadFiles $entry.Key
    Copy-Item -LiteralPath $entry.Value -Destination $destination
    $hash = Get-LowerFileSha256 -Path $destination
    $manifestFiles[$entry.Key] = "sha256:$hash"
}

function Write-PayloadManifest([string] $Version) {
    $manifest = [ordered]@{
        schema = 1
        version = $Version
        files = $manifestFiles
    } | ConvertTo-Json -Depth 4 -Compress
    [System.IO.File]::WriteAllText((Join-Path $payloadRoot 'manifest.json'), $manifest, [System.Text.UTF8Encoding]::new($false))
}
Write-PayloadManifest '0.2.0'

function Get-LowerHexDigest([byte[]] $Bytes) {
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($Bytes)).ToLowerInvariant()
}

function Assert-ProtectedAcl([string] $Path, [switch] $AllowMissing) {
    try {
        $acl = Get-Acl -LiteralPath $Path -ErrorAction Stop
    } catch {
        # health.json and other service-owned files are replaced atomically. A
        # recursive enumeration can legitimately retain the old temporary name
        # after the rename has completed; that vanished entry is not an ACL
        # failure. Persistent entries and the tree root still fail closed.
        if ($AllowMissing -and -not (Test-Path -LiteralPath $Path)) { return }
        if (-not (Test-Path -LiteralPath $Path)) {
            throw "Protected machine path is missing: $Path"
        }
        throw
    }
    $lowPrivilegeSids = @('S-1-1-0', 'S-1-5-11', 'S-1-5-32-545')

    foreach ($rule in $acl.Access) {
        if ($rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) { continue }
        try {
            $sid = $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
        } catch {
            continue
        }
        if ($sid -in $lowPrivilegeSids -and
            (Test-OpenServiceAclWriteCapability -Rights $rule.FileSystemRights)) {
            throw "$Path grants machine-runtime write rights to ${sid}: $($rule.FileSystemRights)"
        }
    }
}

function Assert-ProtectedTree([string] $Path) {
    Assert-ProtectedAcl $Path
    foreach ($item in Get-ChildItem -LiteralPath $Path -Recurse -Force) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Protected machine tree contains a reparse point: $($item.FullName)"
        }
        Assert-ProtectedAcl $item.FullName -AllowMissing
    }
}

function Assert-ActiveRuntimeProfile([byte[]] $ExpectedBytes, [string] $ExpectedDigest, [string] $Phase) {
    foreach ($journal in @(
        (Join-Path $machineRoot 'runtime-activation.json'),
        (Join-Path $profileRoot 'profile-activation.json')
    )) {
        if (Test-Path -LiteralPath $journal) {
            throw "$Phase left a durable activation recovery pending: $journal"
        }
    }
    $activePointerPath = Join-Path $profileRoot 'active.json'
    if (-not (Test-Path -LiteralPath $activePointerPath -PathType Leaf)) {
        throw "$Phase did not leave a protected active profile pointer."
    }
    $activePointer = Get-Content -LiteralPath $activePointerPath -Raw | ConvertFrom-Json -AsHashtable
    if ($activePointer.Count -ne 2 -or $activePointer.schema -ne 1 -or $activePointer.generation -cne "sha256:$ExpectedDigest") {
        throw "$Phase active profile pointer does not match the expected generation."
    }

    $protectedProfilePath = Join-Path $profileRoot "generations\$ExpectedDigest\profile.ini"
    $runtimePointer = Get-Content -LiteralPath (Join-Path $machineRoot 'current.json') -Raw | ConvertFrom-Json -AsHashtable
    $runtimeProfilePath = Join-Path $machineRoot "bin\$($runtimePointer.version)\MacType.ini"
    foreach ($path in @($protectedProfilePath, $runtimeProfilePath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "$Phase profile bridge output is missing: $path"
        }
    }

    [byte[]] $protectedBytes = Get-Content -LiteralPath $protectedProfilePath -AsByteStream -Raw
    [byte[]] $runtimeBytes = Get-Content -LiteralPath $runtimeProfilePath -AsByteStream -Raw
    $expectedBase64 = [Convert]::ToBase64String($ExpectedBytes)
    if ([Convert]::ToBase64String($protectedBytes) -cne $expectedBase64) {
        throw "$Phase protected profile generation does not contain the exact published bytes."
    }
    if ([Convert]::ToBase64String($runtimeBytes) -cne $expectedBase64) {
        throw "$Phase DLL-adjacent MacType.ini does not contain the exact active profile bytes."
    }
    if ((Get-LowerHexDigest $runtimeBytes) -cne $ExpectedDigest) {
        throw "$Phase DLL-adjacent MacType.ini digest does not match the active generation."
    }
    Assert-ProtectedTree $machineRoot
}

function Assert-PersistedReadyHealth([string] $ExpectedDigest, [string] $Phase) {
    $path = Join-Path $machineRoot 'health.json'
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    $report = $null
    $lastReadError = $null
    do {
        try {
            $report = Read-OpenServiceHealthSnapshot -Path $path `
                -Context "$Phase persisted health snapshot"
            $lastReadError = $null
            if ($report.health -eq 'ready' -and
                $report.activeProfileDigest -ceq "sha256:$ExpectedDigest" -and
                -not $report.lastError) {
                return
            }
        } catch {
            $lastReadError = $_.Exception.Message
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)

    if ($lastReadError) {
        throw "$Phase persisted health snapshot could not be read after the Ready deadline: $lastReadError"
    }
    throw "$Phase persisted health snapshot did not converge to strict Ready for the expected profile (health=$($report.health), digest=$($report.activeProfileDigest), lastError=$($report.lastError))."
}

function Assert-GenerationBoundMarkerTelemetry(
    [object[]] $MarkerResults,
    [string] $ExpectedDigest
) {
    $healthPath = Join-Path $machineRoot 'health.json'
    $report = Read-OpenServiceHealthSnapshot -Path $healthPath `
        -Context 'Generation-bound marker health snapshot'
    $runtimeGeneration = $null
    foreach ($architecture in @('x86', 'x64')) {
        $marker = $MarkerResults | Where-Object architecture -CEQ $architecture | Select-Object -First 1
        $telemetry = $report.injection.$architecture
        if (-not $marker -or -not $telemetry -or $telemetry.successCount -lt 1 -or -not $telemetry.lastSuccess) {
            throw "$architecture marker has no matching generation-bound injection telemetry."
        }
        $success = $telemetry.lastSuccess
        if ($success.profileDigest -cne "sha256:$ExpectedDigest") {
            throw "$architecture marker telemetry is bound to the wrong profile digest."
        }
        if ([string] $success.runtimeGenerationId -notmatch '^[0-9a-f]{64}$') {
            throw "$architecture marker telemetry has a non-canonical runtime generation."
        }
        if ($null -eq $runtimeGeneration) {
            $runtimeGeneration = [string] $success.runtimeGenerationId
        } elseif ($runtimeGeneration -cne [string] $success.runtimeGenerationId) {
            throw 'x86 and x64 marker telemetry is not bound to the same runtime generation.'
        }
    }
}

$installationAttempted = $false
$leaveInstalled = $false
try {
    $installationAttempted = $true
    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'install'

    $service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if (-not $service) { throw "Setup did not register $serviceName." }
    if (Get-Service -Name $productionServiceName -ErrorAction SilentlyContinue) {
        throw 'The ci-test-adapter registered the production service name.'
    }
    if ($service.StartName -ne 'LocalSystem') { throw "$serviceName runs as '$($service.StartName)' instead of LocalSystem." }
    if ($service.StartMode -ne 'Auto') { throw "$serviceName start mode is '$($service.StartMode)' instead of Auto." }
    if ($service.PathName -match '(?i)MacTray|AppData|LOCALAPPDATA') { throw "Unsafe service ImagePath: $($service.PathName)" }
    if ($service.PathName -notmatch '(?i)\\MacType Control Center\\Service\\bin\\[^\\]+\\mactype-service\.exe"?(?:\s+--service)?$') {
        throw "Service ImagePath is outside the protected immutable generation: $($service.PathName)"
    }
    $runtimePointerPath = Join-Path $machineRoot 'current.json'
    $runtimePointer = Get-Content -LiteralPath $runtimePointerPath -Raw | ConvertFrom-Json -AsHashtable
    if ($runtimePointer.Count -ne 2 -or $runtimePointer.schema -ne 1 -or $runtimePointer.version -ne '0.2.0') {
        throw 'Installed machine runtime pointer violates the fixed schema/version contract.'
    }
    $installedRuntimeRoot = Join-Path $machineRoot 'bin\0.2.0'
    $installedRuntimeNames = Get-ChildItem -LiteralPath $installedRuntimeRoot -File | Select-Object -ExpandProperty Name
    if (Compare-Object ($payloadSources.Keys | Sort-Object) ($installedRuntimeNames | Sort-Object)) {
        throw 'Protected machine runtime contains a missing or unapproved file.'
    }
    foreach ($name in $payloadSources.Keys) {
        $installedHash = Get-LowerFileSha256 `
            -Path (Join-Path $installedRuntimeRoot $name)
        if ($manifestFiles[$name] -ne "sha256:$installedHash") {
            throw "Protected machine runtime hash differs from the verified staging manifest: $name"
        }
    }
    $profileA = [Text.UTF8Encoding]::new($false).GetBytes("[General]`r`nHintingMode=0`r`n")
    $profileB = [Text.UTF8Encoding]::new($false).GetBytes("[General]`r`nHintingMode=1`r`n")
    $digestA = Get-LowerHexDigest $profileA
    $digestB = Get-LowerHexDigest $profileB
    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup `
        -Verb 'publish-profile' -InputBytes $profileA
    $generationA = Join-Path $profileRoot "generations\$digestA\profile.ini"
    if (-not (Test-Path -LiteralPath $generationA -PathType Leaf)) { throw "First protected profile generation is missing: $generationA" }
    Assert-ActiveRuntimeProfile -ExpectedBytes $profileA -ExpectedDigest $digestA -Phase 'publish A'
    Assert-ProtectedTree $profileRoot

    $poisonedAclPath = Join-Path $installedRuntimeRoot 'mactype-service.exe'
    $null = Invoke-OpenServiceAclRepairFixture `
        -Path $poisonedAclPath `
        -ServiceName $serviceName `
        -RepairContext $stagedSetup `
        -RepairAction {
            param($setupExecutable)

            $null = Invoke-OpenServiceSetupLogged `
                -SetupExecutable $setupExecutable -Verb 'repair'
        }
    Assert-ProtectedTree $machineRoot
    Write-Host 'Exact Users:M ACL poison-to-repair regression passed.'
    $dependencies = @((Get-Service -Name $serviceName).ServicesDependedOn | Select-Object -ExpandProperty Name)
    if ($dependencies -contains 'winmgmt') { throw 'Open service copied the unproven legacy winmgmt dependency.' }
    Assert-ProtectedTree $machineRoot

    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'start'
    if ((Get-Service -Name $serviceName).Status -ne 'Running') { throw "$serviceName did not reach SCM Running after its Ready handshake." }
    Assert-PersistedReadyHealth -ExpectedDigest $digestA -Phase 'initial start'

    Write-PayloadManifest '0.3.0'
    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'upgrade'
    $upgradedPointer = Get-Content -LiteralPath $runtimePointerPath -Raw | ConvertFrom-Json -AsHashtable
    if ($upgradedPointer.Count -ne 2 -or $upgradedPointer.schema -ne 1 -or $upgradedPointer.version -ne '0.3.0') {
        throw 'Upgrade did not activate the bundled 0.3.0 runtime generation.'
    }
    $upgradedService = $null
    $upgradedServiceDeadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $candidate = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
        if ($candidate -and
            $candidate.State -eq 'Running' -and
            $candidate.PathName -match '(?i)\\bin\\0\.3\.0\\mactype-service\.exe"?(?:\s+--service)?$') {
            $upgradedService = $candidate
            break
        }
        $upgradedService = $candidate
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $upgradedServiceDeadline)
    if (-not $upgradedService -or
        $upgradedService.State -ne 'Running' -or
        $upgradedService.PathName -notmatch '(?i)\\bin\\0\.3\.0\\mactype-service\.exe"?(?:\s+--service)?$') {
        $observedState = if ($upgradedService) { $upgradedService.State } else { '<absent>' }
        $observedPath = if ($upgradedService) { $upgradedService.PathName } else { '<absent>' }
        throw "Upgrade did not reconfigure, start, and reach Ready on the bundled runtime. Observed state='$observedState' path='$observedPath'."
    }
    Assert-ActiveRuntimeProfile -ExpectedBytes $profileA -ExpectedDigest $digestA -Phase 'upgrade to bundled 0.3.0'
    Assert-PersistedReadyHealth -ExpectedDigest $digestA -Phase 'upgrade to bundled 0.3.0'

    $beforeCrash = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if (-not $beforeCrash -or $beforeCrash.ProcessId -le 0) { throw 'Could not capture the pre-crash test service PID.' }
    $crashAdapterRoot = Join-Path $profileRoot 'ci-test-adapter'
    New-Item -ItemType Directory -Path $crashAdapterRoot -Force | Out-Null
    if (((Get-Item -LiteralPath $crashAdapterRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'The fixed crash-once adapter directory is a reparse point.'
    }
    $crashRequest = Join-Path $crashAdapterRoot 'crash-once.request'
    $crashConsumed = Join-Path $crashAdapterRoot 'crash-once.consumed'
    [System.IO.File]::WriteAllText($crashRequest, "mactype-ci-crash-once`n", [System.Text.UTF8Encoding]::new($false))
    Assert-ProtectedTree $crashAdapterRoot

    $restartDeadline = [DateTime]::UtcNow.AddSeconds(60)
    $afterCrash = $null
    while ([DateTime]::UtcNow -lt $restartDeadline) {
        $candidate = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
        if ($candidate -and $candidate.State -eq 'Running' -and $candidate.ProcessId -gt 0 -and $candidate.ProcessId -ne $beforeCrash.ProcessId) {
            $afterCrash = $candidate
            break
        }
        Start-Sleep -Milliseconds 250
    }
    if (-not $afterCrash) { throw 'SCM did not restart the crash-once test service with a new PID.' }
    if (Test-Path -LiteralPath $crashRequest) { throw 'The crash-once request was not atomically consumed.' }
    if (-not (Test-Path -LiteralPath $crashConsumed -PathType Leaf)) { throw 'The crash-once consumed receipt is missing.' }
    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'start'
    $readyAfterCrash = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if ($readyAfterCrash.State -ne 'Running' -or $readyAfterCrash.ProcessId -ne $afterCrash.ProcessId) {
        throw 'The SCM-restarted service did not retain its recovered PID through the Ready handshake.'
    }
    Assert-PersistedReadyHealth -ExpectedDigest $digestA -Phase 'SCM crash recovery'
    Remove-Item -LiteralPath $crashConsumed -Force
    Remove-Item -LiteralPath $crashAdapterRoot -Force

    $beforeRunningRepair = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'repair'
    $afterRunningRepair = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
    if ($afterRunningRepair.State -ne 'Running' -or $afterRunningRepair.ProcessId -le 0 -or $afterRunningRepair.ProcessId -eq $beforeRunningRepair.ProcessId) {
        throw 'Repair did not preserve Running via stop, protected repair, restart, and Ready.'
    }
    Assert-ActiveRuntimeProfile -ExpectedBytes $profileA -ExpectedDigest $digestA -Phase 'running repair'
    Assert-PersistedReadyHealth -ExpectedDigest $digestA -Phase 'running repair'

    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup `
        -Verb 'publish-profile' -InputBytes $profileB
    $generationB = Join-Path $profileRoot "generations\$digestB\profile.ini"
    if (-not (Test-Path -LiteralPath $generationB -PathType Leaf)) { throw "Second protected profile generation is missing: $generationB" }
    Assert-ActiveRuntimeProfile -ExpectedBytes $profileB -ExpectedDigest $digestB -Phase 'publish B'
    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'rollback'
    $activePointer = Join-Path $profileRoot 'active.json'
    $previousPointer = Join-Path $profileRoot 'previous.json'
    if (-not (Test-Path -LiteralPath $activePointer -PathType Leaf)) { throw "Protected profile active pointer is missing: $activePointer" }
    if (-not (Test-Path -LiteralPath $previousPointer -PathType Leaf)) { throw "Protected profile rollback pointer is missing: $previousPointer" }
    $active = Get-Content -LiteralPath $activePointer -Raw | ConvertFrom-Json
    $previous = Get-Content -LiteralPath $previousPointer -Raw | ConvertFrom-Json
    if ($active.schema -ne 1 -or $active.generation -ne "sha256:$digestA") {
        throw 'Profile rollback did not restore the previous immutable generation.'
    }
    if ($previous.schema -ne 1 -or $previous.generation -ne "sha256:$digestB") {
        throw 'Profile rollback did not retain the displaced generation as the next rollback target.'
    }
    Assert-ActiveRuntimeProfile -ExpectedBytes $profileA -ExpectedDigest $digestA -Phase 'rollback to A'
    Assert-ProtectedTree $profileRoot

    $runtimePointer = Get-Content -LiteralPath (Join-Path $machineRoot 'current.json') -Raw | ConvertFrom-Json -AsHashtable
    $activeRuntimeRoot = Join-Path $machineRoot "bin\$($runtimePointer.version)"
    $markerResults = @(& (Join-Path $PSScriptRoot 'Test-OpenServiceMarkersWindows.ps1') `
        -Marker32 $Marker32 `
        -Marker64 $Marker64 `
        -ExpectedRuntimeRoot $activeRuntimeRoot)
    Assert-PersistedReadyHealth -ExpectedDigest $digestA -Phase 'x86/x64 marker injection'
    Assert-GenerationBoundMarkerTelemetry -MarkerResults $markerResults -ExpectedDigest $digestA

    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'stop'
    if ((Get-Service -Name $serviceName).Status -ne 'Stopped') { throw "$serviceName did not stop." }
    $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'repair'
    if ((Get-Service -Name $serviceName).Status -ne 'Stopped') { throw 'Repair changed a previously stopped service to Running.' }
    Assert-ActiveRuntimeProfile -ExpectedBytes $profileA -ExpectedDigest $digestA -Phase 'stopped repair'
    if ($LeaveInstalledForReboot) {
        $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'start'
        Assert-PersistedReadyHealth -ExpectedDigest $digestA -Phase 'pre-reboot start'
        $leaveInstalled = $true
    }
} finally {
    if ($installationAttempted -and -not $leaveInstalled) {
        try {
            $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'stop'
        } catch { Write-Warning $_ }
        try {
            $null = Invoke-OpenServiceSetupLogged -SetupExecutable $stagedSetup -Verb 'remove'
        } catch { Write-Warning $_ }
    }
}

if ($leaveInstalled) {
    Write-Host 'Hosted Windows lifecycle passed and left the isolated service Ready for an operator-controlled reboot.'
} else {
    $deleteDeadline = [DateTime]::UtcNow.AddSeconds(15)
    while ((Get-Service -Name $serviceName -ErrorAction SilentlyContinue) -and [DateTime]::UtcNow -lt $deleteDeadline) {
        Start-Sleep -Milliseconds 200
    }
    if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) { throw "$serviceName was not removed from SCM." }
    Write-Host 'Hosted Windows open-service install/start/Ready/crash-restart/marker/stop/remove lifecycle passed.'
}
if (Get-Service -Name $productionServiceName -ErrorAction SilentlyContinue) { throw 'Hosted lifecycle unexpectedly left the production service registered.' }
