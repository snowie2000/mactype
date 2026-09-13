Set-StrictMode -Version Latest

$script:MaximumSetupOutputBytes = 64 * 1024
$script:SetupTerminationTimeoutMilliseconds = 5000
$script:MaximumHealthSnapshotBytes = 16 * 1024

if (-not ('MacType.ControlCenter.Ci.BoundedProcessRunner' -as [type])) {
    Add-Type -Path @(
        (Join-Path $PSScriptRoot 'BoundedProcessRunner.cs'),
        (Join-Path $PSScriptRoot 'BoundedProcessIo.cs'),
        (Join-Path $PSScriptRoot 'WindowsProcessJob.cs')
    )
}

function Invoke-OpenServiceSetup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $SetupExecutable,
        [Parameter(Mandatory)] [string] $Verb,
        [byte[]] $InputBytes = $null,
        [switch] $ExpectFailure,
        [ValidateRange(1, [int]::MaxValue)] [int] $TimeoutMilliseconds = 60000
    )

    if (-not (Test-Path -LiteralPath $SetupExecutable -PathType Leaf)) {
        throw "Setup executable is missing: $SetupExecutable"
    }

    $result = [MacType.ControlCenter.Ci.BoundedProcessRunner]::Run(
        (Resolve-Path -LiteralPath $SetupExecutable).Path,
        $Verb,
        $InputBytes,
        $TimeoutMilliseconds,
        $script:MaximumSetupOutputBytes,
        $script:SetupTerminationTimeoutMilliseconds
    )
    if ($ExpectFailure) {
        if ($result.ExitCode -eq 0) {
            throw "Setup verb '$Verb' unexpectedly succeeded."
        }
    } elseif ($result.ExitCode -ne 0) {
        throw "Setup verb '$Verb' failed with $($result.ExitCode). " +
            "stdout=$($result.StandardOutput) stderr=$($result.StandardError)"
    }
    return $result
}

function Read-OpenServiceHealthSnapshot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Path,
        [string] $Context = 'The protected health snapshot'
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "${Context} is missing: $Path"
    }
    $item = Get-Item -LiteralPath $Path -Force
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0 -or
        $item.Length -le 0 -or $item.Length -gt $script:MaximumHealthSnapshotBytes) {
        throw "${Context} is not a bounded regular file."
    }

    $stream = [System.IO.FileStream]::new(
        $item.FullName,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read
    )
    try {
        $bytes = [byte[]]::new($script:MaximumHealthSnapshotBytes + 1)
        $total = 0
        while ($total -lt $bytes.Length) {
            $read = $stream.Read($bytes, $total, $bytes.Length - $total)
            if ($read -eq 0) { break }
            $total += $read
        }
        if ($total -eq 0 -or $total -gt $script:MaximumHealthSnapshotBytes) {
            throw "${Context} is not a bounded regular file."
        }
        $json = [System.Text.UTF8Encoding]::new($false, $true).GetString($bytes, 0, $total)
        return $json | ConvertFrom-Json
    } finally {
        $stream.Dispose()
    }
}

function Invoke-OpenServiceSetupLogged {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $SetupExecutable,
        [Parameter(Mandatory)] [string] $Verb,
        [byte[]] $InputBytes = $null,
        [switch] $ExpectFailure,
        [ValidateRange(1, [int]::MaxValue)] [int] $TimeoutMilliseconds = 60000
    )

    $result = Invoke-OpenServiceSetup -SetupExecutable $SetupExecutable `
        -Verb $Verb -InputBytes $InputBytes -ExpectFailure:$ExpectFailure `
        -TimeoutMilliseconds $TimeoutMilliseconds
    Write-Host "Setup verb '$Verb' completed with exit code $($result.ExitCode). $($result.StandardOutput)"
    return $result
}

function Get-LowerFileSha256 {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $Path)

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-OpenServiceStrictReady {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $ServiceName,
        [Parameter(Mandatory)] [string] $HealthPath
    )

    $service = Get-CimInstance Win32_Service -Filter "Name='$ServiceName'"
    if (-not $service -or $service.State -ne 'Running' -or
        $service.ProcessId -le 0) {
        throw "$ServiceName is not SCM Running with a live PID."
    }
    $health = Read-OpenServiceHealthSnapshot -Path $HealthPath
    if ($health.protocolVersion -ne 1 -or $health.health -ne 'ready' -or
        $health.lastError -or -not $health.activeProfileDigest -or
        $health.readiness.profile -ne 'ready' -or
        $health.readiness.observer -ne 'ready' -or
        $health.readiness.injector32 -ne 'ready' -or
        $health.readiness.injector64 -ne 'ready') {
        throw "$ServiceName did not publish strict Ready health."
    }
    return [pscustomobject]@{
        Service = $service
        Health = $health
    }
}

Export-ModuleMember -Function @(
    'Invoke-OpenServiceSetup',
    'Invoke-OpenServiceSetupLogged',
    'Read-OpenServiceHealthSnapshot',
    'Assert-OpenServiceStrictReady',
    'Get-LowerFileSha256'
)
