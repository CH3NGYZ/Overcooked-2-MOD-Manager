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
    goto failed
)

if not exist "src-tauri\Cargo.toml" (
    echo ERROR: src-tauri\Cargo.toml was not found.
    goto failed
)

where npm >nul 2>nul
if errorlevel 1 (
    echo ERROR: npm was not found in PATH.
    goto failed
)

if not exist "%CARGO_HOME%\bin\cargo.exe" (
    echo ERROR: cargo was not found at %CARGO_HOME%\bin\cargo.exe
    goto failed
)

rem A terminal opened before setx or the Environment dialog may not have the
rem latest user variables. Read the current user's registry values as a
rem fallback without printing their contents.
if not defined OC2_INSTALLER_GITHUB_TOKEN (
    for /f "tokens=1,2,*" %%A in ('reg query "HKCU\Environment" /v OC2_INSTALLER_GITHUB_TOKEN 2^>nul') do if /I "%%A"=="OC2_INSTALLER_GITHUB_TOKEN" set "OC2_INSTALLER_GITHUB_TOKEN=%%C"
)
if not defined OC2_INSTALLER_GITEE_TOKEN (
    for /f "tokens=1,2,*" %%A in ('reg query "HKCU\Environment" /v OC2_INSTALLER_GITEE_TOKEN 2^>nul') do if /I "%%A"=="OC2_INSTALLER_GITEE_TOKEN" set "OC2_INSTALLER_GITEE_TOKEN=%%C"
)

if not defined OC2_INSTALLER_GITHUB_TOKEN (
    echo ERROR: OC2_INSTALLER_GITHUB_TOKEN is not set.
    echo Set the build-time GitHub token in the environment without putting it in source files.
    goto failed
)

if not defined OC2_INSTALLER_GITEE_TOKEN (
    echo ERROR: OC2_INSTALLER_GITEE_TOKEN is not set.
    echo Set the build-time Gitee token in the environment without putting it in source files.
    goto failed
)

echo GitHub installer token: configured
echo Gitee installer token: configured

echo Building frontend and NSIS installer...
call npx tauri build --bundles nsis
if errorlevel 1 (
    echo.
    echo BUILD FAILED.
    goto failed
)

echo.
echo BUILD SUCCEEDED.
echo.
echo NSIS installer:
echo %CD%\src-tauri\target\release\bundle\nsis\
goto done

:failed
set "EXIT_CODE=1"
if not defined EXIT_CODE set "EXIT_CODE=1"
echo.
echo See the messages above for the failure details.
goto finish

:done
set "EXIT_CODE=0"

:finish
echo.
pause
exit /b %EXIT_CODE%
