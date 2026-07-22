[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$InputIpa,
    [Parameter(Mandatory=$true)][string]$Manifest,
    [string]$OutputIpa,
    [string]$Distro = $env:IAOT_WSL_DISTRO
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$inputPath = [IO.Path]::GetFullPath($InputIpa)
$manifestPath = [IO.Path]::GetFullPath($Manifest)
if (-not (Test-Path -LiteralPath $inputPath -PathType Leaf)) { throw "[input] decrypted IPA not found: $inputPath" }
if ([IO.Path]::GetExtension($inputPath) -ne '.ipa') { throw "[input] expected an .ipa file: $inputPath" }
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "[config] manifest not found: $manifestPath" }
$project = Split-Path -Parent $manifestPath
$buildRoot = Join-Path $repo 'build\windows'
New-Item -ItemType Directory -Force -Path $buildRoot | Out-Null
if (-not $OutputIpa) {
    $base = [IO.Path]::GetFileNameWithoutExtension($inputPath)
    $OutputIpa = Join-Path $buildRoot "$base-IAOT-unsigned.ipa"
}
$outputPath = [IO.Path]::GetFullPath($OutputIpa)
if ($outputPath -eq $inputPath) { throw '[output] input IPA will never be overwritten; select another output path' }

foreach ($commandName in @('cargo','wsl.exe')) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) { throw "[dependency] $commandName was not found on PATH" }
}
Write-Host '[1/6] Building the IAOT Windows CLI'
& cargo build --manifest-path (Join-Path $repo 'patcher\Cargo.toml') --release --locked
if ($LASTEXITCODE -ne 0) { throw '[cli] Cargo build failed' }
$iaot = Join-Path $repo 'patcher\target\release\iaot.exe'
if (-not (Test-Path -LiteralPath $iaot)) { throw "[cli] Cargo did not produce $iaot" }

if (-not $Distro) {
    $Distro = (& wsl.exe --list --quiet | Where-Object { $_.Trim() } | Select-Object -First 1).Trim()
}
if (-not $Distro) { throw '[wsl] no Linux distribution is installed; run wsl --install -d Ubuntu' }
Write-Host "[2/6] Checking WSL distribution: $Distro"
& wsl.exe -d $Distro -- bash -lc 'true'
if ($LASTEXITCODE -ne 0) { throw "[wsl] $Distro could not start; run 'wsl --shutdown', check available RAM, and retry" }

function Convert-ToWsl([string]$Path) {
    $mapped = (& wsl.exe -d $Distro -- wslpath -a $Path)
    if ($LASTEXITCODE -ne 0 -or -not $mapped) { throw "[wsl-path] could not map: $Path" }
    return $mapped.Trim()
}
$repoWsl = Convert-ToWsl $repo
$projectWsl = Convert-ToWsl $project
$runtimeWindows = Join-Path $buildRoot 'runtime\IAOTRuntime.dylib'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $runtimeWindows) | Out-Null
$runtimeWsl = Convert-ToWsl $runtimeWindows
$scriptWsl = Convert-ToWsl (Join-Path $repo 'scripts\build-wsl.sh')
$reports = Join-Path $buildRoot 'reports'

Write-Host '[3/6] Analyzing the IPA and generating target-aware sources'
& $iaot prepare --manifest $manifestPath --in $inputPath --reports $reports
if ($LASTEXITCODE -ne 0) { throw '[analysis] IPA analysis or source generation failed' }

$generatedSettings = Join-Path $project 'generated\IAOTGeneratedProject.mk'
$modeMatch = Select-String -LiteralPath $generatedSettings -Pattern '^IAOT_BUILD_MODE := (development|release)$'
if (-not $modeMatch) { throw "[config] generated build mode is missing from $generatedSettings" }
$mode = $modeMatch.Matches[0].Groups[1].Value
Write-Host '[4/6] Building the precompiled runtime in WSL/Theos'
& wsl.exe -d $Distro -- bash $scriptWsl $repoWsl $projectWsl $runtimeWsl $mode
if ($LASTEXITCODE -ne 0) { throw '[runtime] WSL/Theos build failed; see the dependency or compiler error above' }
if (-not (Test-Path -LiteralPath $runtimeWindows -PathType Leaf)) { throw '[runtime] WSL did not copy the dylib back to Windows' }

Write-Host '[5/6] Patching, injecting, repackaging, and validating'
& $iaot pipeline --manifest $manifestPath --in $inputPath --out $outputPath --dylib $runtimeWindows --reports $reports
if ($LASTEXITCODE -ne 0) { throw '[pipeline] final IPA pipeline failed' }
Write-Host '[6/6] Complete'
Write-Host "Unsigned IPA: $outputPath"
Write-Host "Validation:  $(Join-Path $reports 'final-validation.yaml')"
Write-Warning 'The IPA must still be signed/re-signed, including every nested framework and dylib.'
