@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001
cd /d "%~dp0"

set "BUILD_LOG=%CD%\build.log"
set "BUILD_TARGET=x86_64-pc-windows-msvc"
if not defined CARGO_TARGET_DIR set "CARGO_TARGET_DIR=%CD%\src-tauri\target"
for %%A in ("%CARGO_TARGET_DIR%") do set "CARGO_TARGET_DIR=%%~fA"
set "NSIS_BUNDLE_DIR=%CARGO_TARGET_DIR%\%BUILD_TARGET%\release\bundle\nsis"

rem Default builds forward API tokens; noenv / clean clear those tokens.

set "USE_ENV=1"

if /I "%~1"=="noenv" set "USE_ENV=0"
if /I "%~1"=="clean" set "USE_ENV=0"

> "%BUILD_LOG%" echo [%date% %time%] OC2 MOD Manager build started

set "CARGO_HOME=D:\ProgramData\rust\.cargo"
set "RUSTUP_HOME=D:\ProgramData\rust\.rustup"
set "PATH=D:\ProgramData\rust\.cargo\bin;%PATH%"

rem Use every logical processor available for Cargo compilation.
if not defined NUMBER_OF_PROCESSORS set "NUMBER_OF_PROCESSORS=1"
set "CARGO_BUILD_JOBS=%NUMBER_OF_PROCESSORS%"

echo ========================================
echo OC2 MOD Manager build
echo ========================================
echo.

if "%USE_ENV%"=="1" (
    echo Build mode: WITH environment variables
    >>"%BUILD_LOG%" echo [%date% %time%] Build mode: WITH environment variables
) else (
    echo Build mode: WITHOUT environment variables
    >>"%BUILD_LOG%" echo [%date% %time%] Build mode: WITHOUT environment variables
)

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

where node >nul 2>nul
if errorlevel 1 (
    echo ERROR: Node.js was not found in PATH.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: Node.js was not found in PATH.
    goto failed
)

if not exist "scripts\build-release.mjs" (
    echo ERROR: the shared Actions release build script was not found.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: the shared release build script was not found.
    goto failed
)

if not exist "%CARGO_HOME%\bin\cargo.exe" (
    echo ERROR: cargo was not found at %CARGO_HOME%\bin\cargo.exe
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: cargo was not found at %CARGO_HOME%\bin\cargo.exe
    goto failed
)

rem Load or clear the build-time API tokens.

if "%USE_ENV%"=="1" goto load_environment

echo Environment variables are disabled for this build.
>>"%BUILD_LOG%" echo [%date% %time%] Environment variables are disabled for this build.

rem Clear inherited tokens too, so child compilers cannot receive them.
set "OC2_INSTALLER_GITHUB_TOKEN="
set "OC2_INSTALLER_GITEE_TOKEN="

goto environment_ready


:load_environment

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

>>"%BUILD_LOG%" echo [%date% %time%] GitHub installer token: configured
>>"%BUILD_LOG%" echo [%date% %time%] Gitee installer token: configured

goto environment_ready


:environment_ready

rem Updater signing is required even in noenv / clean mode. Those modes only
rem disable the two build-time API tokens, not the production updater signature.
if not defined TAURI_SIGNING_PRIVATE_KEY (
    for /f "tokens=1,2,*" %%A in ('reg query "HKCU\Environment" /v TAURI_SIGNING_PRIVATE_KEY 2^>nul') do if /I "%%A"=="TAURI_SIGNING_PRIVATE_KEY" set "TAURI_SIGNING_PRIVATE_KEY=%%C"
)
if not defined TAURI_SIGNING_PRIVATE_KEY_PATH (
    for /f "tokens=1,2,*" %%A in ('reg query "HKCU\Environment" /v TAURI_SIGNING_PRIVATE_KEY_PATH 2^>nul') do if /I "%%A"=="TAURI_SIGNING_PRIVATE_KEY_PATH" set "TAURI_SIGNING_PRIVATE_KEY_PATH=%%C"
)
if not defined TAURI_SIGNING_PRIVATE_KEY_PASSWORD (
    for /f "tokens=1,2,*" %%A in ('reg query "HKCU\Environment" /v TAURI_SIGNING_PRIVATE_KEY_PASSWORD 2^>nul') do if /I "%%A"=="TAURI_SIGNING_PRIVATE_KEY_PASSWORD" set "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=%%C"
)
if not defined TAURI_SIGNING_PRIVATE_KEY if not defined TAURI_SIGNING_PRIVATE_KEY_PATH (
    echo ERROR: configure TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH.
    echo Use the private key matching the updater public key, and its password if needed.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: an updater signing key is required.
    goto failed
)

echo Updater signing key: configured
>>"%BUILD_LOG%" echo [%date% %time%] Updater signing key: configured

echo Cargo parallel jobs: %CARGO_BUILD_JOBS%
>>"%BUILD_LOG%" echo [%date% %time%] Cargo parallel jobs: %CARGO_BUILD_JOBS%

echo Building frontend and NSIS installer...
>>"%BUILD_LOG%" echo [%date% %time%] Building frontend and NSIS installer...

rem Child processes inherit the configured tokens or cleared noenv / clean values.

rem Stream build output to the console while keeping a complete build log.
rem Reuse Actions' target, version-bound signatures and public-key verification.
rem Merge stderr inside cmd.exe without turning redirection into Node arguments.
rem Append UTF-8 explicitly; Windows PowerShell's Tee-Object defaults to UTF-16.

powershell -NoProfile -ExecutionPolicy Bypass -Command "& { $ErrorActionPreference = 'Stop'; [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); $buildExit = 1; $log = [IO.StreamWriter]::new($env:BUILD_LOG, $true, [Text.UTF8Encoding]::new($false)); $log.AutoFlush = $true; try { & cmd.exe /d /c 'node scripts/build-release.mjs build --target %BUILD_TARGET% --bundles nsis 2>&1' | ForEach-Object { $log.WriteLine($_); $_ }; $buildExit = $LASTEXITCODE } finally { $log.Dispose() }; exit $buildExit }"

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
echo %NSIS_BUNDLE_DIR%\
>>"%BUILD_LOG%" echo NSIS installer: %NSIS_BUNDLE_DIR%\

set "BUILD_VERSION="
for /f "delims=" %%A in ('node -p "JSON.parse(require('node:fs').readFileSync('package.json','utf8')).version"') do set "BUILD_VERSION=%%A"
if not defined BUILD_VERSION (
    echo ERROR: the current project version could not be read.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: the current project version could not be read.
    goto failed
)

set "NSIS_INSTALLER=%NSIS_BUNDLE_DIR%\OC2ModManager_%BUILD_VERSION%_x64-setup.exe"

if not exist "%NSIS_INSTALLER%" (
    echo ERROR: the NSIS installer for the current version was not found.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: the NSIS installer for the current version was not found.
    goto failed
)
if not exist "%NSIS_INSTALLER%.sig" (
    echo ERROR: the verified updater signature was not found.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: the verified updater signature was not found.
    goto failed
)

echo Installing NSIS package silently...
>>"%BUILD_LOG%" echo [%date% %time%] Installing NSIS package silently: %NSIS_INSTALLER%

start /wait "" "%NSIS_INSTALLER%" /S >> "%BUILD_LOG%" 2>&1

if errorlevel 1 (
    rem NSIS can return a non-zero code after completing an elevated install
    rem (for example when the installer requests a relaunch). Verify the
    rem installed executable before treating the result as a real failure.
    if exist "%LOCALAPPDATA%\OC2ModManager\OC2ModManager.exe" (
        echo WARNING: installer returned a non-zero code, but installation completed.
        >>"%BUILD_LOG%" echo [%date% %time%] WARNING: installer returned %ERRORLEVEL%, installation completed.
    ) else (
        echo ERROR: silent installation failed.
        >>"%BUILD_LOG%" echo [%date% %time%] ERROR: silent installation failed.
        goto failed
    )
)

set "INSTALLED_EXE=%LOCALAPPDATA%\OC2ModManager\OC2ModManager.exe"

if not exist "%INSTALLED_EXE%" (
    echo ERROR: installed executable was not found at %INSTALLED_EXE%.
    >>"%BUILD_LOG%" echo [%date% %time%] ERROR: installed executable was not found at %INSTALLED_EXE%.
    goto failed
)

echo Starting installed application...
>>"%BUILD_LOG%" echo [%date% %time%] Starting installed application: %INSTALLED_EXE%

rem Clear the installer process exit code before invoking START. NSIS may
rem legitimately return 2 after a completed installation.
ver >nul
start "OC2 MOD Manager" "%INSTALLED_EXE%"

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
