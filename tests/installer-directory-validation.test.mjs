import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const source = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const installer = source('src-tauri/nsis/installer.nsi')
const guard = installer.split('Function OC2ValidateInstallDirectory\n')[1].split('FunctionEnd')[0]

test('only an explicit unsafe-directory verdict can trigger automatic migration', () => {
  assert.match(guard, /\$\{If\} \$0 == 1\n\s*\$\{AndIf\} \$UpdateMode = 1\n\s*\$\{AndIf\} \$OC2MigrationPlan == ""/)
  assert.doesNotMatch(guard, /\$\{If\} \$0 != 0\n\s*\$\{AndIf\} \$UpdateMode = 1/)
  assert.match(guard, /\$\{If\} \$0 != 1[\s\S]*?Goto oc2_guard_allowed/)
  assert.ok(guard.indexOf('Goto oc2_guard_allowed') < guard.indexOf('--oc2-migrate-update prepare'))
  assert.match(guard, /StrCpy \$2 "\$\(OC2InvalidInstallDirectory\)"/)
  assert.match(guard, /\$\{Else\}\n\s*StrCpy \$2 "\$\(OC2MigrationFailed\)"/)
})

test('unavailable checks may continue while explicit rejections remain closed and retain diagnostics', () => {
  assert.match(guard, /nsExec::ExecToStack \/TIMEOUT=10000[^\n]+\n\s*Pop \$0\n\s*Pop \$1/)
  assert.match(guard, /\$\{If\} \$0 != 0\n\s*DetailPrint "Install directory validation: directory=\$INSTDIR; status=\$0"\n\s*DetailPrint "\$1"/)
  assert.match(guard, /\$\{IfNot\} \$\{Silent\}\n\s*MessageBox MB_OK\|MB_ICONEXCLAMATION "\$\(OC2InstallDirectoryValidationFailed\)" \/SD IDOK/)
  assert.match(guard, /MessageBox MB_OK\|MB_ICONSTOP "\$2" \/SD IDOK\n\s*\$\{EndIf\}\n\s*SetErrorLevel 1\n\s*Pop \$2\n\s*Pop \$1\n\s*Pop \$0\n\s*Abort/)
  assert.match(guard, /oc2_guard_allowed:\n\s*SetErrorLevel 0\n\s*Pop \$2\n\s*Pop \$1\n\s*Pop \$0/)
  // Directory selection, old removal, silent checks and target writes all stay protected.
  assert.match(installer, /MUI_PAGE_CUSTOMFUNCTION_LEAVE OC2ValidateInstallDirectory/)
  assert.match(installer, /reinst_uninstall:[\s\S]*?Call OC2ValidateInstallDirectory[\s\S]*?HideWindow/)
  assert.match(installer, /Section EarlyChecks[\s\S]*?Call OC2ValidateInstallDirectory/)
  assert.match(installer, /Section Install\n\s*Call OC2ValidateInstallDirectory\n\s*SetOutPath \$INSTDIR/)
})

test('installer errors separate helper failures from BepInEx restrictions in all five languages', () => {
  const hooks = source('src-tauri/installer-hooks.nsh')
  const languages = ['1028', '1033', '1041', '1042', '2052']
  for (const key of ['OC2InvalidInstallDirectory', 'OC2InstallDirectoryValidationFailed']) {
    const messages = [...hooks.matchAll(new RegExp(`^LangString ${key} (\\d+) "([^"\\n]+)"$`, 'gm'))]
    assert.deepEqual(messages.map(match => match[1]).sort(), languages)
    for (const [, language, message] of messages) {
      if (key === 'OC2InvalidInstallDirectory') {
        assert.match(message, /BepInEx/, language)
        assert.doesNotMatch(message, /plugins|config\./, language)
      } else {
        assert.match(message, /\$0/, language)
        assert.doesNotMatch(message, /BepInEx/, language)
        const continuation = {
          2052: /跳过此检查并继续安装/, 1028: /略過此檢查並繼續安裝/,
          1033: /check will be skipped so installation can continue/,
          1042: /확인을 건너뛰고 설치를 계속/, 1041: /確認をスキップしてインストールを続行/,
        }
        assert.match(message, continuation[language], language)
      }
    }
  }
})

const makensis = join(process.env.LOCALAPPDATA ?? '', 'tauri', 'NSIS', 'makensis.exe')
const csc = join(process.env.WINDIR ?? 'C:\\Windows', 'Microsoft.NET', 'Framework64', 'v4.0.30319', 'csc.exe')
const nativeProbeAvailable = process.platform === 'win32' && existsSync(makensis) && existsSync(csc)

test('the actual NSIS guard handles validator outcomes without installing the product', {
  skip: nativeProbeAvailable ? false : 'requires the Windows NSIS and .NET Framework compiler tools',
}, async t => {
  const temporary = fileURLToPath(new URL('../.tmp/', import.meta.url))
  mkdirSync(temporary, { recursive: true })
  const directory = mkdtempSync(join(temporary, 'installer-guard-probe-'))
  const runtime = join(directory, 'runtime')
  mkdirSync(runtime)
  t.diagnostic(`Isolated guard probe evidence: ${directory}`)
  const helper = join(directory, 'probe-helper.exe')
  const helperSource = join(directory, 'probe-helper.cs')
  writeFileSync(helperSource, `using System;
using System.IO;
using System.Threading;
class ProbeHelper {
  static int Main(string[] args) {
    File.AppendAllText(Environment.GetEnvironmentVariable("OC2_PROBE_HELPER_TRACE"), String.Join("|", args) + Environment.NewLine);
    bool migrating = args.Length > 0 && args[0] == "--oc2-migrate-update";
    string status = Environment.GetEnvironmentVariable(migrating ? "OC2_PROBE_MIGRATION_STATUS" : "OC2_PROBE_GUARD_STATUS");
    Console.WriteLine("probe helper output (" + status + ")");
    if (status == "timeout") { Thread.Sleep(20000); return 0; }
    return Int32.Parse(status);
  }
}`, 'utf8')
  const run = (executable, args, env = {}) => {
    const result = spawnSync(executable, args, {
      cwd: directory, env: { ...process.env, TEMP: runtime, TMP: runtime, ...env }, encoding: 'utf8',
      timeout: 25000, windowsHide: true, maxBuffer: 1024 * 1024,
    })
    assert.ifError(result.error)
    return result
  }
  const compiled = run(csc, ['/nologo', '/target:exe', `/out:${helper}`, helperSource])
  assert.equal(compiled.status, 0, compiled.stdout + compiled.stderr)

  // Keep the real instructions and conditionals. Capture only UI/log output,
  // and stop in .onInit before a product section, registry or user data exists.
  const probedGuard = guard
    .replace(/^\s*DetailPrint "([^"\n]+)"$/gm, (_, text) => {
      const key = text.startsWith('$(') ? `${text.slice(2, -1)}:` : ''
      return `    FileWriteUTF16LE $ProbeTrace "detail:${key}${text}$\\r$\\n"`
    })
    .replace(/^\s*MessageBox MB_OK\|MB_ICON(?:STOP|EXCLAMATION) "([^"\n]+)" \/SD IDOK$/gm,
      (_, text) => `    FileWriteUTF16LE $ProbeTrace "message:${text}$\\r$\\n"`)
    .replace(/^\s*Abort$/gm, '    FileWriteUTF16LE $ProbeTrace "aborted$\\r$\\nregisters:$0|$1|$2$\\r$\\n"\n    Abort')
  assert.doesNotMatch(probedGuard, /^\s*(?:MessageBox|DetailPrint)\b/m, 'the fixture must never display a dialog')
  const quote = text => text.replaceAll('$', '$$').replaceAll('"', '$\\"')
  const hooks = fileURLToPath(new URL('../src-tauri/installer-hooks.nsh', import.meta.url))
  const buildProbe = (name, embeddedHelper) => {
    const executable = join(directory, `${name}.exe`)
    const script = join(directory, `${name}.nsi`)
    writeFileSync(script, `Unicode true
RequestExecutionLevel user
OutFile "${quote(executable)}"
!define MAINBINARYSRCPATH "${quote(embeddedHelper)}"
!define PRODUCTNAME "OC2GuardProbe"
!include "${quote(hooks)}"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\English.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\SimpChinese.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\TradChinese.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\Korean.nlf"
LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\Japanese.nlf"
Var UpdateMode
Var OC2MigrationPlan
Var OC2MigrationTarget
Var ProbeTrace
!macro SetContext
!macroend
Function OC2ValidateInstallDirectory
${probedGuard}
FunctionEnd
Function .onInit
  StrCpy $LANGUAGE 1033
  StrCpy $INSTDIR "$EXEDIR\\chosen"
  ReadEnvStr $UpdateMode OC2_PROBE_UPDATE
  ReadEnvStr $OC2MigrationPlan OC2_PROBE_EXISTING_PLAN
  FileOpen $ProbeTrace "$EXEDIR\\probe-result.txt" w
  StrCpy $0 "saved-zero"
  StrCpy $1 "saved-one"
  StrCpy $2 "saved-two"
  Call OC2ValidateInstallDirectory
  FileWriteUTF16LE $ProbeTrace "continued$\\r$\\nregisters:$0|$1|$2$\\r$\\nplan:$OC2MigrationPlan$\\r$\\ndirectory:$INSTDIR$\\r$\\n"
  FileClose $ProbeTrace
  Quit
FunctionEnd
Section
SectionEnd
`, 'utf8')
    const result = run(makensis, ['/V2', '/INPUTCHARSET', 'UTF8', script])
    assert.equal(result.status, 0, result.stdout + result.stderr)
    return executable
  }
  const executable = buildProbe('guard-probe', helper)
  const invalidHelper = join(directory, 'invalid-helper.exe')
  writeFileSync(invalidHelper, 'This is deliberately not a Windows executable.', 'utf8')
  const invalidExecutable = buildProbe('guard-unavailable-probe', invalidHelper)
  let sequence = 0
  const probe = ({ status = '0', silent = true, update = false, migrationStatus = '0', invalid = false, existingPlan = '' } = {}) => {
    const helperTrace = join(directory, 'helper-invocations.txt')
    writeFileSync(helperTrace, '', 'utf8')
    const result = run(invalid ? invalidExecutable : executable, silent ? ['/S'] : [], {
      OC2_PROBE_HELPER_TRACE: helperTrace, OC2_PROBE_GUARD_STATUS: status,
      OC2_PROBE_MIGRATION_STATUS: migrationStatus, OC2_PROBE_UPDATE: update ? '1' : '0',
      OC2_PROBE_EXISTING_PLAN: existingPlan,
    })
    const observation = {
      exit: result.status,
      trace: readFileSync(join(directory, 'probe-result.txt'), 'utf16le'),
      invocations: readFileSync(helperTrace, 'utf8'),
    }
    writeFileSync(join(directory, `case-${++sequence}.json`), JSON.stringify({ status, silent, update, migrationStatus, invalid, ...observation }, null, 2), 'utf8')
    return observation
  }
  for (const silent of [false, true]) {
    await t.test(`allowed directory: silent=${silent}`, () => {
      const result = probe({ silent })
      assert.equal(result.exit, 0)
      assert.match(result.trace, /continued/)
      assert.match(result.trace, /registers:saved-zero\|saved-one\|saved-two/)
      assert.doesNotMatch(result.trace, /message:|aborted|status=/)
      assert.doesNotMatch(result.invocations, /--oc2-migrate-update/)
    })
    await t.test(`forbidden directory: silent=${silent}`, () => {
      const result = probe({ status: '1', silent })
      assert.equal(result.exit, 1)
      assert.match(result.trace, /aborted/)
      assert.match(result.trace, /registers:saved-zero\|saved-one\|saved-two/)
      assert.match(result.trace, /status=1/)
      assert.match(result.trace, /probe helper output \(1\)/)
      assert.doesNotMatch(result.trace, /continued|OC2InstallDirectoryValidationFailed/)
      assert.equal(result.trace.includes('message:'), !silent)
      assert.doesNotMatch(result.invocations, /--oc2-migrate-update/)
    })
    for (const failure of [{ status: '2' }, { invalid: true }, { status: '-1073741502' }, { status: 'timeout' }]) {
      await t.test(`unavailable validator ${JSON.stringify(failure)}: silent=${silent}`, () => {
        const result = probe({ ...failure, silent, update: true })
        assert.equal(result.exit, 0)
        assert.match(result.trace, /continued/)
        assert.match(result.trace, /registers:saved-zero\|saved-one\|saved-two/)
        assert.match(result.trace, /detail:OC2InstallDirectoryValidationFailed:/)
        assert.equal(result.trace.includes('message:'), !silent)
        assert.doesNotMatch(result.trace, /aborted|inside the BepInEx|plan:.+migration\.json/)
        assert.doesNotMatch(result.invocations, /--oc2-migrate-update/)
        if (failure.invalid) assert.match(result.trace, /status=error/)
        else {
          assert.match(result.trace, new RegExp(`status=${failure.status}`))
          assert.match(result.trace, /probe helper output/)
        }
      })
    }
  }
  await t.test('only a forbidden update can prepare and retain a relocation plan', () => {
    const result = probe({ status: '1', update: true })
    assert.equal(result.exit, 0)
    assert.match(result.trace, /continued/)
    assert.match(result.trace, /plan:.+migration\.json/)
    assert.match(result.trace, /status=1/)
    assert.match(result.invocations, /--oc2-migrate-update\|prepare\|/)
    assert.doesNotMatch(result.trace, /OC2InstallDirectoryValidationFailed|message:|aborted/)
  })
  await t.test('a failed relocation remains blocked even when its helper returns an unavailable status', () => {
    const result = probe({ status: '1', update: true, migrationStatus: '2' })
    assert.equal(result.exit, 1)
    assert.match(result.trace, /status=1/)
    assert.match(result.trace, /status=2/)
    assert.match(result.trace, /probe helper output \(2\)/)
    assert.match(result.trace, /aborted/)
    assert.doesNotMatch(result.trace, /continued|OC2InstallDirectoryValidationFailed/)
  })
  await t.test('an existing relocation plan cannot be prepared twice', () => {
    const result = probe({ status: '1', update: true, existingPlan: 'already-prepared.json' })
    assert.equal(result.exit, 1)
    assert.doesNotMatch(result.invocations, /--oc2-migrate-update/)
    assert.match(result.trace, /aborted/)
  })
})
