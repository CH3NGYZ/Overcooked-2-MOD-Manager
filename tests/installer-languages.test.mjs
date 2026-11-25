import assert from 'node:assert/strict'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const source = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const { nsis } = JSON.parse(source('src-tauri/tauri.conf.json')).bundle.windows
const installer = source('src-tauri/nsis/installer.nsi')
const hooks = source('src-tauri/installer-hooks.nsh')
const languages = [
  ['English', 1033, 'en'], ['German', 1031, 'de'], ['French', 1036, 'fr'], ['Italian', 1040, 'it'],
  ['Korean', 1042, 'ko'], ['Spanish', 1034, 'es-ES'], ['SimpChinese', 2052, 'zh-CN'],
  ['TradChinese', 1028, 'zh-TW'], ['Russian', 1049, 'ru'], ['Japanese', 1041, 'ja'],
  ['Polish', 1045, 'pl'], ['PortugueseBR', 1046, 'pt-BR'],
]

test('the interactive NSIS selector exposes all 12 languages, independent of the Windows ANSI code page', () => {
  assert.equal(nsis.displayLanguageSelector, true)
  assert.deepEqual(nsis.languages, languages.map(([label]) => label))
  assert.match(installer, /^!define MUI_LANGDLL_ALLLANGUAGES$/m)
  assert.match(installer, /\{\{#each languages\}\}\s+!insertmacro MUI_LANGUAGE "\{\{this\}\}"\s+\{\{\/each\}\}/)
  assert.match(installer, /!insertmacro MUI_LANGDLL_DISPLAY/)
})

const makensis = join(process.env.LOCALAPPDATA ?? '', 'tauri', 'NSIS', 'makensis.exe')
const probeAvailable = process.platform === 'win32' && existsSync(makensis)

test('each NSIS language hands the manager the expected fresh-install locale without rewriting it during deferred update', {
  skip: probeAvailable ? false : 'requires Windows NSIS',
}, async t => {
  const temporary = fileURLToPath(new URL('../.tmp/locale12/', import.meta.url))
  mkdirSync(temporary, { recursive: true })
  const directory = mkdtempSync(join(temporary, 'installer-language-'))
  const executable = join(directory, 'language-probe.exe')
  const script = join(directory, 'language-probe.nsi')
  const hooksPath = fileURLToPath(new URL('../src-tauri/installer-hooks.nsh', import.meta.url))
  const postInstall = hooks.match(/^!macro NSIS_HOOK_POSTINSTALL\n([\s\S]*?)\n!macroend$/m)?.[1]
  assert.ok(postInstall?.includes('$DOCUMENTS\\OC2ModManager'))
  const isolated = postInstall.replaceAll('$DOCUMENTS\\OC2ModManager', '$EXEDIR\\probe-data')
  assert.doesNotMatch(isolated, /\$(?:DOCUMENTS|INSTDIR|APPDATA|LOCALAPPDATA)|\b(?:WriteReg\w*|Exec\w*)\b/i)
  const quote = text => text.replaceAll('$', '$$').replaceAll('"', '$\\"')
  writeFileSync(script, `Unicode true
RequestExecutionLevel user
OutFile "${quote(executable)}"
!include "${quote(hooksPath)}"
${languages.map(([label]) => `LoadLanguageFile "\${NSISDIR}\\Contrib\\Language files\\${label}.nlf"`).join('\n')}
Function .onInit
  ReadEnvStr $LANGUAGE OC2_PROBE_LANGUAGE
  ReadEnvStr $OC2DeferredUpdate OC2_PROBE_DEFERRED
FunctionEnd
Section
${isolated}
  SetErrorLevel 0
SectionEnd
`, 'utf8')
  const run = (executable, args, env = {}) => {
    const processResult = spawnSync(executable, args, {
      cwd: directory, env: { ...process.env, TEMP: directory, TMP: directory, ...env },
      encoding: 'utf8', timeout: 10000, windowsHide: true,
    })
    assert.ifError(processResult.error)
    assert.equal(processResult.status, 0, processResult.stdout + processResult.stderr)
  }
  run(makensis, ['/V2', '/INPUTCHARSET', 'UTF8', script])
  const marker = join(directory, 'probe-data', 'install-language.txt')
  for (const [name, langId, tag] of languages) {
    await t.test(`${name} → ${tag}`, () => {
      run(executable, ['/S'], { OC2_PROBE_LANGUAGE: `${langId}`, OC2_PROBE_DEFERRED: '0' })
      assert.equal(readFileSync(marker, 'utf8'), tag)
    })
  }
  run(executable, ['/S'], { OC2_PROBE_LANGUAGE: '1033', OC2_PROBE_DEFERRED: '1' })
  assert.equal(readFileSync(marker, 'utf8'), 'pt-BR')
  t.diagnostic(`Isolated installer-language markers: ${directory}`)
})
