import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const source = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const installer = source('src-tauri/nsis/installer.nsi')
const guard = installer.split('Function OC2ValidateInstallDirectory\n')[1].split('FunctionEnd')[0]

test('only an explicit unsafe-directory verdict can trigger automatic migration', () => {
  assert.match(guard, /\$\{If\} \$0 == 1\n\s*\$\{AndIf\} \$UpdateMode = 1\n\s*\$\{AndIf\} \$OC2MigrationPlan == ""/)
  assert.doesNotMatch(guard, /\$\{If\} \$0 != 0\n\s*\$\{AndIf\} \$UpdateMode = 1/)
  assert.match(guard, /StrCpy \$2 "\$\(OC2InstallDirectoryValidationFailed\)"\n\s*\$\{If\} \$0 == 1\n\s*StrCpy \$2 "\$\(OC2InvalidInstallDirectory\)"/)
  assert.match(guard, /\$\{Else\}\n\s*StrCpy \$2 "\$\(OC2MigrationFailed\)"/)
})

test('failed checks remain closed and retain the raw helper status and output', () => {
  assert.match(guard, /nsExec::ExecToStack \/TIMEOUT=10000[^\n]+\n\s*Pop \$0\n\s*Pop \$1/)
  assert.match(guard, /\$\{If\} \$0 != 0\n\s*DetailPrint "Install directory validation: directory=\$INSTDIR; status=\$0"\n\s*DetailPrint "\$1"/)
  assert.match(guard, /MessageBox MB_OK\|MB_ICONSTOP "\$2" \/SD IDOK\n\s*SetErrorLevel 1\n\s*Pop \$2\n\s*Pop \$1\n\s*Pop \$0\n\s*Abort/)
  assert.match(guard, /SetErrorLevel 0\n\s*Pop \$2\n\s*Pop \$1\n\s*Pop \$0/)
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
      }
    }
  }
})
