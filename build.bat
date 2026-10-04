@echo off
setlocal EnableExtensions

cd /d "%~dp0"

set "BUILD_LOG=%CD%\build.log"
> "%BUILD_LOG%" echo [%date% %time%] OC2 Mod Manager build started

set "CARGO_HOME=D:\ProgramData\rust\.cargo"
set "RUSTUP_HOME=D:\ProgramData\rust\.rustup"
set "PATH=D:\ProgramData\rust\.cargo\bin;%PATH%"
rem Use every logical processor available for Cargo compilation.
if not defined NUMBER_OF_PROCESSORS set "NUMBER_OF_PROCESSORS=1"
set "CARGO_BUILD_JOBS=%NUMBER_OF_PROCESSORS%"

echo ========================================
echo OC2 Mod Manager build
echo ========================================
echo.

if not exist "package.json" (
    echo ERROR: package.json was not found.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: package.json was not found.
    goto failed
)

if not exist "src-tauri\Cargo.toml" (
    echo ERROR: src-tauri\Cargo.toml was not found.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: src-tauri\Cargo.toml was not found.
    goto failed
)

where npm >nul 2>nul
if errorlevel 1 (
    echo ERROR: npm was not found in PATH.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: npm was not found in PATH.
    goto failed
)

if not exist "%CARGO_HOME%\bin\cargo.exe" (
    echo ERROR: cargo was not found at %CARGO_HOME%\bin\cargo.exe
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: cargo was not found at %CARGO_HOME%\bin\cargo.exe
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
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: OC2_INSTALLER_GITHUB_TOKEN is not set.
    goto failed
)

if not defined OC2_INSTALLER_GITEE_TOKEN (
    echo ERROR: OC2_INSTALLER_GITEE_TOKEN is not set.
    echo Set the build-time Gitee token in the environment without putting it in source files.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: OC2_INSTALLER_GITEE_TOKEN is not set.
    goto failed
)

echo GitHub installer token: configured
echo Gitee installer token: configured
echo Cargo parallel jobs: %CARGO_BUILD_JOBS%
>>"%BUILD_LOG%" echo [%date% %time%] Cargo parallel jobs: %CARGO_BUILD_JOBS%

echo Building frontend and NSIS installer...
>>"%BUILD_LOG%" echo [%date% %time%] Building frontend and NSIS installer...
rem Stream build output to the console while keeping a complete build log.
rem Merge npx stderr inside cmd.exe so normal Tauri status lines are not wrapped as PowerShell errors.
powershell -NoProfile -ExecutionPolicy Bypass -Command "& { & cmd.exe /d /c 'call npx tauri build --bundles nsis 2^>^&1' | Tee-Object -FilePath $env:BUILD_LOG -Append; exit $LASTEXITCODE }"
set "BUILD_EXIT=%ERRORLEVEL%"
if not "%BUILD_EXIT%"=="0" (
    echo.
    echo BUILD FAILED.
    >>"%BUILD_LOG%" echo [%date% %time%] BUILD FAILED.
    goto failed
)

echo.
echo BUILD SUCCEEDED.
>>"%BUILD_LOG%" echo [%date% %time%] BUILD SUCCEEDED.
echo.
echo NSIS installer:
echo %CD%\src-tauri\target\release\bundle\nsis\
>>"%BUILD_LOG%" echo NSIS installer: %CD%\src-tauri\target\release\bundle\nsis\

set "NSIS_INSTALLER="
for /f "delims=" %%A in ('dir /b /a-d /o-d "%CD%\src-tauri\target\release\bundle\nsis\OC2ModManager_*_x64-setup.exe" 2^>nul') do if not defined NSIS_INSTALLER set "NSIS_INSTALLER=%CD%\src-tauri\target\release\bundle\nsis\%%A"
if not defined NSIS_INSTALLER (
    echo ERROR: NSIS installer was not found.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: NSIS installer was not found.
    goto failed
)

echo Installing NSIS package silently...
>>"%BUILD_LOG%" echo [%date% %time%] Installing NSIS package silently: %NSIS_INSTALLER%
taskkill /IM OC2ModManager.exe /T /F >nul 2>&1
start /wait "" "%NSIS_INSTALLER%" /S >> "%BUILD_LOG%" 2>&1
if errorlevel 1 (
    echo ERROR: silent installation failed.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: silent installation failed.
    goto failed
)

set "INSTALLED_EXE=%LOCALAPPDATA%\OC2ModManager\OC2ModManager.exe"
if not exist "%INSTALLED_EXE%" (
    echo ERROR: installed executable was not found at %INSTALLED_EXE%.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: installed executable was not found at %INSTALLED_EXE%.
    goto failed
)

echo Starting installed application...
>>"%BUILD_LOG%" echo [%date% %time%] Starting installed application: %INSTALLED_EXE%
start "OC2 Mod Manager" "%INSTALLED_EXE%"
if errorlevel 1 (
    echo ERROR: installed application could not be started.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: installed application could not be started.
    goto failed
)
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
