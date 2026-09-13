[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $Executable,
    [int] $TimeoutSeconds = 20
)

$ErrorActionPreference = 'Stop'
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).Path
$markerRoot = Join-Path $env:TEMP ("mactype-tray-smoke-" + [Guid]::NewGuid().ToString('N'))
$marker = Join-Path $markerRoot 'tray.ready'
New-Item -ItemType Directory -Force -Path $markerRoot | Out-Null

try {
    $env:MACTYPE_CI_SMOKE_FILE = $marker
    $process = Start-Process -FilePath $resolvedExecutable -ArgumentList @('--tray', '--ci-view', 'overview') -PassThru -WindowStyle Hidden
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while (-not $process.HasExited -and -not (Test-Path -LiteralPath $marker) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 200
        $process.Refresh()
    }
    if (-not (Test-Path -LiteralPath $marker)) {
        if (-not $process.HasExited) { $process.Kill($true) }
        throw "Tray startup did not report readiness within $TimeoutSeconds seconds."
    }
    $content = (Get-Content -LiteralPath $marker -Raw).Trim()
    if ($content -ne 'ready:overview') {
        throw "Tray startup failed: $content"
    }
    if (-not $process.WaitForExit(5000)) {
        $process.Kill($true)
        throw 'Tray smoke process did not exit.'
    }
    if ($process.ExitCode -ne 0) {
        throw "Tray smoke process exited with code $($process.ExitCode)."
    }
    Write-Host 'PASS: hidden tray startup'
}
finally {
    Remove-Item Env:MACTYPE_CI_SMOKE_FILE -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $markerRoot -Recurse -Force -ErrorAction SilentlyContinue
}
