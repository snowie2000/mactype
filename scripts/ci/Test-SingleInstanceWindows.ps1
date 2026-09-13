[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Executable,
    [string] $InstallerScript = 'installer\mactype-control-center.iss',
    [ValidateRange(2, 32)]
    [int] $ProcessCount = 8,
    [ValidateRange(5, 120)]
    [int] $TimeoutSeconds = 30
)

$ErrorActionPreference = 'Stop'
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).Path
$markerRoot = Join-Path $env:TEMP ("mactype-single-instance-" + [Guid]::NewGuid().ToString('N'))
$readyMarker = Join-Path $markerRoot 'primary.ready'
$eventMarker = Join-Path $markerRoot 'activation-events.jsonl'
$previousReadyMarker = $env:MACTYPE_CI_SINGLE_INSTANCE_READY
$previousEventMarker = $env:MACTYPE_CI_SINGLE_INSTANCE_EVENTS
$processes = @()

New-Item -ItemType Directory -Force -Path $markerRoot | Out-Null

$resolvedInstallerScript = (Resolve-Path -LiteralPath $InstallerScript).Path
$installerSource = Get-Content -LiteralPath $resolvedInstallerScript -Raw
if ($installerSource -notmatch '(?m)^\s*PrivilegesRequired=admin\s*$' -or
    $installerSource -notmatch '(?m)^\s*DefaultDirName=\{autopf\}\\MacType Control Center\s*$' -or
    $installerSource -notmatch '(?m)^\s*UsePreviousAppDir=no\s*$') {
    throw 'The installer must use the fixed protected Program Files/admin contract.'
}

$manifestTool = Get-Command mt.exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source
if (-not $manifestTool) {
    $kitsBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $manifestTool = Get-ChildItem -LiteralPath $kitsBin -Filter mt.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\mt\.exe$' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $manifestTool) {
    throw 'Windows SDK mt.exe was not found; the execution-level policy cannot be verified.'
}
$manifestPath = Join-Path $markerRoot 'application.manifest'
& $manifestTool -nologo "-inputresource:$resolvedExecutable;#1" "-out:$manifestPath"
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $manifestPath)) {
    throw "mt.exe could not extract the application manifest (exit $LASTEXITCODE)."
}
$manifest = Get-Content -LiteralPath $manifestPath -Raw
if ($manifest -notmatch 'requestedExecutionLevel' -or
    $manifest -notmatch 'level\s*=\s*["'']asInvoker["'']' -or
    $manifest -notmatch 'uiAccess\s*=\s*["'']false["'']') {
    throw 'The Control Center manifest must request asInvoker with uiAccess=false.'
}

if (-not ('MacType.SingleInstance.NativeMethods' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

namespace MacType.SingleInstance {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct StartupInfo {
        public int cb;
        public IntPtr lpReserved;
        public IntPtr lpDesktop;
        public IntPtr lpTitle;
        public int dwX;
        public int dwY;
        public int dwXSize;
        public int dwYSize;
        public int dwXCountChars;
        public int dwYCountChars;
        public int dwFillAttribute;
        public int dwFlags;
        public short wShowWindow;
        public short cbReserved2;
        public IntPtr lpReserved2;
        public IntPtr hStdInput;
        public IntPtr hStdOutput;
        public IntPtr hStdError;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct ProcessInformation {
        public IntPtr hProcess;
        public IntPtr hThread;
        public int dwProcessId;
        public int dwThreadId;
    }

    public static class NativeMethods {
        public const uint CreateSuspended = 0x00000004;

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool CreateProcess(
            string applicationName,
            StringBuilder commandLine,
            IntPtr processAttributes,
            IntPtr threadAttributes,
            bool inheritHandles,
            uint creationFlags,
            IntPtr environment,
            string currentDirectory,
            ref StartupInfo startupInfo,
            out ProcessInformation processInformation);

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern uint ResumeThread(IntPtr threadHandle);

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetExitCodeProcess(IntPtr processHandle, out uint exitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool CloseHandle(IntPtr handle);
    }
}
'@
}

try {
    $env:MACTYPE_CI_SINGLE_INSTANCE_READY = $readyMarker
    $env:MACTYPE_CI_SINGLE_INSTANCE_EVENTS = $eventMarker

    for ($index = 0; $index -lt $ProcessCount; $index++) {
        $startupInfo = [MacType.SingleInstance.StartupInfo]::new()
        $startupInfo.cb = [Runtime.InteropServices.Marshal]::SizeOf($startupInfo)
        $processInfo = [MacType.SingleInstance.ProcessInformation]::new()
        $commandLine = [Text.StringBuilder]::new(
            ('"{0}" --tray --ci-single-instance-probe {1}' -f $resolvedExecutable, $index)
        )
        $created = [MacType.SingleInstance.NativeMethods]::CreateProcess(
            $resolvedExecutable,
            $commandLine,
            [IntPtr]::Zero,
            [IntPtr]::Zero,
            $false,
            [MacType.SingleInstance.NativeMethods]::CreateSuspended,
            [IntPtr]::Zero,
            (Split-Path -Parent $resolvedExecutable),
            [ref] $startupInfo,
            [ref] $processInfo
        )
        if (-not $created) {
            throw "CreateProcess failed for probe $index with Windows error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())."
        }
        $process = [Diagnostics.Process]::GetProcessById($processInfo.dwProcessId)
        $processes += [pscustomobject]@{
            Index = $index
            Process = $process
            ProcessHandle = $processInfo.hProcess
            ThreadHandle = $processInfo.hThread
        }
    }

    foreach ($record in $processes) {
        $resumeResult = [MacType.SingleInstance.NativeMethods]::ResumeThread($record.ThreadHandle)
        if ($resumeResult -eq [uint32]::MaxValue) {
            throw "ResumeThread failed for probe $($record.Index) with Windows error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())."
        }
        [MacType.SingleInstance.NativeMethods]::CloseHandle($record.ThreadHandle) | Out-Null
        $record.ThreadHandle = [IntPtr]::Zero
    }

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $events = @()
    do {
        Start-Sleep -Milliseconds 200
        if (Test-Path -LiteralPath $eventMarker) {
            try {
                $events = @(Get-Content -LiteralPath $eventMarker | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json })
            }
            catch {
                $events = @()
            }
        }
    } while ((-not (Test-Path -LiteralPath $readyMarker) -or $events.Count -lt ($ProcessCount - 1)) -and [DateTime]::UtcNow -lt $deadline)

    if (-not (Test-Path -LiteralPath $readyMarker)) {
        throw "No primary process reported readiness within $TimeoutSeconds seconds."
    }
    $primaryId = [int] (Get-Content -LiteralPath $readyMarker -Raw).Trim()
    $primary = $processes | Where-Object { $_.Process.Id -eq $primaryId }
    if ($null -eq $primary) {
        throw "Ready marker named unknown primary PID $primaryId."
    }

    foreach ($record in $processes | Where-Object { $_.Process.Id -ne $primaryId }) {
        $waitResult = [MacType.SingleInstance.NativeMethods]::WaitForSingleObject($record.ProcessHandle, 5000)
        if ($waitResult -ne 0) {
            throw "Secondary probe $($record.Index) (PID $($record.Process.Id)) did not exit."
        }
        [uint32] $exitCode = 0
        if (-not [MacType.SingleInstance.NativeMethods]::GetExitCodeProcess($record.ProcessHandle, [ref] $exitCode)) {
            throw "Could not read the exit code for secondary probe $($record.Index)."
        }
        if ($exitCode -ne 0) {
            throw "Secondary probe $($record.Index) exited with code $exitCode."
        }
    }

    $survivors = @($processes | Where-Object {
        [uint32] $exitCode = 0
        if (-not [MacType.SingleInstance.NativeMethods]::GetExitCodeProcess($_.ProcessHandle, [ref] $exitCode)) {
            throw "Could not read process state for probe $($_.Index)."
        }
        $exitCode -eq 259
    })
    if ($survivors.Count -ne 1 -or $survivors[0].Process.Id -ne $primaryId) {
        throw "Expected exactly primary PID $primaryId to survive, but found: $($survivors.Process.Id -join ', ')."
    }
    if ($events.Count -ne ($ProcessCount - 1)) {
        throw "Expected $($ProcessCount - 1) activation events, but found $($events.Count)."
    }
    if (@($events | Where-Object { -not $_.restored }).Count -ne 0) {
        throw 'At least one secondary activation failed to restore the existing main window.'
    }
    if (@($events | Where-Object { $_.pid -ne $primaryId }).Count -ne 0) {
        throw 'Activation events were not handled by the surviving primary process.'
    }
    if (@($events | Where-Object { [string]::IsNullOrWhiteSpace($_.cwd) }).Count -ne 0) {
        throw 'At least one activation event omitted its working directory.'
    }

    $probeIndexes = @($events | ForEach-Object {
        $probePosition = [Array]::IndexOf([object[]] $_.args, '--ci-single-instance-probe')
        if ($probePosition -lt 0 -or $probePosition + 1 -ge $_.args.Count) {
            throw 'An activation event did not preserve its probe argument.'
        }
        [int] $_.args[$probePosition + 1]
    } | Sort-Object -Unique)
    if ($probeIndexes.Count -ne ($ProcessCount - 1) -or $probeIndexes -contains $primary.Index) {
        throw "Activation probe set was incomplete or included the primary: $($probeIndexes -join ', ')."
    }

    Write-Host "PASS: $ProcessCount suspended launches produced one primary and $($events.Count) restored activations"
}
finally {
    foreach ($record in $processes) {
        if ($record.ThreadHandle -ne [IntPtr]::Zero) {
            [MacType.SingleInstance.NativeMethods]::CloseHandle($record.ThreadHandle) | Out-Null
        }
        if ($record.ProcessHandle -ne [IntPtr]::Zero) {
            [MacType.SingleInstance.NativeMethods]::CloseHandle($record.ProcessHandle) | Out-Null
            $record.ProcessHandle = [IntPtr]::Zero
        }
        if ($null -ne $record.Process) {
            $record.Process.Refresh()
            if (-not $record.Process.HasExited) {
                $record.Process.Kill($true)
                $record.Process.WaitForExit(5000) | Out-Null
            }
            $record.Process.Dispose()
        }
    }
    if ($null -eq $previousReadyMarker) {
        Remove-Item Env:MACTYPE_CI_SINGLE_INSTANCE_READY -ErrorAction SilentlyContinue
    }
    else {
        $env:MACTYPE_CI_SINGLE_INSTANCE_READY = $previousReadyMarker
    }
    if ($null -eq $previousEventMarker) {
        Remove-Item Env:MACTYPE_CI_SINGLE_INSTANCE_EVENTS -ErrorAction SilentlyContinue
    }
    else {
        $env:MACTYPE_CI_SINGLE_INSTANCE_EVENTS = $previousEventMarker
    }
    Remove-Item -LiteralPath $markerRoot -Recurse -Force -ErrorAction SilentlyContinue
}
