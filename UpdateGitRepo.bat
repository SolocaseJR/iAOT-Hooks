@echo off
setlocal

cd /d "%~dp0"

echo.
git status --short
echo.

set /p COMMIT_MSG=Enter commit message: 

if "%COMMIT_MSG%"=="" (
    echo Commit message cannot be empty.
    pause
    exit /b 1
)

git add -A

git diff --cached --quiet
if %errorlevel%==0 (
    echo No changes to commit.
    pause
    exit /b 0
)

git commit -m "%COMMIT_MSG%"
if errorlevel 1 (
    echo Commit failed.
    pause
    exit /b 1
)

git push origin main
if errorlevel 1 (
    echo Push failed.
    pause
    exit /b 1
)

echo.
echo Done.
pause