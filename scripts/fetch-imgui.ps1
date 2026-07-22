$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Destination = Join-Path $Root 'vendor\imgui'
$Ref = if ($env:IMGUI_REF) { $env:IMGUI_REF } else { 'v1.92.8' }
if (Test-Path (Join-Path $Destination '.git')) {
    git -C $Destination fetch --tags --depth 1 origin $Ref
    if ($LASTEXITCODE -ne 0) { throw "git fetch failed with exit code $LASTEXITCODE" }
    git -C $Destination checkout --detach FETCH_HEAD
    if ($LASTEXITCODE -ne 0) { throw "git checkout failed with exit code $LASTEXITCODE" }
} elseif (Test-Path $Destination) {
    throw "$Destination exists but is not a Git checkout"
} else {
    git clone --depth 1 --branch $Ref https://github.com/ocornut/imgui.git $Destination
    if ($LASTEXITCODE -ne 0) { throw "git clone failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path (Join-Path $Destination 'imgui.cpp'))) {
    throw "Dear ImGui checkout is incomplete: imgui.cpp is missing"
}
if (-not (Test-Path (Join-Path $Destination 'LICENSE.txt'))) {
    throw "Dear ImGui checkout is incomplete: LICENSE.txt is missing"
}
Write-Host "Dear ImGui $Ref is ready at $Destination"
