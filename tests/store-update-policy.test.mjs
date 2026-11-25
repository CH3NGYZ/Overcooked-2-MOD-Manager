import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const source = path => readFileSync(new URL(`../src-tauri/src/${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const commands = source('commands/update.rs')
const distribution = source('app_update/distribution.rs')

test('About reads the native version at startup while update initialization waits for first-run language confirmation', () => {
  assert.match(commands, /#\[tauri::command\]\s*pub fn get_app_distribution\(\) -> Result<AppDistribution, String> \{\s*distribution::current\(\)\s*\}/)
  const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
  const onboarding = app.slice(app.indexOf('async function startOnboarding()'), app.indexOf('function finishLanguageSelection()'))
  assert.match(onboarding, /if \(desktop\) initializeAppUpdates\(\)/)
  const mount = app.slice(app.indexOf('onMounted(() => {'), app.indexOf('</script>'))
  assert.match(mount, /if \(isTauri\(\)\) getVersion\(\)\.then\([^\n]*\n\s*if \(!choosingLanguage\.value\) void startOnboarding\(\)/)
  assert.doesNotMatch(mount, /initializeAppUpdates\(\)|checkForAppUpdate\(/)
  assert.match(app, /function finishLanguageSelection\(\) \{\s*choosingLanguage\.value = false\s*void startOnboarding\(\)\s*\}/)
  assert.match(app, /<StartupLanguageDialog v-if="choosingLanguage" @confirmed="finishLanguageSelection" \/>/)
  const service = readFileSync(new URL('../src/composables/useAppUpdates.ts', import.meta.url), 'utf8')
  const startup = service.slice(service.indexOf('export function initializeAppUpdates()'))
  assert.match(startup, /distribution \?\?= api\.getAppDistribution\(\)\.then\(setAppDistribution\)[^]*?void checkForAppUpdate\(false\)/)
  assert.match(service, /await distribution\s*if \(appUpdateState\.distribution === 'microsoft_store'\)/)
})

test('Store checks and installs stop before standalone cache, network or update locks', () => {
  const check = commands.slice(commands.indexOf('pub async fn check_app_update('), commands.indexOf('fn failed_app_update_check('))
  assert.match(check, /distribution::current\(\)\? == AppDistribution::MicrosoftStore[^]*?return Ok\(distribution::store_check\(region.as_str\(\)\)\);/)
  const earlyReturn = check.indexOf('return Ok(distribution::store_check(')
  for (const action of ['app_update_cache_get(', 'run_update_check(', 'discover_installer_update(']) {
    assert.ok(check.indexOf(action) > earlyReturn, `${action} must follow the Store return`)
  }
  const install = commands.slice(commands.indexOf('pub async fn stage_app_update('))
  const gate = install.indexOf('UpdateTarget::current()?')
  assert.ok(gate >= 0)
  assert.ok(install.indexOf('download_confirmed_update(') > gate, 'download must follow the distribution gate')
})

test('packaged or unknown runtime cannot expose updater plugin IPC or create installer targets', () => {
  const lib = source('lib.rs')
  const registration = lib.indexOf('match app_update::distribution::current()')
  assert.ok(lib.indexOf('.plugin(tauri_plugin_single_instance::init(') < registration, 'single-instance must register first')
  assert.match(lib, /match app_update::distribution::current\(\) \{\s*Ok\(models::AppDistribution::Standalone\) => \{\s*builder = builder.plugin\(\s*tauri_plugin_updater::Builder::new\(\)[^]*?Ok\(models::AppDistribution::MicrosoftStore\) => \{\}\s*Err\(error\) => eprintln!/)
  assert.equal((lib.match(/tauri_plugin_updater::Builder::new\(\)/g) ?? []).length, 1)
  assert.match(source('app_update.rs'), /pub\(crate\) fn current\(\) -> Result<Self, String> \{\s*distribution::require_standalone\(\)\?;/)
  assert.match(distribution, /fn GetCurrentPackageFamilyName\(length: \*mut u32, name: \*mut u16\) -> i32;/)
  assert.doesNotMatch(distribution, /WindowsApps|current_exe\(/)
})

test('Store update action uses the system Store page instead of an executable or Release', () => {
  assert.match(distribution, /STORE_UPDATES_URI: &str = "ms-windows-store:\/\/downloadsandupdates";/)
  const open = commands.slice(commands.indexOf('pub fn open_update_repository('), commands.indexOf('pub async fn check_app_update('))
  assert.match(open, /distribution::current\(\)\? == AppDistribution::MicrosoftStore[^]*?shell_open::open_url\(distribution::STORE_UPDATES_URI\)\?;[^]*?return Ok\(OperationResult \{[^]*?message: "update.microsoftStoreOpened".into\(\),/)
  assert.ok(open.indexOf('distribution::STORE_UPDATES_URI') < open.indexOf('installer_region('))
})
