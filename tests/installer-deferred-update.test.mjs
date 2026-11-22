import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { once } from 'node:events'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const installer = readFileSync(new URL('../src-tauri/nsis/installer.nsi', import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const hooks = readFileSync(new URL('../src-tauri/installer-hooks.nsh', import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const deferredPath = fileURLToPath(new URL('../src-tauri/nsis/deferred-update.nsh', import.meta.url))
const makensis = join(process.env.LOCALAPPDATA ?? '', 'tauri', 'NSIS', 'makensis.exe')
const csc = join(process.env.WINDIR ?? 'C:\\Windows', 'Microsoft.NET', 'Framework64', 'v4.0.30319', 'csc.exe')
const nativeProbeAvailable = process.platform === 'win32' && existsSync(makensis) && existsSync(csc)

function fn(name, source = installer) {
  const start = source.indexOf(`Function ${name}\n`)
  assert.notEqual(start, -1, `${name} must exist in the production installer`)
  return source.slice(start, source.indexOf('FunctionEnd', start) + 'FunctionEnd'.length)
}

test('deferred update waits before installer initialization and keeps legacy updates separate', () => {
  const init = fn('.onInit')
  assert.match(init, /Call OC2WaitForUpdateOwner[\s\S]*?MUI_LANGDLL_DISPLAY[\s\S]*?SetContext/)
  assert.match(installer, /^VIAddVersionKey \/LANG=1033 "OC2DeferredUpdate" "1"$/m)
  assert.match(installer, /^VIAddVersionKey \/LANG=1033 "FileVersion" /m)
  assert.match(installer, /^VIAddVersionKey \/LANG=1033 "ProductVersion" /m)
  assert.match(hooks, /!include [^\n]*deferred-update\.nsh/)
  assert.match(installer, /!insertmacro OC2_DEFERRED_UPDATE_FUNCTIONS/)
  const install = installer.split('Section Install\n')[1].split('SectionEnd')[0]
  assert.match(install, /\$\{If\} \$OC2DeferredUpdate = 1\n\s*Call OC2CheckDeferredTarget\n\s*\$\{EndIf\}[\s\S]*?SetOutPath \$INSTDIR\n\n\s*\$\{If\} \$OC2DeferredUpdate != 1[\s\S]*?NSIS_HOOK_PREINSTALL[\s\S]*?CheckIfAppIsRunning[\s\S]*?\$\{EndIf\}/)
  assert.match(install, /!insertmacro OC2_STAGE_MAIN_BINARY[\s\S]*?File \/a [\s\S]*?WriteUninstaller "\$INSTDIR\\uninstall.exe"[\s\S]*?Call OC2ReplaceDeferredMain/)
  assert.match(install, /WriteUninstaller "\$INSTDIR\\uninstall.exe"\n\s*\$\{If\} \$OC2DeferredUpdate = 1\n\s*\$\{AndIf\} \$\{Errors\}[\s\S]*?SetErrorLevel 1\n\s*Abort\n\s*\$\{EndIf\}[\s\S]*?Call OC2ReplaceDeferredMain/)
  assert.match(fn('.onInstFailed'), /Delete "\$OC2StagedMain"/)
  const deferred = readFileSync(deferredPath, 'utf8').replace(/\r\n/g, '\n')
  assert.doesNotMatch(fn('OC2CheckDeferredTarget', deferred), /RmShutdown|taskkill/)
  assert.match(fn('OC2ReplaceDeferredMain', deferred), /MoveFileExW\([^\n]*\$OC2StagedMain[^\n]*\$INSTDIR\\\$\{MAINBINARYNAME\}\.exe[^\n]*\bi 9\b/)
})

test('migration and ordinary installer success honor the deferred restart choice', () => {
  const success = fn('.onInstSuccess')
  assert.match(success, /\$\{If\} \$OC2DeferredUpdate = 1\n\s*\$\{AndIf\} \$OC2RestartAfterUpdate != 1\n[^\n]+ finish "\$OC2MigrationPlan" --no-restart'/)
  assert.match(success, /finish "\$OC2MigrationPlan"'\n\s*\$\{EndIf\}/)
  assert.match(success, /NSIS_HOOK_ONINSTSUCCESS[\s\S]*?\$\{If\} \$OC2DeferredUpdate = 1\n\s*!insertmacro OC2_UPDATE_RESULT "installed"\n\s*Return[\s\S]*?GetOptions\} \$CMDLINE "\/R"/)
  assert.match(hooks, /\$\{If\} \$OC2RestartAfterUpdate = 1\n\s*ClearErrors\n\s*Exec/)
})

test('native NSIS handoff waits, publishes the staged EXE and reports its result', {
  skip: nativeProbeAvailable ? false : 'requires Windows NSIS and .NET Framework compiler tools',
}, async t => {
  const temporary = fileURLToPath(new URL('../.tmp/', import.meta.url))
  mkdirSync(temporary, { recursive: true })
  const directory = mkdtempSync(join(temporary, 'installer-deferred-probe-'))
  const runtime = join(directory, 'runtime')
  mkdirSync(runtime)
  t.diagnostic(`Isolated deferred probe evidence: ${directory}`)
  const env = { ...process.env, TEMP: runtime, TMP: runtime }
  const options = { cwd: directory, env, windowsHide: true }
  const run = (executable, args, overrides = {}) => {
    const result = spawnSync(executable, args, { ...options, ...overrides, encoding: 'utf8', timeout: 8000 })
    assert.ifError(result.error)
    return result
  }
  const quote = text => text.replaceAll('$', '$$').replaceAll('"', '$\\"')
  const marker = installer.match(/^VIAddVersionKey \/LANG=1033 "OC2DeferredUpdate" "1"$/m)?.[0]
  const executable = join(directory, 'deferred-probe.exe')
  const script = join(directory, 'deferred-probe.nsi')
  const newBinary = join(directory, 'new-main.exe')
  const newBytes = Buffer.from('fixture replacement EXE contents')
  writeFileSync(newBinary, newBytes)
  writeFileSync(script, `Unicode true
RequestExecutionLevel user
OutFile "${quote(executable)}"
VIProductVersion "1.0.0.0"
${marker}
!include MUI2.nsh
!include FileFunc.nsh
!define MUI_LANGDLL_ALWAYSSHOW
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
!define MAINBINARYNAME "occupant"
!define MAINBINARYSRCPATH "${quote(newBinary)}"
!define VERSION "2.4.1"
!define OC2_UPDATE_EXIT_WAIT_MS 2000
Var UpdateMode
!include "${quote(deferredPath)}"
!insertmacro OC2_DEFERRED_UPDATE_FUNCTIONS
Function .onInit
  \${If} $INSTDIR == ""
    StrCpy $INSTDIR "$EXEDIR"
  \${EndIf}
  Call OC2WaitForUpdateOwner
  !insertmacro MUI_LANGDLL_DISPLAY
  \${If} $OC2DeferredUpdate = 1
    ReadEnvStr $0 OC2_PROBE_PUBLISH
    \${If} $0 == "1"
      SetOutPath "$INSTDIR"
      !insertmacro OC2_STAGE_MAIN_BINARY
      Call OC2ReplaceDeferredMain
      !insertmacro OC2_UPDATE_RESULT "installed"
    \${Else}
      Call OC2CheckDeferredTarget
    \${EndIf}
  \${EndIf}
  FileOpen $0 "$EXEDIR\\continued.txt" a
  FileSeek $0 0 END
  FileWrite $0 "$OC2DeferredUpdate|$OC2RestartAfterUpdate$\\r$\\n"
  FileClose $0
  SetErrorLevel 0
  Quit
FunctionEnd
Section
SectionEnd
`, 'utf8')
  const compiled = run(makensis, ['/V3', '/INPUTCHARSET', 'UTF8', script])
  assert.equal(compiled.status, 0, compiled.stdout + compiled.stderr)

  const occupant = join(directory, 'occupant.exe')
  const occupantSource = join(directory, 'occupant.cs')
  writeFileSync(occupantSource, String.raw`using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
class Occupant {
  [DllImport("version.dll", CharSet = CharSet.Unicode)]
  static extern uint GetFileVersionInfoSize(string path, out uint handle);
  [DllImport("version.dll", CharSet = CharSet.Unicode)]
  static extern bool GetFileVersionInfo(string path, uint handle, uint size, IntPtr data);
  [DllImport("version.dll", CharSet = CharSet.Unicode)]
  static extern bool VerQueryValue(IntPtr data, string query, out IntPtr value, out uint size);
  static int Main(string[] args) {
    if (args.Length == 1) {
      uint handle, length;
      IntPtr value;
      uint size = GetFileVersionInfoSize(args[0], out handle);
      if (size == 0) return 1;
      IntPtr data = Marshal.AllocHGlobal((int)size);
      try {
        if (!GetFileVersionInfo(args[0], 0, size, data)) return 2;
        if (!VerQueryValue(data, @"\StringFileInfo\040904b0\OC2DeferredUpdate", out value, out length)) return 3;
        Console.WriteLine(Marshal.PtrToStringUni(value));
        return 0;
      } finally { Marshal.FreeHGlobal(data); }
    }
    File.WriteAllText(args[0], "ready");
    while (!File.Exists(args[1])) Thread.Sleep(10);
    return 0;
  }
}`, 'utf8')
  const helper = run(csc, ['/nologo', '/target:exe', `/out:${occupant}`, occupantSource])
  assert.equal(helper.status, 0, helper.stdout + helper.stderr)
  const readMarker = run(occupant, [executable])
  assert.equal(readMarker.status, 0, readMarker.stdout + readMarker.stderr)
  assert.equal(readMarker.stdout.trim(), '1')

  const trace = join(directory, 'continued.txt')
  writeFileSync(trace, '', 'utf8')
  const contents = () => readFileSync(trace, 'utf8')
  const args = (parameter, auto = false) => ['/S', '/UPDATE', `/OC2WaitForPid${parameter}`, ...(auto ? ['/OC2AutoLaunch'] : [])]
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms))
  const namedProcesses = []
  let sequence = 0
  async function launchOccupant() {
    const ready = join(directory, `ready-${++sequence}`)
    const stop = join(directory, `stop-${sequence}`)
    const child = spawn(occupant, [ready, stop], options)
    namedProcesses.push(child)
    const closed = once(child, 'exit')
    const deadline = Date.now() + 5000
    while (!existsSync(ready) && Date.now() < deadline && child.exitCode === null) await delay(10)
    assert.ok(existsSync(ready), 'fixture process did not start')
    return { pid: child.pid, child, closed, stop: () => writeFileSync(stop, '') }
  }
  t.after(() => {
    for (const child of namedProcesses) if (child.exitCode === null) child.kill()
  })

  assert.equal(run(executable, ['/S']).status, 0)
  assert.equal(contents(), '|\r\n')
  for (const invalid of ['', '=', '=0', '=-1', '=+2', '=xyz', '=7x', '=4294967296']) {
    assert.equal(run(executable, args(invalid)).status, 1, `invalid PID ${invalid}`)
  }
  assert.equal(run(executable, ['/S', '/OC2WaitForPid=999']).status, 1)
  assert.equal(run(executable, ['/UPDATE', '/OC2WaitForPid=999']).status, 1)
  assert.equal(contents(), '|\r\n')

  const waitingOwner = await launchOccupant()
  const waitingInstaller = spawn(executable, args(`=${waitingOwner.pid}`, true), options)
  namedProcesses.push(waitingInstaller)
  const waitingCode = once(waitingInstaller, 'exit')
  await delay(200)
  assert.equal(waitingInstaller.exitCode, null)
  assert.equal(contents(), '|\r\n')
  waitingOwner.stop()
  assert.equal((await waitingOwner.closed)[0], 0)
  assert.equal((await waitingCode)[0], 0)
  assert.match(contents(), /^\|\r\n1\|1\r\n$/)

  const other = await launchOccupant()
  assert.equal(run(executable, args(`=${other.pid}`)).status, 1, 'a still-live owner times out')
  assert.equal(other.child.exitCode, null)
  assert.equal(run(executable, args(`=${waitingOwner.pid}`)).status, 1, 'a different process holds the target')
  assert.equal(other.child.exitCode, null)
  other.stop()
  assert.equal((await other.closed)[0], 0)
  assert.equal(run(executable, args(`=${other.pid}`)).status, 0)
  assert.match(contents(), /^\|\r\n1\|1\r\n1\|0?\r\n$/)

  const receiptPath = join(directory, 'update-result.txt')
  const oldBytes = readFileSync(occupant)
  const publish = (pid, restart, target) => {
    rmSync(receiptPath, { force: true })
    const result = run(executable, [...args(`=${pid}`, restart), `/D=${target}`], {
      env: { ...env, OC2_PROBE_PUBLISH: '1' }, windowsVerbatimArguments: true,
    })
    const receipt = readFileSync(receiptPath, 'utf16le').replace(/^\uFEFF/, '').trimEnd()
    return { exit: result.status, receipt }
  }
  const locked = await launchOccupant()
  const lockedResult = publish(other.pid, false, directory)
  assert.equal(lockedResult.exit, 1)
  assert.equal(lockedResult.receipt, `2.4.1|replace_executable_failed|${directory}|restart=0`)
  assert.deepEqual(readFileSync(occupant), oldBytes)
  assert.equal(locked.child.exitCode, null)
  locked.stop()
  assert.equal((await locked.closed)[0], 0)

  const installDirectory = join(directory, 'install 更新 with spaces')
  mkdirSync(installDirectory)
  const installedBinary = join(installDirectory, 'occupant.exe')
  writeFileSync(installedBinary, oldBytes)
  for (const restart of [false, true]) {
    const result = publish(other.pid, restart, installDirectory)
    assert.equal(result.exit, 0)
    assert.equal(result.receipt, `2.4.1|installed|${installDirectory}|restart=${Number(restart)}`)
    assert.deepEqual(readFileSync(installedBinary), newBytes)
    assert.deepEqual(readFileSync(occupant), oldBytes)
  }
})
