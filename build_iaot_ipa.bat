@echo off
setlocal
cd /d "%~dp0"
if "%~1"=="" (
  echo Usage: build_iaot_ipa.bat hooks.yaml [decrypted-input.ipa]
  exit /b 2
)
set "IAOT_MANIFEST=%~f1"
set "IAOT_INPUT=%~f2"
if defined IAOT_INPUT (
  powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-iaot.ps1" -Manifest "%IAOT_MANIFEST%" -InputIpa "%IAOT_INPUT%"
) else (
  powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-iaot.ps1" -Manifest "%IAOT_MANIFEST%"
)
set "IAOT_EXIT=%ERRORLEVEL%"
if not "%IAOT_EXIT%"=="0" (
  echo.
  echo [IAOT] Build failed. The stage-specific error is shown above.
  pause
)
exit /b %IAOT_EXIT%
