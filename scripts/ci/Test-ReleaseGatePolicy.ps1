[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$workflowPath = Join-Path $root '.github\workflows\build.yml'
$workflow = Get-Content -LiteralPath $workflowPath -Raw

$requiredTokens = @(
    'release-frontend-quality:',
    'pnpm generate:settings',
    'pnpm test:i18n',
    'pnpm test:settings',
    'pnpm lint',
    'release-tauri-quality:',
    'cargo fmt --all -- --check',
    'cargo clippy --all-targets --all-features -- -D warnings',
    'cargo test --all-targets',
    'release-cpp-quality:',
    'ctest --test-dir build/preview-helper -C Release --output-on-failure',
    'release-injector-quality:',
    'release-static-quality:',
    'scripts/ci/Test-DistributionPolicy.ps1',
    'scripts/ci/Test-ReleaseGatePolicy.ps1',
    'scripts/ci/Test-InstallerRollbackPolicy.ps1',
    'release-gallery:',
    'pnpm test:gallery',
    'release-quality-gate:'
)
foreach ($token in $requiredTokens) {
    if (-not $workflow.Contains($token)) {
        throw "Release workflow omits required quality token: $token"
    }
}

$qualityNeeds = @(
    'release-frontend-quality',
    'release-tauri-quality',
    'release-cpp-quality',
    'release-injector-quality',
    'release-static-quality',
    'release-gallery'
)
$qualityGate = [regex]::Match(
    $workflow,
    '(?ms)^  release-quality-gate:\s*.*?(?=^  [a-zA-Z0-9_-]+:|\z)'
).Value
if (-not $qualityGate) { throw 'Release quality gate job is missing.' }
foreach ($job in $qualityNeeds) {
    if ($qualityGate -notmatch "(?m)^\s+needs:\s*\[[^\]]*\b$([regex]::Escape($job))\b") {
        throw "Release quality gate does not depend on $job."
    }
}

$releaseGalleryJob = [regex]::Match(
    $workflow,
    '(?ms)^  release-gallery:\s*.*?(?=^  [a-zA-Z0-9_-]+:|\z)'
).Value
$galleryUpload = [regex]::Match(
    $releaseGalleryJob,
    '(?ms)^\s+- uses: actions/upload-artifact@v7\s*.*?(?=^\s+- (?:uses:|name:)|\z)'
).Value
if (
    -not $galleryUpload -or
    $galleryUpload -notmatch '(?m)^\s+name:\s*release-frontend-window-gallery\s*$' -or
    $galleryUpload -notmatch '(?m)^\s+path:\s*artifacts/frontend-gallery\s*$'
) {
    throw 'Release gallery job no longer preserves the complete validated gallery artifact.'
}

$releaseJob = [regex]::Match(
    $workflow,
    '(?ms)^  release-main-snapshot:\s*.*?(?=^  [a-zA-Z0-9_-]+:|\z)'
).Value
if (-not $releaseJob) { throw 'Main snapshot release job is missing.' }
foreach ($job in @('windows-build', 'open-service-windows', 'release-quality-gate', 'release-gallery')) {
    if ($releaseJob -notmatch "(?m)^\s+needs:\s*\[[^\]]*\b$([regex]::Escape($job))\b") {
        throw "Main snapshot publication can run without $job."
    }
}
if ($releaseJob -notmatch "(?m)^\s+if:\s*success\(\) && github\.event_name == 'push' && github\.ref == 'refs/heads/main'") {
    throw 'Main snapshot publication is not restricted to successful main pushes.'
}

$downloadSteps = [regex]::Matches(
    $releaseJob,
    '(?ms)^      - uses: actions/download-artifact@v8\s*.*?(?=^      - (?:uses:|name:)|\z)'
)
$galleryDownload = @(
    $downloadSteps |
        ForEach-Object { $_.Value } |
        Where-Object { $_ -match '(?m)^\s+name:\s*release-frontend-window-gallery\s*$' }
) | Select-Object -First 1
if (-not $galleryDownload) {
    throw 'Main snapshot publication does not download the validated frontend gallery artifact.'
}
if ($galleryDownload -notmatch '(?m)^\s+path:\s*gallery-artifact\s*$') {
    throw 'Validated frontend gallery is not downloaded to the bounded selection input.'
}

$releaseAssets = @(
    'desktop-1280-overview-en.png',
    'desktop-1280-execution-ready-en.png',
    'desktop-1280-execution-migration-available-en.png',
    'desktop-1280-profiles-zh-CN.png',
    'mobile-390-overview-ar.png'
)

$selectionStep = [regex]::Match(
    $releaseJob,
    '(?ms)^\s+- name:\s*Select the bounded release gallery\s*.*?(?=^\s+- (?:uses:|name:)|\z)'
).Value
if (-not $selectionStep) { throw 'Bounded release gallery selection step is missing.' }
$selectedAssets = @(
    [regex]::Matches($selectionStep, "'(?<asset>[^'\r\n]+\.png)'") |
        ForEach-Object { $_.Groups['asset'].Value }
)

$publicationStep = [regex]::Match(
    $releaseJob,
    '(?ms)^\s+- name:\s*Publish automatic pre-release\s*.*?(?=^\s+- (?:uses:|name:)|\z)'
).Value
if (-not $publicationStep) { throw 'Automatic pre-release publication step is missing.' }
$publishedAssets = @(
    [regex]::Matches($publicationStep, '(?m)^\s+release/gallery/(?<asset>[^/\r\n]+\.png)\s*$') |
        ForEach-Object { $_.Groups['asset'].Value }
)

foreach ($assetSet in @(
    @{ Name = 'selected'; Values = $selectedAssets },
    @{ Name = 'published'; Values = $publishedAssets }
)) {
    $difference = @(Compare-Object -ReferenceObject $releaseAssets -DifferenceObject $assetSet.Values -CaseSensitive)
    if ($assetSet.Values.Count -ne $releaseAssets.Count -or $difference.Count -ne 0) {
        throw "Release gallery $($assetSet.Name) asset allowlist does not exactly match the required five files."
    }
}

Write-Host 'Release publication quality-gate policy passed.'
