[CmdletBinding()]
param(
    [string]$InputIpa,
    [Parameter(Mandatory=$true)][string]$Manifest,
    [string]$OutputIpa,
    [string]$Distro = $env:IAOT_WSL_DISTRO,
    [string]$CodeVirtualizerRoot,
    [string]$CodeVirtualizerProject
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$buildRoot = $null
$temporaryRoot = $null

# Keep Cargo's target directory as the Rust incremental compiler cache. It is
# what lets an unchanged patcher start immediately on subsequent builds.
function Clear-TransientBuildOutput {
    if ($buildRoot -and (Test-Path -LiteralPath $buildRoot)) {
        Remove-Item -LiteralPath $buildRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    if ($temporaryRoot -and (Test-Path -LiteralPath $temporaryRoot)) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Get-Sha256([string]$Path) {
    $hash = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead($Path)
    try { return [Convert]::ToBase64String($hash.ComputeHash($stream)) }
    finally { $stream.Dispose(); $hash.Dispose() }
}

trap {
    Clear-TransientBuildOutput
    break
}

Clear-TransientBuildOutput
$manifestPath = [IO.Path]::GetFullPath($Manifest)
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "[config] manifest not found: $manifestPath" }
$project = Split-Path -Parent $manifestPath
$cvEnabled = -not [string]::IsNullOrWhiteSpace($CodeVirtualizerRoot)
if ($cvEnabled) {
    if ([string]::IsNullOrWhiteSpace($CodeVirtualizerProject)) {
        throw '[cv] -CodeVirtualizerProject is required with -CodeVirtualizerRoot'
    }
    $CodeVirtualizerRoot = [IO.Path]::GetFullPath($CodeVirtualizerRoot)
    $CodeVirtualizerProject = [IO.Path]::GetFullPath($CodeVirtualizerProject)
    $cvExe = Join-Path $CodeVirtualizerRoot 'VirtualizerArm64.exe'
    if (-not (Test-Path -LiteralPath $cvExe -PathType Leaf)) { throw "[cv] ARM64 CLI not found: $cvExe" }
    if (-not (Test-Path -LiteralPath $CodeVirtualizerProject -PathType Leaf)) { throw "[cv] project not found: $CodeVirtualizerProject" }
}

foreach ($commandName in @('cargo','wsl.exe')) {
    if (-not (Get-Command $commandName -ErrorAction SilentlyContinue)) { throw "[dependency] $commandName was not found on PATH" }
}
Write-Host '[1/6] Building the IAOT Windows CLI'
& cargo build --manifest-path (Join-Path $repo 'patcher\Cargo.toml') --release --locked
if ($LASTEXITCODE -ne 0) { throw '[cli] Cargo build failed' }
$iaot = Join-Path $repo 'patcher\target\release\iaot.exe'
if (-not (Test-Path -LiteralPath $iaot)) { throw "[cli] Cargo did not produce $iaot" }

$resolvedJson = @(& $iaot resolve-config --manifest $manifestPath)
if ($LASTEXITCODE -ne 0) { throw '[config] could not resolve the split project configuration' }
$resolved = ($resolvedJson -join "`n") | ConvertFrom-Json
if (-not $InputIpa) { $InputIpa = [string]$resolved.input_ipa }
if (-not $OutputIpa) { $OutputIpa = [string]$resolved.output_ipa }
if ([string]::IsNullOrWhiteSpace($InputIpa)) { throw '[config] paths.input_ipa is missing and -InputIpa was not supplied' }
if ([string]::IsNullOrWhiteSpace($OutputIpa)) { throw '[config] paths.output_ipa is missing and -OutputIpa was not supplied' }
$inputPath = [IO.Path]::GetFullPath($InputIpa)
$outputPath = [IO.Path]::GetFullPath($OutputIpa)
if (-not (Test-Path -LiteralPath $inputPath -PathType Leaf)) { throw "[input] decrypted IPA not found: $inputPath" }
if ([IO.Path]::GetExtension($inputPath) -ne '.ipa') { throw "[input] expected an .ipa file: $inputPath" }
if ($outputPath -eq $inputPath) { throw '[output] input IPA will never be overwritten; select another output path' }
$configuredBuildDir = [string]$resolved.build_dir
if ([string]::IsNullOrWhiteSpace($configuredBuildDir)) {
    $configuredBuildDir = Join-Path $project 'build'
}
$buildRoot = Join-Path ([IO.Path]::GetFullPath($configuredBuildDir)) 'windows'
$configuredTempDir = [string]$resolved.temp_dir
if ([string]::IsNullOrWhiteSpace($configuredTempDir)) {
    $configuredTempDir = Join-Path $project 'build\temp'
}
$temporaryRoot = [IO.Path]::GetFullPath($configuredTempDir)
Clear-TransientBuildOutput
New-Item -ItemType Directory -Force -Path $buildRoot | Out-Null

if (-not $Distro) {
    $Distro = (& wsl.exe --list --quiet | Where-Object { $_.Trim() } | Select-Object -First 1).Trim()
}
if (-not $Distro) { throw '[wsl] no Linux distribution is installed; run wsl --install -d Ubuntu' }
Write-Host "[2/6] Checking WSL distribution: $Distro"
& wsl.exe -d $Distro -- bash -lc 'true'
if ($LASTEXITCODE -ne 0) { throw "[wsl] $Distro could not start; run 'wsl --shutdown', check available RAM, and retry" }

function Convert-ToWsl([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    $variableName = 'IAOT_WINDOWS_PATH'
    $oldValue = [Environment]::GetEnvironmentVariable($variableName, 'Process')
    $oldWslEnv = $env:WSLENV

    try {
        # WSLENV /p asks wsl.exe itself to translate the Windows path. This
        # avoids shell parsing of spaces, backslashes, ampersands, parentheses,
        # and quotes in project paths.
        Set-Item -LiteralPath "Env:$variableName" -Value $fullPath

        $entries = @()
        if ($oldWslEnv) {
            $entries = @($oldWslEnv -split ':' | Where-Object {
                $_ -and $_ -notmatch '^IAOT_WINDOWS_PATH(?:/.*)?$'
            })
        }
        $env:WSLENV = (@($entries) + "$variableName/p") -join ':'

        $mappedOutput = @(
            & wsl.exe -d $Distro -- bash -lc 'printf "%s" "$IAOT_WINDOWS_PATH"'
        )
        $exitCode = $LASTEXITCODE
        $mapped = ($mappedOutput -join "`n").Trim()
        if ($exitCode -ne 0 -or [string]::IsNullOrWhiteSpace($mapped)) {
            throw "[wsl-path] could not map: $fullPath"
        }
        return $mapped
    }
    finally {
        if ($null -eq $oldValue) {
            Remove-Item -LiteralPath "Env:$variableName" -ErrorAction SilentlyContinue
        } else {
            Set-Item -LiteralPath "Env:$variableName" -Value $oldValue
        }

        if ($null -eq $oldWslEnv) {
            Remove-Item -LiteralPath 'Env:WSLENV' -ErrorAction SilentlyContinue
        } else {
            $env:WSLENV = $oldWslEnv
        }
    }
}
$repoWsl = Convert-ToWsl $repo
$projectWsl = Convert-ToWsl $project
$runtimeWindows = Join-Path $buildRoot 'runtime\IAOTRuntime.dylib'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $runtimeWindows) | Out-Null
$runtimeWsl = Convert-ToWsl $runtimeWindows
$scriptWsl = Convert-ToWsl (Join-Path $repo 'scripts\build-wsl.sh')
$reports = [string]$resolved.generated_dir
if ([string]::IsNullOrWhiteSpace($reports)) {
    $reports = Join-Path $buildRoot 'reports'
}

Write-Host '[3/6] Analyzing the IPA and generating target-aware sources'
& $iaot prepare --manifest $manifestPath --in $inputPath --reports $reports
if ($LASTEXITCODE -ne 0) { throw '[analysis] IPA analysis or source generation failed' }

$mode = [string]$resolved.build_mode
if ($mode -notin @('development', 'release')) { throw "[config] resolved build mode is invalid: $mode" }
if ($cvEnabled -and $mode -ne 'release') { throw '[cv] Code Virtualizer requires build.mode: release' }
Write-Host '[4/6] Building the precompiled runtime in WSL/Theos'
if ($cvEnabled) {
    $cvRootWsl = Convert-ToWsl $CodeVirtualizerRoot
    & wsl.exe -d $Distro -- bash $scriptWsl $repoWsl $projectWsl $runtimeWsl $mode $cvRootWsl
} else {
    & wsl.exe -d $Distro -- bash $scriptWsl $repoWsl $projectWsl $runtimeWsl $mode
}
if ($LASTEXITCODE -ne 0) { throw '[runtime] WSL/Theos build failed; see the dependency or compiler error above' }
if (-not (Test-Path -LiteralPath $runtimeWindows -PathType Leaf)) { throw '[runtime] WSL did not copy the dylib back to Windows' }

if ($cvEnabled) {
    $cvVerify = Join-Path $repo 'scripts\verify-cv-input.py'
    & python $cvVerify --binary $runtimeWindows
    if ($LASTEXITCODE -ne 0) { throw '[cv] compiled dylib lacks ARM64 stealth markers' }
    $protectedRuntime = Join-Path $buildRoot 'runtime\IAOTRuntime-protected.dylib'
    Write-Host '[cv] Protecting the freshly built dylib with Code Virtualizer'
    # VirtualizerArm64.exe is a Windows GUI-subsystem executable. PowerShell's
    # call operator can return before it finishes; wait for the actual process.
    $cvArguments = '/protect "{0}" /inputfile "{1}" /outputfile "{2}" /shareconsole' -f `
        $CodeVirtualizerProject, $runtimeWindows, $protectedRuntime
    $cvProcess = Start-Process -FilePath $cvExe -ArgumentList $cvArguments -PassThru -Wait -WindowStyle Hidden
    if ($cvProcess.ExitCode -ne 0) { throw "[cv] Code Virtualizer exited $($cvProcess.ExitCode); unprotected dylib will not be packaged" }
    if (-not (Test-Path -LiteralPath $protectedRuntime -PathType Leaf)) { throw '[cv] protected dylib missing' }
    if ((Get-Sha256 $runtimeWindows) -eq (Get-Sha256 $protectedRuntime)) {
        throw '[cv] protected dylib is byte-identical to the input'
    }
    Write-Host '[cv] Stripping and checking protected Mach-O symbols'
    $finalizeWsl = Convert-ToWsl (Join-Path $repo 'scripts\finalize-cv.sh')
    $protectedWsl = Convert-ToWsl $protectedRuntime
    $verifyWsl = Convert-ToWsl $cvVerify
    & wsl.exe -d $Distro -- bash $finalizeWsl $protectedWsl $verifyWsl
    if ($LASTEXITCODE -ne 0) { throw '[cv] protected dylib symbol finalization failed' }
    & python $cvVerify --final-symbols $protectedRuntime
    if ($LASTEXITCODE -ne 0) { throw '[cv] protected dylib still exposes project symbols' }
    $runtimeWindows = $protectedRuntime
}

Write-Host '[5/6] Patching, injecting, repackaging, and validating'
& $iaot pipeline --manifest $manifestPath --in $inputPath --out $outputPath --dylib $runtimeWindows --reports $reports
if ($LASTEXITCODE -ne 0) { throw '[pipeline] final IPA pipeline failed' }
Clear-TransientBuildOutput
Write-Host '[6/6] Complete'
Write-Host "Unsigned IPA: $outputPath"
Write-Host "Validation:  $(Join-Path $reports 'final-validation.yaml')"
Write-Warning 'The IPA must still be signed/re-signed, including every nested framework and dylib.'
