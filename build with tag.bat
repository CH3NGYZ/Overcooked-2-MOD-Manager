@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul
pushd "%~dp0"
if errorlevel 1 exit /b 1

rem Reuse build.bat, including its noenv / clean arguments and build log.
if not exist "build.bat" (
    echo 错误：找不到 build.bat。
    goto failed
)

where node >nul 2>nul
if errorlevel 1 (
    echo 错误：PATH 中找不到 Node.js。
    goto failed
)

set "OC2_VERSION_BUILD_SCRIPT=%~f0"

:read_version
set "OC2_BUILD_VERSION="
set /p "OC2_BUILD_VERSION=请输入三段版本号（例如 1.2.3）："
node -e "const v=(process.env.OC2_BUILD_VERSION||'').trim(); process.exit(/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.test(v)?0:1)"
if errorlevel 1 (
    echo 错误：请使用三段数字，例如 1.2.3；不支持 v 前缀、前导零或测试版后缀。
    goto read_version
)

rem The embedded updater receives the version as data, never as shell code.
node -e "const fs=require('node:fs'); const vm=require('node:vm'); const p=process.env.OC2_VERSION_BUILD_SCRIPT; const source=fs.readFileSync(p,'utf8').replace(/\r\n/g,'\n').split('\n// VERSION_UPDATE_NODE\n')[1]; if(!source) throw new Error('Version updater was not found'); vm.runInThisContext(source,{filename:p});"
if errorlevel 1 goto failed

call "%~dp0build.bat" %*
set "OC2_VERSION_BUILD_EXIT=%ERRORLEVEL%"
popd
endlocal & exit /b %OC2_VERSION_BUILD_EXIT%

:failed
echo.
echo 版本更新失败，请查看上方错误信息。
pause
popd
endlocal & exit /b 1

// VERSION_UPDATE_NODE
(() => {
    const fs = require('node:fs');
    const path = require('node:path');
    const version = (process.env.OC2_BUILD_VERSION || '').trim();
    if (!/^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.test(version)) {
        throw new Error('Invalid version. Expected three numeric parts, for example 1.2.3.');
    }
    const root = path.dirname(process.env.OC2_VERSION_BUILD_SCRIPT);
    function readDocument(relativePath) {
        const file = path.join(root, relativePath);
        const original = fs.readFileSync(file);
        const raw = original.toString('utf8');
        return { file, original, bom: raw.startsWith('\uFEFF') ? '\uFEFF' : '', text: raw.replace(/^\uFEFF/, '') };
    }
    function updateJson(document, update) {
        const value = JSON.parse(document.text);
        update(value);
        const indent = document.text.match(/\r?\n([ \t]+)"/)?.[1] || '  ';
        const newline = document.text.includes('\r\n') ? '\r\n' : '\n';
        const trailing = document.text.endsWith('\n') ? newline : '';
        document.updated = Buffer.from(document.bom + JSON.stringify(value, null, indent).replace(/\n/g, newline) + trailing);
    }
    function setVersion(value, description) {
        if (!value || typeof value.version !== 'string') {
            throw new Error(description + ': project version was not found.');
        }
        value.version = version;
    }

    // Read and validate every target before writing any of them.
    const cargo = readDocument('src-tauri/Cargo.toml');
    const tauri = readDocument('src-tauri/tauri.conf.json');
    const manifest = readDocument('package.json');
    const lock = readDocument('package-lock.json');
    const packageSection = /^(\[package\][^\r\n]*(?:\r?\n|$))([\s\S]*?)(?=^\[|(?![\s\S]))/m;
    let packageCount = 0;
    const cargoText = cargo.text.replace(packageSection, (section, header, body) => {
        const versionLine = /^([ \t]*version[ \t]*=[ \t]*")[^"\r\n]*("[^\r\n]*)$/gm;
        const updatedBody = body.replace(versionLine, (_, prefix, suffix) => {
            packageCount += 1;
            return prefix + version + suffix;
        });
        return header + updatedBody;
    });
    if (packageCount !== 1) throw new Error('Cargo.toml must have exactly one [package] version.');
    cargo.updated = Buffer.from(cargo.bom + cargoText);
    updateJson(tauri, value => setVersion(value, 'tauri.conf.json'));
    updateJson(manifest, value => setVersion(value, 'package.json'));
    updateJson(lock, value => {
        setVersion(value, 'package-lock.json');
        setVersion(value.packages?.[''], 'package-lock.json packages[""]');
    });

    const written = [];
    try {
        for (const document of [cargo, tauri, manifest, lock]) {
            if (document.original.equals(document.updated)) continue;
            written.push(document);
            fs.writeFileSync(document.file, document.updated);
        }
    } catch (error) {
        for (const document of written.reverse()) {
            try { if (fs.readFileSync(document.file).equals(document.original)) continue; }
            catch { /* A partial write may also make the file unreadable; still attempt restoration. */ }
            try { fs.writeFileSync(document.file, document.original); }
            catch (restoreError) { console.error('Could not restore ' + path.relative(root, document.file) + ': ' + restoreError.message); }
        }
        throw error;
    }
    console.log('项目版本已更新为 ' + version + '：');
    console.log('  src-tauri/Cargo.toml, src-tauri/tauri.conf.json, package.json');
    console.log('  package-lock.json version and packages[""] version');
})();
