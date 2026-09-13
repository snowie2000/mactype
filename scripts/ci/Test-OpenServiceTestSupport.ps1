[CmdletBinding()]
param(
    [ValidateSet('all', 'stdin-timeout', 'descendant-pipe')]
    [string] $Only = 'all'
)

$ErrorActionPreference = 'Stop'
$modulePath = Join-Path $PSScriptRoot 'lib\OpenServiceTestSupport.psm1'
Import-Module $modulePath -Force
. (Join-Path $PSScriptRoot 'lib\InstallerWindowsAssertions.ps1')

function Assert-ThrowsLike {
    param(
        [Parameter(Mandatory)] [scriptblock] $Action,
        [Parameter(Mandatory)] [string] $Pattern
    )

    try {
        & $Action
    } catch {
        if ($_.Exception.Message -notmatch $Pattern) {
            throw "Expected error /$Pattern/, got: $($_.Exception.Message)"
        }
        return
    }
    throw "Expected action to fail with /$Pattern/."
}

function Assert-ProcessExited {
    param([Parameter(Mandatory)] [int] $Id)

    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (-not (Get-Process -Id $Id -ErrorAction SilentlyContinue)) { return }
        Start-Sleep -Milliseconds 50
    }
    throw "Process $Id survived bounded setup cleanup."
}

function Wait-TestPid {
    param([Parameter(Mandatory)] [string] $Path)

    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $Path -PathType Leaf) {
            return [int] ([System.IO.File]::ReadAllText($Path))
        }
        Start-Sleep -Milliseconds 25
    }
    throw "Test helper did not publish its PID: $Path"
}

function Get-ActiveTestJobHandleCount {
    $jobType = [MacType.ControlCenter.Ci.BoundedProcessRunner].Assembly.GetType(
        'MacType.ControlCenter.Ci.WindowsProcessJob',
        $true
    )
    $property = $jobType.GetProperty(
        'ActiveHandleCount',
        [Reflection.BindingFlags]'Static, NonPublic'
    )
    return [int] $property.GetValue($null)
}

$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) `
    "mactype-open-service-support-$PID-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
$helperPath = Join-Path $temporaryRoot 'OpenServiceTestChild.exe'

try {
    & rustc (Join-Path $PSScriptRoot 'fixtures\OpenServiceTestChild.rs') `
        -C opt-level=1 -o $helperPath
    if ($LASTEXITCODE -ne 0) { throw 'Could not compile the setup process test fixture.' }

    $invalidRunArguments = @(
        @{ Name = 'timeout below range'; Timeout = -2; Output = 65536; Termination = 5000; Input = $null; Pattern = 'timeoutMilliseconds' },
        @{ Name = 'timeout above range'; Timeout = 600001; Output = 65536; Termination = 5000; Input = $null; Pattern = 'timeoutMilliseconds' },
        @{ Name = 'output below range'; Timeout = 1000; Output = 0; Termination = 5000; Input = $null; Pattern = 'maximumOutputBytes' },
        @{ Name = 'output above range'; Timeout = 1000; Output = 1048577; Termination = 5000; Input = $null; Pattern = 'maximumOutputBytes' },
        @{ Name = 'termination below range'; Timeout = 1000; Output = 65536; Termination = 0; Input = $null; Pattern = 'terminationTimeoutMilliseconds' },
        @{ Name = 'termination above range'; Timeout = 1000; Output = 65536; Termination = 60001; Input = $null; Pattern = 'terminationTimeoutMilliseconds' },
        @{ Name = 'input above range'; Timeout = 1000; Output = 65536; Termination = 5000; Input = [byte[]]::new((4 * 1024 * 1024) + 1); Pattern = 'input' }
    )
    foreach ($case in $invalidRunArguments) {
        if ((Get-ActiveTestJobHandleCount) -ne 0) {
            throw "Pre-start resource baseline is not zero before $($case.Name)."
        }
        $message = $null
        try {
            [MacType.ControlCenter.Ci.BoundedProcessRunner]::Run(
                $helperPath,
                'ok',
                $case.Input,
                $case.Timeout,
                $case.Output,
                $case.Termination
            )
        } catch {
            $message = $_.Exception.ToString()
        }
        if ((Get-ActiveTestJobHandleCount) -ne 0) {
            throw "$($case.Name) leaked a Windows Job HANDLE before process start."
        }
        if (-not $message -or $message -notmatch $case.Pattern) {
            throw "$($case.Name) was not rejected by pre-resource validation: $message"
        }
    }

    $success = Invoke-OpenServiceSetup -SetupExecutable $helperPath -Verb 'ok'
    if ($success.ExitCode -ne 0 -or $success.StandardOutput -cne 'stdout-ok' -or
        $success.StandardError -cne 'stderr-ok') {
        throw 'Successful setup output contract changed.'
    }

    $argumentVector = @(
        'arguments',
        'value with spaces',
        '/DIR=C:\Program Files\MacType Control Center',
        ''
    )
    $argumentResult = [MacType.ControlCenter.Ci.BoundedProcessRunner]::RunArguments(
        $helperPath,
        $argumentVector,
        $null,
        5000,
        65536,
        5000
    )
    if ($argumentResult.ExitCode -ne 0 -or
        $argumentResult.StandardOutput -cne "value with spaces`n/DIR=C:\Program Files\MacType Control Center`n" -or
        $argumentResult.StandardError -cne '') {
        throw "Bounded process runner did not preserve the exact argument vector: " +
            "exit=$($argumentResult.ExitCode) stdout=<$($argumentResult.StandardOutput)> " +
            "stderr=<$($argumentResult.StandardError)>"
    }

    Assert-ThrowsLike -Pattern 'stdout-fail.*stderr-fail' -Action {
        Invoke-ExpectedSuccess -File $helperPath -Arguments @('fail') -Label 'Diagnostic success assertion'
    }
    Assert-ThrowsLike -Pattern 'stdout-ok.*stderr-ok' -Action {
        Invoke-ExpectedFailure -File $helperPath -Arguments @('ok') -Label 'Diagnostic failure assertion'
    }

    $installerTimeoutPidPath = Join-Path $temporaryRoot 'installer-timeout-child.pid'
    Assert-ThrowsLike -Pattern 'timed out.*process tree.*terminated' -Action {
        Invoke-ProcessExit -File $helperPath `
            -Arguments @("timeout|$installerTimeoutPidPath") `
            -TimeoutMilliseconds 300
    }
    Assert-ProcessExited -Id (Wait-TestPid -Path $installerTimeoutPidPath)

    $expectedFailure = Invoke-OpenServiceSetup -SetupExecutable $helperPath `
        -Verb 'fail' -ExpectFailure
    if ($expectedFailure.ExitCode -ne 9 -or
        $expectedFailure.StandardOutput -cne 'stdout-fail' -or
        $expectedFailure.StandardError -cne 'stderr-fail') {
        throw 'Expected-failure setup output contract changed.'
    }
    Assert-ThrowsLike -Pattern 'failed with 9' -Action {
        Invoke-OpenServiceSetup -SetupExecutable $helperPath -Verb 'fail'
    }

    $bothStreams = Invoke-OpenServiceSetup -SetupExecutable $helperPath `
        -Verb 'both-near-limit' -TimeoutMilliseconds 5000
    if ($bothStreams.StandardOutput.Length -ne (60 * 1024) -or
        $bothStreams.StandardError.Length -ne (60 * 1024)) {
        throw 'stdout and stderr were not drained concurrently without truncation.'
    }

    foreach ($streamName in @('stdout', 'stderr')) {
        $pidPath = Join-Path $temporaryRoot "$streamName-overflow.pid"
        $watch = [Diagnostics.Stopwatch]::StartNew()
        Assert-ThrowsLike -Pattern "$streamName.*65536.*bytes" -Action {
            Invoke-OpenServiceSetup -SetupExecutable $helperPath `
                -Verb "$streamName-overflow|$pidPath" -TimeoutMilliseconds 10000
        }
        $watch.Stop()
        if ($watch.Elapsed -ge [TimeSpan]::FromSeconds(5)) {
            throw "$streamName overflow was not rejected promptly."
        }
        Assert-ProcessExited -Id (Wait-TestPid -Path $pidPath)
    }

    $childPidPath = Join-Path $temporaryRoot 'timeout-child.pid'
    Assert-ThrowsLike -Pattern 'timed out.*process tree.*terminated' -Action {
        Invoke-OpenServiceSetup -SetupExecutable $helperPath `
            -Verb "timeout|$childPidPath" -TimeoutMilliseconds 300
    }
    Assert-ProcessExited -Id (Wait-TestPid -Path $childPidPath)

    if ($Only -in @('all', 'stdin-timeout')) {
        $stdinPidPath = Join-Path $temporaryRoot 'stdin-stall.pid'
        $largeInput = [byte[]]::new(2 * 1024 * 1024)
        $watch = [Diagnostics.Stopwatch]::StartNew()
        Assert-ThrowsLike -Pattern 'timed out.*process tree.*terminated' -Action {
            Invoke-OpenServiceSetup -SetupExecutable $helperPath `
                -Verb "stdin-stall|$stdinPidPath" -InputBytes $largeInput `
                -TimeoutMilliseconds 300
        }
        $watch.Stop()
        if ($watch.Elapsed -ge [TimeSpan]::FromSeconds(5)) {
            throw 'Blocked stdin escaped the setup process overall deadline.'
        }
        Assert-ProcessExited -Id (Wait-TestPid -Path $stdinPidPath)
    }

    if ($Only -in @('all', 'descendant-pipe')) {
        $pipeHolderPidPath = Join-Path $temporaryRoot 'pipe-holder.pid'
        $watch = [Diagnostics.Stopwatch]::StartNew()
        Assert-ThrowsLike -Pattern 'timed out.*process tree.*terminated' -Action {
            Invoke-OpenServiceSetup -SetupExecutable $helperPath `
                -Verb "exit-with-pipe-descendant|$pipeHolderPidPath" `
                -TimeoutMilliseconds 300
        }
        $watch.Stop()
        if ($watch.Elapsed -ge [TimeSpan]::FromSeconds(5)) {
            throw 'Descendant-held output pipes escaped the setup process overall deadline.'
        }
        Assert-ProcessExited -Id (Wait-TestPid -Path $pipeHolderPidPath)
    }

    $healthPath = Join-Path $temporaryRoot 'health.json'
    [System.IO.File]::WriteAllText(
        $healthPath,
        '{"protocolVersion":1,"health":"ready"}',
        [System.Text.UTF8Encoding]::new($false)
    )
    $health = Read-OpenServiceHealthSnapshot -Path $healthPath
    if ($health.protocolVersion -ne 1 -or $health.health -cne 'ready') {
        throw 'Health snapshot JSON decode contract changed.'
    }

    [System.IO.File]::WriteAllBytes($healthPath, [byte[]]::new((16 * 1024) + 1))
    Assert-ThrowsLike -Pattern 'bounded regular file' -Action {
        Read-OpenServiceHealthSnapshot -Path $healthPath
    }

    $supportSource = [System.IO.File]::ReadAllText($modulePath)
    $runnerSource = @(
        'BoundedProcessRunner.cs',
        'BoundedProcessIo.cs',
        'WindowsProcessJob.cs'
    ) | ForEach-Object {
        [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot "lib\$_"))
    }
    $runnerSource = $runnerSource -join "`n"
    if (($supportSource + $runnerSource).Contains('ReadToEndAsync')) {
        throw 'Setup process capture must never use unbounded ReadToEndAsync.'
    }
    if ($supportSource -match 'Get-Content\s+-LiteralPath\s+\$Path\s+-Raw') {
        throw 'Health snapshots must be bounded again while streaming; metadata length is raceable.'
    }
    foreach ($token in @('FileStream', 'MaximumHealthSnapshotBytes + 1')) {
        if (-not $supportSource.Contains($token)) {
            throw "Health snapshot bounded-read contract is missing '$token'."
        }
    }
} finally {
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
}

& (Join-Path $PSScriptRoot 'Test-OpenServiceAclFixture.ps1')

Write-Host 'Open service CI support bounded-process and bounded-health tests passed.'
