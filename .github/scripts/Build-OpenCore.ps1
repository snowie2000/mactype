[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$dependencyRoot = Join-Path $root 'build\core-dependencies'
$libraryRoot = Join-Path $root 'deps\lib'
$artifactRoot = [System.IO.Path]::GetFullPath((Join-Path $root 'artifacts\open-core'))
$artifactBoundary = [System.IO.Path]::GetFullPath((Join-Path $root 'artifacts'))
if (-not $artifactRoot.StartsWith($artifactBoundary + [System.IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Open core output must stay inside artifacts/: $artifactRoot"
}
$freetypeRoot = Join-Path $dependencyRoot 'freetype'
$iniParserRoot = Join-Path $dependencyRoot 'IniParser'
$detoursRoot = Join-Path $dependencyRoot 'Detours'
$wow64extRoot = Join-Path $dependencyRoot 'wow64ext'

$msbuildCommand = Get-Command msbuild -ErrorAction SilentlyContinue
$msbuild = if ($msbuildCommand) {
    $msbuildCommand.Source
} else {
    @(
        'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin\MSBuild.exe',
        'C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\MSBuild\Current\Bin\MSBuild.exe'
    ) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $msbuild) { throw 'MSBuild was not found. Install Visual Studio Build Tools with the C++ workload.' }

New-Item -ItemType Directory -Force -Path $dependencyRoot, $libraryRoot | Out-Null

function Assert-ExactRelativeFileSet(
    [string] $Path,
    [string[]] $Expected,
    [string] $FailureMessage
) {
    $actual = @(Get-ChildItem -LiteralPath $Path -Recurse -File | ForEach-Object {
        [System.IO.Path]::GetRelativePath($Path, $_.FullName)
    } | Sort-Object)
    $difference = @(Compare-Object @($Expected | Sort-Object) $actual)
    if ($difference.Count -ne 0) {
        $details = $difference | ForEach-Object { "$($_.SideIndicator) $($_.InputObject)" }
        throw "$FailureMessage`: $($details -join ', ')"
    }
}

$ExpectedOpenCoreFiles = @(
    'MacType.dll',
    'MacType64.dll',
    'MacType.Core.dll',
    'MacType64.Core.dll',
    'MacLoader.exe',
    'MacLoader64.exe',
    'mactype-injector32.exe',
    'mactype-injector64.exe'
)

function Sync-Dependency([string] $Url, [string] $Path, [string] $Commit) {
    if (-not (Test-Path -LiteralPath $Path)) {
        git clone --filter=blob:none --no-checkout $Url $Path
    }
    git -C $Path fetch --depth 1 origin $Commit
    git -C $Path checkout --detach $Commit
}

Sync-Dependency -Url 'https://github.com/snowie2000/freetype.git' -Path $freetypeRoot -Commit 'ef771574d04721baf45a1b66bfb4692193603088'
Sync-Dependency -Url 'https://github.com/snowie2000/IniParser.git' -Path $iniParserRoot -Commit 'a457397ffa9d20e8df43e2c143c60da78c16c059'
Sync-Dependency -Url 'https://github.com/microsoft/Detours.git' -Path $detoursRoot -Commit 'd644ce94e8c7f7f5a31591577c78134ea3ac1fae'
Sync-Dependency -Url 'https://github.com/snowie2000/rewolf-wow64ext.git' -Path $wow64extRoot -Commit '667359c7967249dd9d28d8f8cef65b60e7e2d963'

function Build-Freetype([string] $Platform, [string] $BuildName, [string] $OutputName) {
    $buildPath = Join-Path $dependencyRoot $BuildName
    cmake -S $freetypeRoot -B $buildPath -A $Platform -DBUILD_SHARED_LIBS=OFF -DCMAKE_POLICY_DEFAULT_CMP0091=NEW -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DFT_DISABLE_BROTLI=ON -DFT_DISABLE_BZIP2=ON -DFT_DISABLE_HARFBUZZ=ON -DFT_DISABLE_PNG=ON -DFT_DISABLE_ZLIB=ON
    cmake --build $buildPath --config Release --parallel
    $library = Get-ChildItem -LiteralPath $buildPath -Recurse -File -Filter 'freetype.lib' | Select-Object -First 1
    if (-not $library) { throw "FreeType library was not produced for $Platform" }
    Copy-Item -LiteralPath $library.FullName -Destination (Join-Path $libraryRoot $OutputName) -Force
}

Build-Freetype -Platform Win32 -BuildName 'freetype-x86' -OutputName 'freetype.lib'
Build-Freetype -Platform x64 -BuildName 'freetype-x64' -OutputName 'freetype64.lib'

$iniSolution = Join-Path $iniParserRoot 'IniParser.sln'
& $msbuild $iniSolution /m /t:Rebuild /p:Configuration=Release /p:Platform=x86 /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0
& $msbuild $iniSolution /m /t:Rebuild /p:Configuration=Release /p:Platform=x64 /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0

$ini32 = Get-ChildItem -LiteralPath $iniParserRoot -Recurse -File -Filter 'IniParser.lib' | Where-Object { $_.FullName -notmatch '[\\/]x64[\\/]' } | Select-Object -First 1
$ini64 = Get-ChildItem -LiteralPath $iniParserRoot -Recurse -File -Filter 'IniParser64.lib' | Select-Object -First 1
if (-not $ini32 -or -not $ini64) { throw 'IniParser x86/x64 libraries were not produced.' }
Copy-Item -LiteralPath $ini32.FullName -Destination (Join-Path $libraryRoot 'iniparser.lib') -Force
Copy-Item -LiteralPath $ini64.FullName -Destination (Join-Path $libraryRoot 'iniparser64.lib') -Force

$detoursSolution = Join-Path $detoursRoot 'vc\Detours.sln'
& $msbuild $detoursSolution /m /t:Rebuild /p:Configuration=ReleaseMD /p:Platform=x86 /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0
$detours32 = Get-ChildItem -LiteralPath $detoursRoot -Recurse -File -Filter 'detours.lib' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
if (-not $detours32) { throw 'Detours x86 library was not produced.' }
Copy-Item -LiteralPath $detours32.FullName -Destination (Join-Path $libraryRoot 'detours.lib') -Force
Copy-Item -LiteralPath $detours32.FullName -Destination (Join-Path $root 'detours.lib') -Force

& $msbuild $detoursSolution /m /t:Rebuild /p:Configuration=ReleaseMD /p:Platform=x64 /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0
$detours64 = Get-ChildItem -LiteralPath $detoursRoot -Recurse -File -Filter 'detours.lib' | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
if (-not $detours64) { throw 'Detours x64 library was not produced.' }
Copy-Item -LiteralPath $detours64.FullName -Destination (Join-Path $libraryRoot 'detours64.lib') -Force
Copy-Item -LiteralPath $detours64.FullName -Destination (Join-Path $root 'detours64.lib') -Force

$wow64Solution = Join-Path $wow64extRoot 'src\wow64ext.sln'
& $msbuild $wow64Solution /m /t:Rebuild /p:Configuration=Release /p:Platform=Win32 /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0
$wow64Library = Get-ChildItem -LiteralPath $wow64extRoot -Recurse -File -Filter 'wow64ext.lib' | Sort-Object Length -Descending | Select-Object -First 1
if (-not $wow64Library) { throw 'wow64ext x86 library was not produced.' }
Copy-Item -LiteralPath $wow64Library.FullName -Destination (Join-Path $libraryRoot 'wow64ext.lib') -Force

$env:FREETYPE_PATH = $freetypeRoot
$env:INI_PARSER_PATH = $iniParserRoot
$solution = Join-Path $root 'gdipp.sln'
& $msbuild $solution /m /t:Rebuild '/p:Configuration=Rel+Detours' /p:Platform=Win32 /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0
& $msbuild $solution /m /t:Rebuild '/p:Configuration=Rel+Detours' /p:Platform=x64 /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0

$injectorSource = Join-Path $root 'service-injector'
if (-not (Test-Path -LiteralPath (Join-Path $injectorSource 'CMakeLists.txt') -PathType Leaf)) {
    throw 'Open C++ service injector source is missing: service-injector/CMakeLists.txt'
}
$injector32Build = Join-Path $root 'build\service-injector-x86'
$injector64Build = Join-Path $root 'build\service-injector-x64'
cmake -S $injectorSource -B $injector32Build -A Win32 -DBUILD_TESTING=ON
cmake --build $injector32Build --config Release --parallel
ctest --test-dir $injector32Build -C Release --output-on-failure
cmake -S $injectorSource -B $injector64Build -A x64 -DBUILD_TESTING=ON
cmake --build $injector64Build --config Release --parallel
ctest --test-dir $injector64Build -C Release --output-on-failure

$artifacts = @(
    @{ Source = (Join-Path $root 'Rel+Detours\MacType.Core.dll'); Destination = 'MacType.Core.dll' },
    @{ Source = (Join-Path $root 'Rel+Detours\MacType.Core.dll'); Destination = 'MacType.dll' },
    @{ Source = (Join-Path $root 'Release\macloader.exe'); Destination = 'MacLoader.exe' },
    @{ Source = (Join-Path $root 'x64\Rel+Detours\MacType64.Core.dll'); Destination = 'MacType64.Core.dll' },
    @{ Source = (Join-Path $root 'x64\Rel+Detours\MacType64.Core.dll'); Destination = 'MacType64.dll' },
    @{ Source = (Join-Path $root 'x64\Release\macloader64.exe'); Destination = 'MacLoader64.exe' },
    @{ Source = (Join-Path $injector32Build 'Release\mactype-injector32.exe'); Destination = 'mactype-injector32.exe' },
    @{ Source = (Join-Path $injector64Build 'Release\mactype-injector64.exe'); Destination = 'mactype-injector64.exe' }
)
foreach ($artifact in $artifacts) {
    if (-not (Test-Path -LiteralPath $artifact.Source)) { throw "Expected core artifact missing: $($artifact.Source)" }
}
if (Test-Path -LiteralPath $artifactRoot) {
    Remove-Item -LiteralPath $artifactRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $artifactRoot | Out-Null
foreach ($artifact in $artifacts) {
    Copy-Item -LiteralPath $artifact.Source -Destination (Join-Path $artifactRoot $artifact.Destination) -Force
}

Assert-ExactRelativeFileSet `
    -Path $artifactRoot `
    -Expected $ExpectedOpenCoreFiles `
    -FailureMessage 'Unexpected open-core artifact set'

Write-Host "Open core source build produced $($artifacts.Count) verified package artifacts."
