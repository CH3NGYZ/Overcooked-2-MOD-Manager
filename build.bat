@echo off
setlocal EnableExtensions

cd /d "%~dp0"

set "CARGO_HOME=D:\ProgramData\rust\.cargo"
set "RUSTUP_HOME=D:\ProgramData\rust\.rustup"
set "PATH=D:\ProgramData\rust\.cargo\bin;%PATH%"

echo ========================================
echo OC2 Mod Manager build
echo ========================================
echo.

if not exist "package.json" (
    echo ERROR: package.json was not found.
    goto :failed
)

if not exist "src-tauri\Cargo.toml" (
    echo ERROR: src-tauri\Cargo.toml was not found.
    goto :failed
)

where npm >nul 2>nul
if errorlevel 1 (
    echo ERROR: npm was not found in PATH.
    goto :failed
)

if not exist "%CARGO_HOME%\bin\cargo.exe" (
    echo ERROR: cargo was not found at %CARGO_HOME%\bin\cargo.exe
    goto :failed
)


echo Building frontend and Windows bundles...
call npx tauri build
if errorlevel 1 (
    echo.
    echo BUILD FAILED.
    goto :failed
)

copy /y "src-tauri\target\release\app.exe" "src-tauri\target\release\OC2ModManager.exe" >nul
if errorlevel 1 (
    echo ERROR: Failed to create portable EXE copy.
    goto :failed
)
copy /y "src-tauri\target\release\bundle\msi\OC2 Mod Manager_1.0.2_x64_zh-CN.msi" "src-tauri\target\release\bundle\msi\OC2ModManager.msi" >nul
if errorlevel 1 (
    echo.
    echo ERROR: Failed to create portable or MSI delivery copy.
    goto :failed
)

echo.
echo BUILD SUCCEEDED.
echo.
echo Portable EXE:
echo %CD%\src-tauri\target\release\app.exe
echo %CD%\src-tauri\target\release\OC2ModManager.exe
echo.
echo MSI installer:
echo %CD%\src-tauri\target\release\bundle\msi\OC2 Mod Manager_1.0.2_x64_zh-CN.msi
echo %CD%\src-tauri\target\release\bundle\msi\OC2ModManager.msi
echo.
echo NSIS installer:
echo %CD%\src-tauri\target\release\bundle\nsis\OC2 Mod Manager_1.0.2_x64-setup.exe
goto :done

:failed
set "EXIT_CODE=1"
if not defined EXIT_CODE set "EXIT_CODE=1"
echo.
echo See the messages above for the failure details.
goto :finish

:done
set "EXIT_CODE=0"

:finish
echo.
pause
exit /b %EXIT_CODE%
