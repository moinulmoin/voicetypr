@echo off
REM VoiceTypr Windows Release Script (Batch Wrapper)
REM This is a simple wrapper around the PowerShell script

setlocal enabledelayedexpansion

REM Check if PowerShell is available
where powershell >nul 2>&1
if errorlevel 1 (
    echo Error: PowerShell not found. This script requires PowerShell.
    echo Please install PowerShell or run the .ps1 script directly.
    exit /b 1
)

REM Get the script directory
set "SCRIPT_DIR=%~dp0"

REM Check if help was requested
if "%1"=="-h" goto :help
if "%1"=="--help" goto :help
if "%1"=="/?" goto :help

REM Run the PowerShell script with all arguments
echo Starting VoiceTypr Windows Release...
powershell -ExecutionPolicy Bypass -File "%SCRIPT_DIR%release-windows.ps1" %*
exit /b %errorlevel%

:help
echo VoiceTypr Windows Release Script
echo.
echo Usage:
echo   release-windows.bat [version]  - Build and release Windows version
echo   release-windows.bat -h        - Show this help
echo.
echo This script builds Windows MSI installer and update artifacts.
echo It should be run AFTER the macOS release script has created the release.
echo.
echo For detailed help, run: powershell -File scripts\release-windows.ps1 -Help
exit /b 0