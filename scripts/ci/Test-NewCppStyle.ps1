[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$sourceRoots = @('preview-helper', 'service-injector', 'tools\service-probe')
$files = foreach ($sourceRoot in $sourceRoots) {
    if (Test-Path -LiteralPath $sourceRoot -PathType Container) {
        Get-ChildItem -LiteralPath $sourceRoot -Recurse -File -Include '*.cpp', '*.h', '*.hpp' |
            Where-Object { $_.FullName -notmatch '[\\/](?:build|target|artifacts|out|CMakeFiles)[\\/]' }
    }
}
$files = @($files | Sort-Object FullName -Unique)
$failed = $false
foreach ($file in $files) {
    $text = Get-Content -LiteralPath $file.FullName -Raw
    if ($text -match "`t") {
        Write-Error "$($file.FullName): tabs are not allowed in new C++ code" -ErrorAction Continue
        $failed = $true
    }
    if ($text -match '(?m)[ \t]+$') {
        Write-Error "$($file.FullName): trailing whitespace" -ErrorAction Continue
        $failed = $true
    }
}
if ($failed) { exit 1 }
Write-Host "New public C++ style gate passed for $($files.Count) files."
