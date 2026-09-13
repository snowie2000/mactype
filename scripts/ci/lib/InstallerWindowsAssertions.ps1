Import-Module (Join-Path $PSScriptRoot 'OpenServiceTestSupport.psm1')

$script:InstallerProcessTimeoutMilliseconds = 10 * 60 * 1000
$script:InstallerProcessMaximumOutputBytes = 64 * 1024
$script:InstallerProcessTerminationTimeoutMilliseconds = 5000
$script:InstallerDiagnosticMaximumBytes = 128 * 1024

function Read-InstallerDiagnosticLog {
    param([Parameter(Mandatory)] [string] $Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return "<missing installer log: $Path>"
    }
    $stream = [IO.FileStream]::new(
        $Path,
        [IO.FileMode]::Open,
        [IO.FileAccess]::Read,
        [IO.FileShare]::ReadWrite
    )
    try {
        $length = $stream.Length
        $start = [Math]::Max(0L, $length - $script:InstallerDiagnosticMaximumBytes)
        if ($start -gt 0) { [void] $stream.Seek($start, [IO.SeekOrigin]::Begin) }
        $bytes = [byte[]]::new([int] [Math]::Min(
            $script:InstallerDiagnosticMaximumBytes,
            $length - $start
        ))
        $total = 0
        while ($total -lt $bytes.Length) {
            $read = $stream.Read($bytes, $total, $bytes.Length - $total)
            if ($read -eq 0) { break }
            $total += $read
        }
        $text = [Text.Encoding]::UTF8.GetString($bytes, 0, $total)
        if ($start -gt 0) { return "<truncated to final $($bytes.Length) bytes>`n$text" }
        return $text
    }
    finally {
        $stream.Dispose()
    }
}

function Get-FixedService {
    param([Parameter(Mandatory)] [string] $Name)

    Get-CimInstance Win32_Service -Filter "Name='$Name'" -ErrorAction SilentlyContinue
}

function Get-ServiceSnapshot {
    param([Parameter(Mandatory)] [string] $Name)

    $service = Get-FixedService -Name $Name
    if (-not $service) { return $null }
    @($service.PathName, $service.StartMode, $service.StartName, $service.DisplayName, $service.State) -join "`n"
}

function Get-ServiceExecutablePath {
    param([Parameter(Mandatory)] [string] $ImagePath)

    if ($ImagePath -match '^"([^"]+)"(?:\s.*)?$') { return $Matches[1] }
    ($ImagePath -split '\s+', 2)[0]
}

function Invoke-ProcessExit {
    param(
        [Parameter(Mandatory)] [string] $File,
        [Parameter(Mandatory)] [string[]] $Arguments,
        [ValidateRange(1, 600000)]
        [int] $TimeoutMilliseconds = $script:InstallerProcessTimeoutMilliseconds
    )

    $resolvedFile = (Resolve-Path -LiteralPath $File).Path
    [MacType.ControlCenter.Ci.BoundedProcessRunner]::RunArguments(
        $resolvedFile,
        $Arguments,
        $null,
        $TimeoutMilliseconds,
        $script:InstallerProcessMaximumOutputBytes,
        $script:InstallerProcessTerminationTimeoutMilliseconds
    )
}

function Invoke-ExpectedSuccess {
    param(
        [Parameter(Mandatory)] [string] $File,
        [Parameter(Mandatory)] [string[]] $Arguments,
        [Parameter(Mandatory)] [string] $Label,
        [string] $DiagnosticLogPath
    )

    $effectiveArguments = if ($DiagnosticLogPath) {
        @($Arguments) + "/LOG=$DiagnosticLogPath"
    } else {
        $Arguments
    }
    $result = Invoke-ProcessExit -File $File -Arguments $effectiveArguments
    if ($result.ExitCode -ne 0) {
        $diagnostic = if ($DiagnosticLogPath) {
            " installer-log=$(Read-InstallerDiagnosticLog -Path $DiagnosticLogPath)"
        } else { '' }
        throw "$Label exited with code $($result.ExitCode). " +
            "stdout=$($result.StandardOutput) stderr=$($result.StandardError)$diagnostic"
    }
}

function Invoke-ExpectedFailure {
    param(
        [Parameter(Mandatory)] [string] $File,
        [Parameter(Mandatory)] [string[]] $Arguments,
        [Parameter(Mandatory)] [string] $Label,
        [string] $DiagnosticLogPath
    )

    $effectiveArguments = if ($DiagnosticLogPath) {
        @($Arguments) + "/LOG=$DiagnosticLogPath"
    } else {
        $Arguments
    }
    $result = Invoke-ProcessExit -File $File -Arguments $effectiveArguments
    if ($result.ExitCode -eq 0) {
        $diagnostic = if ($DiagnosticLogPath) {
            " installer-log=$(Read-InstallerDiagnosticLog -Path $DiagnosticLogPath)"
        } else { '' }
        throw "$Label unexpectedly succeeded. " +
            "stdout=$($result.StandardOutput) stderr=$($result.StandardError)$diagnostic"
    }
}

function Initialize-UserMarkers {
    param(
        [Parameter(Mandatory)] [string] $UserMarkerRoot,
        [Parameter(Mandatory)] [System.Collections.IDictionary] $UserMarkers
    )

    foreach ($entry in $UserMarkers.GetEnumerator()) {
        $path = Join-Path $UserMarkerRoot $entry.Key
        New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
        [IO.File]::WriteAllText($path, $entry.Value, [Text.UTF8Encoding]::new($false))
    }
}

function Assert-UserMarkers {
    param(
        [Parameter(Mandatory)] [string] $UserMarkerRoot,
        [Parameter(Mandatory)] [System.Collections.IDictionary] $UserMarkers
    )

    foreach ($entry in $UserMarkers.GetEnumerator()) {
        $path = Join-Path $UserMarkerRoot $entry.Key
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Installer removed LocalAppData user state: $($entry.Key)"
        }
        if ([IO.File]::ReadAllText($path) -cne $entry.Value) {
            throw "Installer changed LocalAppData user state: $($entry.Key)"
        }
    }
}

function Get-TreeSnapshot {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [string] $ExcludedRoot
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "Cannot snapshot missing directory: $Path"
    }
    function Test-RelativePathEscapesRoot {
        param([Parameter(Mandatory)] [string] $RelativePath)

        if ([IO.Path]::IsPathRooted($RelativePath) -or $RelativePath -eq '..') {
            return $true
        }
        foreach ($separator in @(
            [IO.Path]::DirectorySeparatorChar,
            [IO.Path]::AltDirectorySeparatorChar
        ) | Select-Object -Unique) {
            if ($RelativePath.StartsWith("..$separator", [StringComparison]::Ordinal)) {
                return $true
            }
        }
        return $false
    }

    $resolvedRoot = [IO.Path]::GetFullPath($Path)
    $resolvedExcludedRoot = $null
    if ($ExcludedRoot) {
        $resolvedExcludedRoot = [IO.Path]::GetFullPath($ExcludedRoot)
        $relativeExcludedRoot = [IO.Path]::GetRelativePath($resolvedRoot, $resolvedExcludedRoot)
        if (Test-RelativePathEscapesRoot -RelativePath $relativeExcludedRoot) {
            throw "Excluded snapshot root is outside the application tree: $ExcludedRoot"
        }
    }
    @(
        Get-ChildItem -LiteralPath $resolvedRoot -Recurse -File |
            Where-Object {
                if (-not $resolvedExcludedRoot) { return $true }
                $relativeToExcludedRoot = [IO.Path]::GetRelativePath(
                    $resolvedExcludedRoot,
                    $_.FullName
                )
                Test-RelativePathEscapesRoot -RelativePath $relativeToExcludedRoot
            } |
            Sort-Object FullName |
            ForEach-Object {
            $relative = [IO.Path]::GetRelativePath($resolvedRoot, $_.FullName)
            $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            "$relative|$($_.Length)|$hash"
        }
    ) -join "`n"
}

function Get-TreeSnapshotDifference {
    param(
        [Parameter(Mandatory)] [AllowEmptyString()] [string] $ExpectedSnapshot,
        [Parameter(Mandatory)] [AllowEmptyString()] [string] $ActualSnapshot
    )

    function ConvertTo-TreeSnapshotMap {
        param([Parameter(Mandatory)] [AllowEmptyString()] [string] $Snapshot)

        $map = @{}
        foreach ($line in ($Snapshot -split "`n")) {
            if ($line.Length -eq 0) { continue }
            if ($line -notmatch '^(?<path>[^|]+)\|(?<length>[0-9]+)\|(?<hash>[0-9a-f]{64})$') {
                throw "Tree snapshot entry is invalid: $line"
            }
            $map[$Matches.path] = "$($Matches.length)|$($Matches.hash)"
        }
        return $map
    }

    $expected = ConvertTo-TreeSnapshotMap -Snapshot $ExpectedSnapshot
    $actual = ConvertTo-TreeSnapshotMap -Snapshot $ActualSnapshot
    @(
        @($expected.Keys) + @($actual.Keys) |
            Sort-Object -Unique |
            ForEach-Object {
                $relative = $_
                $hasExpected = $expected.ContainsKey($relative)
                $hasActual = $actual.ContainsKey($relative)
                if (-not $hasExpected) {
                    "added|$relative|actual=$($actual[$relative])"
                }
                elseif (-not $hasActual) {
                    "removed|$relative|expected=$($expected[$relative])"
                }
                elseif ($expected[$relative] -cne $actual[$relative]) {
                    "changed|$relative|expected=$($expected[$relative])|actual=$($actual[$relative])"
                }
            }
    ) -join "`n"
}

function Get-BoundedTreeInventory {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [ValidateRange(1, 256)] [int] $MaximumEntries = 128
    )

    if (-not (Test-Path -LiteralPath $Path)) { return 'missing|.' }
    $root = Get-Item -LiteralPath $Path -Force
    $resolvedRoot = [IO.Path]::GetFullPath($root.FullName)
    if (-not $root.PSIsContainer) {
        $hash = (Get-FileHash -LiteralPath $root.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        return "file|.|$($root.Length)|$hash"
    }

    $lines = [Collections.Generic.List[string]]::new()
    [void] $lines.Add('directory|.')
    $pending = [Collections.Generic.Queue[IO.DirectoryInfo]]::new()
    $pending.Enqueue($root)
    $observedEntries = 0
    $truncated = $false
    while ($pending.Count -gt 0 -and -not $truncated) {
        $directory = $pending.Dequeue()
        $remaining = ($MaximumEntries + 1) - $observedEntries
        $children = @(
            Get-ChildItem -LiteralPath $directory.FullName -Force |
                Select-Object -First $remaining
        )
        foreach ($child in $children) {
            $observedEntries += 1
            if ($observedEntries -gt $MaximumEntries) {
                $truncated = $true
                break
            }
            $relative = [IO.Path]::GetRelativePath($resolvedRoot, $child.FullName)
            $isReparse = ($child.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
            if ($child.PSIsContainer) {
                if ($isReparse) {
                    [void] $lines.Add("reparse-directory|$relative")
                }
                else {
                    [void] $lines.Add("directory|$relative")
                    $pending.Enqueue($child)
                }
                continue
            }
            if ($isReparse) {
                [void] $lines.Add("reparse-file|$relative|$($child.Length)")
                continue
            }
            try {
                $hash = (Get-FileHash -LiteralPath $child.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                [void] $lines.Add("file|$relative|$($child.Length)|$hash")
            }
            catch {
                [void] $lines.Add("file-unreadable|$relative|$($child.Length)|$($_.Exception.Message)")
            }
        }
    }
    if ($truncated -or $pending.Count -gt 0) {
        [void] $lines.Add("truncated|maximum-entries=$MaximumEntries")
    }
    return $lines -join "`n"
}

function Wait-PathAbsent {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [ValidateRange(1, 60000)] [int] $TimeoutMilliseconds = 15000,
        [ValidateRange(1, 1000)] [int] $PollMilliseconds = 50
    )

    $watch = [Diagnostics.Stopwatch]::StartNew()
    while ($watch.ElapsedMilliseconds -lt $TimeoutMilliseconds) {
        if (-not (Test-Path -LiteralPath $Path)) { return }
        $remaining = $TimeoutMilliseconds - [int] $watch.ElapsedMilliseconds
        Start-Sleep -Milliseconds ([Math]::Min($PollMilliseconds, [Math]::Max(1, $remaining)))
    }
    if (-not (Test-Path -LiteralPath $Path)) { return }
    $inventory = Get-BoundedTreeInventory -Path $Path
    throw "Path did not disappear within $TimeoutMilliseconds ms: $Path`nRemaining tree:`n$inventory"
}

function Assert-ServicePayload {
    param([Parameter(Mandatory)] [string] $ApplicationRoot)

    $runtimeRoot = Join-Path $ApplicationRoot 'service-runtime'
    $manifestPath = Join-Path $runtimeRoot 'payload\manifest.json'
    $payloadFiles = Join-Path $runtimeRoot 'payload\files'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -AsHashtable
    if ($manifest.Count -ne 3 -or $manifest.schema -ne 1 -or $manifest.version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+') {
        throw 'Installed open-service manifest violates the fixed schema/version contract.'
    }
    $expectedNames = @('mactype-service.exe', 'mactype-injector32.exe', 'mactype-injector64.exe', 'MacType.dll', 'MacType64.dll')
    if (Compare-Object ($expectedNames | Sort-Object) ($manifest.files.Keys | Sort-Object)) {
        throw 'Installed open-service manifest contains a missing or unapproved payload filename.'
    }
    $expectedRuntimeFiles = @(
        'mactype-service-setup.exe',
        'payload\manifest.json'
        $expectedNames | ForEach-Object { "payload\files\$_" }
    )
    $installedRuntimeFiles = @(
        Get-ChildItem -LiteralPath $runtimeRoot -Recurse -File | ForEach-Object {
            [IO.Path]::GetRelativePath($runtimeRoot, $_.FullName)
        }
    )
    if (Compare-Object ($expectedRuntimeFiles | Sort-Object) ($installedRuntimeFiles | Sort-Object)) {
        throw 'Installed app-side service runtime contains a missing or obsolete file.'
    }
    foreach ($name in $expectedNames) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $payloadFiles $name) -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($manifest.files[$name] -cne "sha256:$hash") {
            throw "Installed open-service payload hash does not match manifest: $name"
        }
    }
    $manifest
}

function Assert-RequiredApplicationFiles {
    param([Parameter(Mandatory)] [string] $ApplicationRoot)

    $expected = @(
        'MacType Control Center.exe',
        'mactype-preview32.exe',
        'MacType.dll',
        'MacType64.dll',
        'MacType.Core.dll',
        'MacType64.Core.dll',
        'MacLoader.exe',
        'MacLoader64.exe',
        'MacType.ini',
        'ini\Default.ini',
        'languages\en.json',
        'languages\ko.json',
        'THIRD_PARTY_NOTICES.md',
        'LICENSE.txt',
        'service-runtime\mactype-service-setup.exe',
        'service-runtime\payload\manifest.json',
        'service-runtime\payload\files\mactype-service.exe',
        'service-runtime\payload\files\mactype-injector32.exe',
        'service-runtime\payload\files\mactype-injector64.exe',
        'service-runtime\payload\files\MacType.dll',
        'service-runtime\payload\files\MacType64.dll'
    )
    foreach ($relative in $expected) {
        if (-not (Test-Path -LiteralPath (Join-Path $ApplicationRoot $relative) -PathType Leaf)) {
            throw "Installer omitted required file: $relative"
        }
    }
}

function Assert-CommonDesktopShortcut {
    param(
        [Parameter(Mandatory)] [string] $CommonDesktopShortcut,
        [Parameter(Mandatory)] [string] $ApplicationRoot
    )

    if (-not (Test-Path -LiteralPath $CommonDesktopShortcut -PathType Leaf)) {
        throw 'Admin installer did not create the requested common desktop shortcut.'
    }
    $shell = New-Object -ComObject WScript.Shell
    try {
        $shortcut = $shell.CreateShortcut($CommonDesktopShortcut)
        $expected = [IO.Path]::GetFullPath((Join-Path $ApplicationRoot 'MacType Control Center.exe'))
        $actual = [IO.Path]::GetFullPath($shortcut.TargetPath)
        if (-not $expected.Equals($actual, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Common desktop shortcut targets '$actual' instead of '$expected'."
        }
    }
    finally {
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
    }
}

function Assert-ReadyOpenService {
    param(
        [Parameter(Mandatory)] [hashtable] $PayloadManifest,
        [Parameter(Mandatory)] [string] $OpenServiceName,
        [Parameter(Mandatory)] [string] $ServiceRoot,
        [Parameter(Mandatory)] [string] $ProfileRoot,
        [Parameter(Mandatory)] [string] $DistributionDefaultProfilePath
    )

    $service = Get-FixedService -Name $OpenServiceName
    if (-not $service) { throw 'Installer did not register the fixed open service.' }
    $current = Get-Content -LiteralPath (Join-Path $ServiceRoot 'current.json') -Raw | ConvertFrom-Json
    if ($current.schema -ne 1 -or $current.version -cne $PayloadManifest.version) {
        throw 'Active runtime pointer does not select the bundled runtime version.'
    }
    $generationRoot = Join-Path $ServiceRoot ("bin\" + $current.version)
    $expectedImage = [IO.Path]::GetFullPath((Join-Path $generationRoot 'mactype-service.exe'))
    $actualImage = Get-ServiceExecutablePath -ImagePath $service.PathName
    if (-not $expectedImage.Equals([IO.Path]::GetFullPath($actualImage), [StringComparison]::OrdinalIgnoreCase)) {
        throw "Open service image '$actualImage' is not the active protected runtime '$expectedImage'."
    }
    if ($service.StartMode -ne 'Auto' -or $service.StartName -ne 'LocalSystem' -or $service.State -ne 'Running') {
        throw "Open service is not Auto/LocalSystem/Running: $($service.StartMode)/$($service.StartName)/$($service.State)"
    }

    $activePointerPath = Join-Path $ProfileRoot 'active.json'
    $activeBytes = [IO.File]::ReadAllBytes($activePointerPath)
    $active = [Text.Encoding]::UTF8.GetString($activeBytes) | ConvertFrom-Json
    if ($active.schema -ne 1 -or $active.generation -notmatch '^sha256:[0-9a-f]{64}$') {
        throw 'Protected active profile pointer is invalid.'
    }
    $profileGeneration = Join-Path $ProfileRoot ("generations\" + $active.generation.Substring(7))
    $profilePath = Join-Path $profileGeneration 'profile.ini'
    if (-not (Test-Path -LiteralPath $profilePath -PathType Leaf)) {
        throw 'Protected active profile generation is missing.'
    }
    $expectedDefaultHash = (Get-FileHash -LiteralPath $DistributionDefaultProfilePath -Algorithm SHA256).Hash.ToLowerInvariant()
    $activeProfileHash = (Get-FileHash -LiteralPath $profilePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ("sha256:$activeProfileHash" -cne $active.generation -or $activeProfileHash -cne $expectedDefaultHash) {
        throw 'Fresh bootstrap did not publish the exact bundled default profile.'
    }
    if (-not [Linq.Enumerable]::SequenceEqual(
        [IO.File]::ReadAllBytes((Join-Path $generationRoot 'MacType.ini')),
        [IO.File]::ReadAllBytes($profilePath)
    )) {
        throw 'Runtime-adjacent profile does not exactly match the protected active profile.'
    }

    $receipt = Get-Content -LiteralPath (Join-Path $ServiceRoot ("runtime-receipts\" + $current.version + '.json')) -Raw | ConvertFrom-Json -AsHashtable
    if ($receipt.schema -ne 1 -or $receipt.version -cne $current.version) {
        throw 'Installed runtime receipt is invalid.'
    }
    foreach ($entry in $receipt.files.GetEnumerator()) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $generationRoot $entry.Key) -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($entry.Value -cne "sha256:$hash") { throw "Installed runtime differs from receipt: $($entry.Key)" }
    }

    $health = Get-Content -LiteralPath (Join-Path $ServiceRoot 'health.json') -Raw | ConvertFrom-Json
    if ($health.protocolVersion -ne 1 -or $health.serviceVersion -cne $current.version -or $health.health -cne 'ready' -or $health.activeProfileDigest -cne $active.generation -or $null -ne $health.lastError) {
        throw 'Installer returned before strict Ready health for the exact active profile.'
    }
    foreach ($component in @('profile', 'observer', 'injector32', 'injector64')) {
        if ($health.readiness.$component -cne 'ready') {
            throw "Required Ready component was not ready: $component"
        }
    }
    [pscustomobject]@{
        ActivePointerBytes = [Convert]::ToBase64String($activeBytes)
        ActiveGeneration = $active.generation
        ProfileGenerationRoot = $profileGeneration
        ProfileGenerationSnapshot = Get-TreeSnapshot -Path $profileGeneration
        RuntimeVersion = $current.version
    }
}

function Assert-BaselineRestoredAfterFailedUpgrade {
    param(
        [Parameter(Mandatory)] [pscustomobject] $Baseline,
        [Parameter(Mandatory)] [string] $BaselineApplicationSnapshot,
        [Parameter(Mandatory)] [string] $BaselineServiceSnapshot,
        [Parameter(Mandatory)] [string] $BaselineRuntimeSnapshot,
        [Parameter(Mandatory)] [string] $ApplicationRoot,
        [Parameter(Mandatory)] [string] $OpenServiceName,
        [Parameter(Mandatory)] [string] $ServiceRoot,
        [Parameter(Mandatory)] [string] $ProfileRoot
    )

    Assert-RequiredApplicationFiles -ApplicationRoot $ApplicationRoot
    $applicationSnapshot = Get-TreeSnapshot -Path $ApplicationRoot -ExcludedRoot $ServiceRoot
    if ($applicationSnapshot -cne $BaselineApplicationSnapshot) {
        $difference = Get-TreeSnapshotDifference `
            -ExpectedSnapshot $BaselineApplicationSnapshot `
            -ActualSnapshot $applicationSnapshot
        throw "Failed upgrade did not preserve the exact existing application file tree.`n$difference"
    }
    if ((Get-ServiceSnapshot -Name $OpenServiceName) -cne $BaselineServiceSnapshot) {
        throw 'Failed upgrade did not restore the exact baseline service configuration and state.'
    }
    $current = Get-Content -LiteralPath (Join-Path $ServiceRoot 'current.json') -Raw | ConvertFrom-Json
    if ($current.schema -ne 1 -or $current.version -cne $Baseline.RuntimeVersion) {
        throw 'Failed upgrade did not restore the baseline active runtime pointer.'
    }
    $baselineRuntimeRoot = Join-Path $ServiceRoot ("bin\" + $Baseline.RuntimeVersion)
    if ((Get-TreeSnapshot -Path $baselineRuntimeRoot) -cne $BaselineRuntimeSnapshot) {
        throw 'Failed upgrade changed the immutable baseline runtime generation.'
    }
    if ([Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $ProfileRoot 'active.json'))) -cne $Baseline.ActivePointerBytes -or
        (Get-TreeSnapshot -Path $Baseline.ProfileGenerationRoot) -cne $Baseline.ProfileGenerationSnapshot) {
        throw 'Failed upgrade did not preserve the exact protected profile generation.'
    }
    $health = Get-Content -LiteralPath (Join-Path $ServiceRoot 'health.json') -Raw | ConvertFrom-Json
    if ($health.protocolVersion -ne 1 -or $health.serviceVersion -cne $Baseline.RuntimeVersion -or
        $health.health -cne 'ready' -or $health.activeProfileDigest -cne $Baseline.ActiveGeneration -or
        $null -ne $health.lastError) {
        throw 'Failed upgrade did not restore strict Ready health for the baseline runtime/profile.'
    }
    foreach ($component in @('profile', 'observer', 'injector32', 'injector64')) {
        if ($health.readiness.$component -cne 'ready') {
            throw "Failed upgrade left baseline readiness incomplete: $component"
        }
    }
}

function New-ForeignService {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $DisplayName
    )

    $image = '"' + (Join-Path $env:SystemRoot 'System32\cmd.exe') + '" /c exit 0'
    & sc.exe create $Name 'binPath=' $image 'start=' 'demand' 'DisplayName=' $DisplayName | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Could not create test service $Name." }
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Get-FixedService -Name $Name) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (-not (Get-FixedService -Name $Name)) { throw "Test service $Name did not become observable." }
}

function Remove-TestService {
    param([Parameter(Mandatory)] [string] $Name)

    if (-not (Get-FixedService -Name $Name)) { return }
    & sc.exe stop $Name | Out-Null
    & sc.exe delete $Name | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while ((Get-FixedService -Name $Name) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (Get-FixedService -Name $Name) { throw "Test service $Name could not be removed." }
}
