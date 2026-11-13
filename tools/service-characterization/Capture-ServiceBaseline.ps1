[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $OutputDirectory,

    [ValidatePattern('^[A-Za-z0-9_.-]+$')]
    [string] $ServiceName = 'MacType',

    [ValidateSet('pre-install', 'post-install', 'started', 'stopped', 'removed', 'test')]
    [string] $Phase = 'pre-install',

    [string] $MacTrayPath = (Join-Path $(
        if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles }
    ) 'MacType\MacTray.exe')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Import-Module (Join-Path $PSScriptRoot 'lib\CharacterizationIO.psm1') -Force

$captures = [System.Collections.Generic.List[object]]::new()

function Add-Capture {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [int] $ExitCode,
        [Parameter(Mandatory)] [string] $Command
    )

    $captures.Add([ordered]@{
        name = $Name
        path = [System.IO.Path]::GetRelativePath($OutputDirectory, $Path)
        exitCode = $ExitCode
        command = $Command
    })
}

function Invoke-ExternalCapture {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $FileName,
        [Parameter(Mandatory)] [string[]] $Arguments,
        [Parameter(Mandatory)] [string] $ArtifactName
    )

    $artifactPath = Join-Path $OutputDirectory $ArtifactName
    $command = $FileName + ' ' + (($Arguments | ForEach-Object {
        if ($_ -match '\s') { '"' + $_.Replace('"', '\"') + '"' } else { $_ }
    }) -join ' ')
    $output = & $FileName @Arguments 2>&1 | Out-String
    $exitCode = if ($null -eq $LASTEXITCODE) { 1 } else { [int] $LASTEXITCODE }
    Write-CharacterizationText -Path $artifactPath -Value $output
    Add-Capture -Name $Name -Path $artifactPath -ExitCode $exitCode -Command $command
}

function Invoke-ScriptCapture {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $ArtifactName,
        [Parameter(Mandatory)] [scriptblock] $Operation
    )

    $artifactPath = Join-Path $OutputDirectory $ArtifactName
    try {
        $value = & $Operation
        Write-CharacterizationText -Path $artifactPath `
            -Value ($value | ConvertTo-Json -Depth 8)
        Add-Capture -Name $Name -Path $artifactPath -ExitCode 0 -Command $Name
    } catch {
        Write-CharacterizationText -Path $artifactPath -Value ([ordered]@{
            error = $_.Exception.Message
            category = $_.CategoryInfo.Category.ToString()
        } | ConvertTo-Json)
        Add-Capture -Name $Name -Path $artifactPath -ExitCode 1 -Command $Name
    }
}

function Invoke-TextScriptCapture {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $ArtifactName,
        [Parameter(Mandatory)] [scriptblock] $Operation
    )

    $artifactPath = Join-Path $OutputDirectory $ArtifactName
    try {
        $value = & $Operation | Out-String -Width 4096
        Write-CharacterizationText -Path $artifactPath -Value $value
        Add-Capture -Name $Name -Path $artifactPath -ExitCode 0 -Command $Name
    } catch {
        Write-CharacterizationText -Path $artifactPath `
            -Value ($_.Exception.Message + "`r`n")
        Add-Capture -Name $Name -Path $artifactPath -ExitCode 1 -Command $Name
    }
}

New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path

$scCommands = [ordered]@{
    'sc-qc' = @('qc', $ServiceName)
    'sc-queryex' = @('queryex', $ServiceName)
    'sc-description' = @('qdescription', $ServiceName)
    'sc-failure' = @('qfailure', $ServiceName)
    'sc-failureflag' = @('qfailureflag', $ServiceName)
    'sc-triggerinfo' = @('qtriggerinfo', $ServiceName)
    'sc-privs' = @('qprivs', $ServiceName)
    'sc-sd' = @('sdshow', $ServiceName)
}
foreach ($entry in $scCommands.GetEnumerator()) {
    Invoke-ExternalCapture -Name $entry.Key -FileName "$env:SystemRoot\System32\sc.exe" `
        -Arguments $entry.Value -ArtifactName "$($entry.Key).txt"
}

Invoke-ScriptCapture -Name 'cim-service' -ArtifactName 'cim-service.json' -Operation {
    $escaped = $ServiceName.Replace("'", "''")
    $service = Get-CimInstance Win32_Service -Filter "Name='$escaped'" -ErrorAction Stop
    if ($null -eq $service) {
        throw "Service '$ServiceName' was not found"
    }
    $service | Select-Object Name, DisplayName, Description, State, Status, StartMode,
        Started, AcceptPause, AcceptStop, DesktopInteract, ErrorControl, ExitCode,
        PathName, ProcessId, ServiceSpecificExitCode, ServiceType, StartName, TagId,
        CheckPoint, WaitHint, SystemName
}

Invoke-TextScriptCapture -Name 'cim-service-raw' -ArtifactName 'cim-service.txt' -Operation {
    $escaped = $ServiceName.Replace("'", "''")
    $service = Get-CimInstance Win32_Service -Filter "Name='$escaped'" -ErrorAction Stop
    if ($null -eq $service) {
        throw "Service '$ServiceName' was not found"
    }
    $service | Format-List *
}

$registryExport = Join-Path $OutputDirectory 'service.reg'
Invoke-ExternalCapture -Name 'registry-export' -FileName "$env:SystemRoot\System32\reg.exe" `
    -Arguments @('export', "HKLM\SYSTEM\CurrentControlSet\Services\$ServiceName", $registryExport, '/y') `
    -ArtifactName 'service-reg-export.txt'

Invoke-ScriptCapture -Name 'mactray-hash' -ArtifactName 'mactray-hash.json' -Operation {
    if (-not (Test-Path -LiteralPath $MacTrayPath -PathType Leaf)) {
        throw "MacTray executable was not found: $MacTrayPath"
    }
    Get-FileHash -LiteralPath $MacTrayPath -Algorithm SHA256 |
        Select-Object Algorithm, Hash, Path
}

Invoke-ScriptCapture -Name 'mactray-signature' -ArtifactName 'mactray-signature.json' -Operation {
    if (-not (Test-Path -LiteralPath $MacTrayPath -PathType Leaf)) {
        throw "MacTray executable was not found: $MacTrayPath"
    }
    Get-AuthenticodeSignature -LiteralPath $MacTrayPath |
        Select-Object Status, StatusMessage, Path, SignatureType, IsOSBinary,
            @{ Name = 'SignerSubject'; Expression = { if ($_.SignerCertificate) { $_.SignerCertificate.Subject } else { $null } } },
            @{ Name = 'SignerThumbprint'; Expression = { if ($_.SignerCertificate) { $_.SignerCertificate.Thumbprint } else { $null } } }
}

$installationRoot = Split-Path -Parent $MacTrayPath
if (Test-Path -LiteralPath $installationRoot -PathType Container) {
    Invoke-ExternalCapture -Name 'mactype-acl' -FileName "$env:SystemRoot\System32\icacls.exe" `
        -Arguments @($installationRoot, '/T', '/C') -ArtifactName 'mactype-acl.txt'
} else {
    $aclPath = Join-Path $OutputDirectory 'mactype-acl.txt'
    Write-CharacterizationText -Path $aclPath `
        -Value "Installation directory not found: $installationRoot`r`n"
    Add-Capture -Name 'mactype-acl' -Path $aclPath -ExitCode 2 -Command "icacls `"$installationRoot`" /T /C"
}

$failed = @($captures | Where-Object exitCode -ne 0).Count
$baseline = [ordered]@{
    schemaVersion = 1
    tool = 'Capture-ServiceBaseline'
    capturedAtUtc = [DateTimeOffset]::UtcNow.ToString('o')
    characterizationStatus = 'UNKNOWN'
    captureStatus = if ($failed -eq 0) { 'complete' } else { 'complete-with-errors' }
    phase = $Phase
    serviceName = $ServiceName
    macTrayPath = $MacTrayPath
    host = [ordered]@{
        computerName = $env:COMPUTERNAME
        osVersion = [Environment]::OSVersion.VersionString
        processArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
        powerShellVersion = $PSVersionTable.PSVersion.ToString()
    }
    failedCaptureCount = $failed
    captures = $captures
}
Write-CharacterizationJson -Path (Join-Path $OutputDirectory 'baseline.json') `
    -Value $baseline -Depth 8

Write-Host "Baseline captured at $OutputDirectory ($failed command(s) unavailable)."
exit 0
