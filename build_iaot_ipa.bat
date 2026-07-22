@echo off
setlocal
cd /d "%~dp0"
if "%~1"=="" (
  echo Usage: drag a decrypted IPA onto build_iaot_ipa.bat
  echo        build_iaot_ipa.bat "C:\path with spaces\OwnedApp.ipa" [hooks.yaml]
  exit /b 2
)
set "IAOT_INPUT=%~f1"
set "IAOT_MANIFEST=%~f2"
if not defined IAOT_MANIFEST set "IAOT_MANIFEST=%~dp0templates\theos-dylib\hooks.yaml"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-iaot.ps1" -InputIpa "%IAOT_INPUT%" -Manifest "%IAOT_MANIFEST%"
set "IAOT_EXIT=%ERRORLEVEL%"
if not "%IAOT_EXIT%"=="0" (
  echo.
  echo [IAOT] Build failed. The stage-specific error is shown above.
  pause
)
exit /b %IAOT_EXIT%
