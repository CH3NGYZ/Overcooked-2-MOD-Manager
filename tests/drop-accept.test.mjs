import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 整窗拖放的接收规则与归属。
 *
 * 真实缺口：拖放监听原先挂在来源卡片的 `onMounted` 上，于是「这页能不能拖」
 * 取决于「这页碰巧挂没挂那张卡片」—— MOD 安装页与地图下载页能拖，地图管理页
 * 连安装入口都没有，拖进去毫无反应。修法是把拖放从卡片里搬出来，变成窗口能力。
 *
 * 所以这里锁两件事：
 *   1. 扩展名判定（真跑 `src/dropAccept.ts`）：拖入与「点击选择文件」共用它，
 *      两个入口不会对同一个文件给出相反结论；
 *   2. 归属：四个 MOD / 地图页面各自声明 `useFileDrop`，来源卡片一行拖放事件
 *      都不认识 —— 卡片再也不能决定「哪页能拖」。
 */
const { acceptLabel, matchesFileAccept } = await import('../src/dropAccept.ts')

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8').replace(/\r\n/g, '\n')

const fileDrop = read('src/composables/useFileDrop.ts')
const sourceCard = read('src/components/InstallSourceCard.vue')
const mapsOnline = read('src/views/MapsOnlineView.vue')
const modsOnline = read('src/views/ModsOnlineView.vue')
const modsLocal = read('src/views/ModsLocalView.vue')
const levels = read('src/views/LevelsView.vue')
const dropRouting = read('src/composables/useDropRouting.ts')
const modInstallFlow = read('src/composables/useModInstallFlow.ts')
const planDialog = read('src/modChanges/ChangePlanDialog.vue')
const planDetails = read('src/modChanges/ChangePlanDetails.vue')
const planPresentation = read('src/modChanges/presentation.ts')
const importFlowSource = read('src/composables/useLevelSetImport.ts')
const tauriService = read('src/services/tauri.ts')
const rustLevelPaths = read('src-tauri/src/level_sets/paths.rs')
const rustLevelZip = read('src-tauri/src/level_sets/zip_inspect.rs')
const rustLevelPlugins = read('src-tauri/src/level_sets/plugin_assignment.rs')
const rustLevelModels = read('src-tauri/src/models/levels.rs')
const rustCommands = read('src-tauri/src/commands/drop_source.rs')
const rustLib = read('src-tauri/src/lib.rs')

test('扩展名判定：带点、不分大小写、不含点不认', () => {
  assert.equal(matchesFileAccept('C:\\Users\\a\\Downloads\\mod.zip', ['zip']), true)
  assert.equal(matchesFileAccept('C:\\Users\\a\\Downloads\\MOD.ZIP', ['zip']), true)
  assert.equal(matchesFileAccept('C:\\Users\\a\\Downloads\\plugin.dll', ['dll', 'zip']), true)
  // 没有点的不算扩展名：`zip` 结尾的文件名不是 ZIP
  assert.equal(matchesFileAccept('C:\\Users\\a\\Downloads\\modzip', ['zip']), false)
  // 只认结尾：后缀再跟东西就不是这种文件了
  assert.equal(matchesFileAccept('C:\\Users\\a\\Downloads\\mod.zip.txt', ['zip']), false)
  assert.equal(matchesFileAccept('C:\\Users\\a\\Downloads\\mod.zipx', ['zip']), false)
  // 白名单里没有的扩展名，两个入口都必须拒绝
  assert.equal(matchesFileAccept('C:\\Users\\a\\Downloads\\mod.dll', ['zip']), false)
  assert.equal(matchesFileAccept('/Users/a/Downloads/map.tar.gz', ['zip']), false)
})

test('扩展名判定：中文、空格与多个点的路径照样认', () => {
  assert.equal(matchesFileAccept('D:\\我的地图\\无前置地图合集.v1.2.zip', ['zip']), true)
  assert.equal(matchesFileAccept('/Users/a/Library/Mobile Documents/map final.ZIP', ['zip']), true)
  assert.equal(matchesFileAccept('', ['zip']), false)
})

test('空白名单一律不接收，不靠调用方自己判空', () => {
  assert.equal(matchesFileAccept('C:\\a.zip', []), false)
  assert.equal(acceptLabel([]), '')
})

test('类型文案：点前缀 + 语言无关的连接符', () => {
  assert.equal(acceptLabel(['zip']), '.zip')
  assert.equal(acceptLabel(['dll', 'zip']), '.dll / .zip')
})

test('监听只注册一次，且不在来源卡片里', () => {
  // 一次拖入触发两次安装，就是「监听注册了两份」
  assert.equal(
    fileDrop.match(/\.onDragDropEvent\(/g).length,
    1,
    '拖放监听只能有一个注册点',
  )
  assert.match(fileDrop, /let listener: Promise<void> \| null = null/, '并发首次调用必须用 promise 占位')
  assert.doesNotMatch(sourceCard, /onDragDropEvent|getCurrentWebview/, '来源卡片不该认识 Tauri 拖放事件')
  // 卡片只保留高亮属性，接收与处理都在页面
  assert.match(sourceCard, /dragActive\?: boolean/)
})

test('全局更新门中断的释放事件在安装 IPC 前留下原因', () => {
  assert.match(fileDrop, /if \(appUpdateBlocksOperations\(\)\) \{\s*if \(payload\.type === 'drop'\) recordFileDrop\('update_gate', appUpdateState\.stage, payload\.paths\)\s*dragActive\.value = false\s*return\s*\}/)
})

test('四个 MOD / 地图页面各自声明拖放，扩展名与来源卡片一致', () => {
  for (const [name, source, accept] of [
    ['ModsOnlineView', modsOnline, "accept: ['dll', 'zip']"],
    ['ModsLocalView', modsLocal, "accept: ['dll', 'zip']"],
    ['MapsOnlineView', mapsOnline, "accept: ['zip', 'dll']"],
    ['LevelsView', levels, "accept: ['zip', 'dll']"],
  ]) {
    assert.match(source, /useFileDrop\(\{/, `${name} 要在页面里注册拖放`)
    assert.match(source, new RegExp(accept.replace(/[[\]]/g, '\\$&')), `${name} 的接收类型要与卡片一致`)
    assert.match(source, /:drag-active="dragActive"/, `${name} 要把拖拽状态传给来源卡片`)
  }
})

test('地图 ZIP 的导入只有一份实现，两个地图页共用', () => {
  for (const [name, source] of [['MapsOnlineView', mapsOnline], ['LevelsView', levels]]) {
    assert.match(source, /useLevelSetImport\(/, `${name} 必须走共享导入实现`)
    assert.doesNotMatch(source, /importZipSets/, `${name} 不许再自己写确认 + 导入`)
    assert.doesNotMatch(source, /api\.importLevelSets\(/, `${name} 不许直接调导入命令`)
  }
  // 共享实现里两条规矩：暂存文件必须清、本地文件绝不删
  const importFlow = read('src/composables/useLevelSetImport.ts')
  assert.match(importFlow, /if \(stagedFile\) await api\.discardMapSource\(stagedFile\)\.catch\(/)
  // 提交成功由 App 统一更新，导入只负责确认、清理和提示。
  assert.doesNotMatch(importFlow, /afterImport|onImported|scanLevelSets|syncDependencySnapshots/)
  assert.match(importFlow, /message\.success\(successText\?\.\(\) \?\? t\('levelSet\.imported'/)
  assert.match(importFlow, /if \(!staged\.sets\.length\)[\s\S]+message\.warning\(t\('levelSet\.zipWithoutInfo'\)\)[\s\S]+return false/)
  assert.match(importFlow, /await changes\.run\(\{ roots: \[\], map_import: \{[\s\S]+selected: sets\.map\(set => set\.dir_name\)/)
  assert.match(importFlow, /\.\.\.\(staged\.download_id \? \{ download_id: staged\.download_id \} : \{\}\)/)
  assert.doesNotMatch(importFlow, /askApproval|resolveApproval|reviewSelection/)
  for (const [name, source] of [['MapsOnlineView', mapsOnline], ['LevelsView', levels]]) {
    assert.doesNotMatch(source, /InstallDetailDialog/, `${name} 不再要求单独确认地图详情`)
  }
  assert.match(modInstallFlow, /changes\.run\(\{ roots:/, 'MOD 拖放预览进入统一整链方案')
  assert.match(planDialog, /import ChangePlanDetails from '\.\/ChangePlanDetails\.vue'/, '统一弹窗必须接入变更表格')
  assert.match(planDialog, /<ChangePlanDetails\b[^>]*:plan="plan"[^>]*:progress="currentProgress"/, '完整方案与准备进度分别传入详情')
  assert.doesNotMatch(modInstallFlow, /confirmModInstall|uninstallLocalMod/, '拖放不能绕过批次执行')
})

/**
 * 跨类型识别：把地图包拖到 MOD 页、把纯 DLL 拖到地图页，都要在**安装之前**
 * 拦下来并引导到对面页面 —— 否则用户看到的是一份空预览（0 个 DLL）或者
 * 一个「没有 info」的地图包，点「安装」还会成功（喷彩花），实际什么都没装。
 */
test('MOD 和地图的拖放与文件选择都进入共享的分类入口', () => {
  for (const [name, source] of [['ModsOnlineView', modsOnline], ['ModsLocalView', modsLocal]]) {
    assert.match(source, /@select="handleInstallSourceSelect"/, `${name} 的文件选择必须走共享入口`)
    assert.match(source, /onDrop:[^\n]+handleInstallSourceSelect\(\{ kind: 'local', value: path \}, 'drop'\)/)
    assert.match(source, /takePendingDrop\('mod'\)[\s\S]+handleInstallSourceSelect\(\{ kind: 'local', value: pending \}, 'routed'\)/)
  }
  assert.match(modInstallFlow, /source\.kind === 'local' && entry !== 'routed' && !\(await acceptDrop\(source\.value, 'mod'\)\)[\s\S]+await api\.analyzeModSource/)
  for (const [name, source] of [['MapsOnlineView', mapsOnline], ['LevelsView', levels]]) {
    assert.match(source, /@select="importSource"/, `${name} 的文件选择必须走共享入口`)
    assert.match(source, /onDrop:[^\n]+importSource\(\{ kind: 'local', value: path \}, 'drop'\)/)
    assert.match(source, /takePendingDrop\('map'\)[\s\S]+importSource\(\{ kind: 'local', value: pending \}, 'routed'\)/)
  }
  assert.match(importFlowSource, /source\.kind === 'local' && entry !== 'routed' && !\(await acceptDrop\(source\.value, 'map'\)\)[\s\S]+await api\.stageMapSource/)
})

test('跨页路由只有一份实现，且只在设备端分类一次', () => {
  assert.match(dropRouting, /api\.classifyDropSource\(path\)/, '分类要调后端命令')
  assert.match(dropRouting, /export function takePendingDrop/, '跳转后的路径要能从模块里取出来')
  assert.match(dropRouting, /router\.push\(target === 'map'/, '「去安装」跳转到安装页')
  // 四个页面各写一份弹窗就会各自漂移：分类与弹窗都只认这一个模块
  for (const [name, source] of [
    ['ModsOnlineView', modsOnline],
    ['ModsLocalView', modsLocal],
    ['MapsOnlineView', mapsOnline],
    ['LevelsView', levels],
  ]) {
    assert.doesNotMatch(source, /classifyDropSource/, `${name} 不许自己调分类命令`)
    assert.doesNotMatch(source, /mapPackageDetected|modPackageDetected/, `${name} 不许自己写跨类型文案`)
  }
  assert.equal(tauriService.match(/classifyDropSource:/g).length, 1, 'IPC 包装只此一处')
})

test('分类命令在后端注册，地图与独立 MOD 混合包分流', () => {
  assert.match(rustLib, /commands::drop_source::classify_drop_source/, 'lib.rs 必须注册这个命令')
  assert.match(rustCommands, /pub async fn classify_drop_source/, '命令必须是 async（要读 ZIP）')
  assert.match(rustCommands, /let kind = if contents\.map_sets > 0 && !contents\.mod_bundled_maps/, '纯地图和地图随包插件走地图入口')
  assert.match(rustCommands, /else if contents\.plugins > 0/, '独立 MOD 及随该 MOD 打包的地图走 MOD 入口')
  assert.match(rustLevelZip, /pub fn classify_zip/)
})

/**
 * 停用根：BepInEx 5 会递归扫描 `BepInEx/plugins/` 下每一个 DLL，停用地图里
 * 作者随包的修复插件只要还留在 plugins 树里，下次启动照样被加载 ——「停用地图」
 * 就只停了地图。停用根必须在 plugins 之外。
 */
test('停用根在 plugins 之外，且旧位置会一次性迁移', () => {
  assert.match(rustLevelPaths, /const DISABLED_ROOT: &str = "BepInEx\/cache\/modmanager_disabled_maps"/)
  assert.doesNotMatch(rustLevelPaths, /const DISABLED_ROOT: &str = "BepInEx\/plugins/, '停用根不能落在 plugins 里')
  assert.match(rustLevelPaths, /pub fn migrate_legacy_disabled_root/, '老用户的停用地图要搬过去')
  assert.match(rustLevelPaths, /BepInEx\/plugins\/OC2DIYLevel\/disabled/, '旧位置只用于迁移')
})

/**
 * 随包插件：地图包里 `levels包/<集>/` 与 `plugins包/<dll>/` 是同一个作者包的两半，
 * DLL 要落进对应关卡集目录，这样「停用地图」搬走整个文件夹时插件一起走。
 */
test('地图包里的修复插件跟着关卡集目录走', () => {
  assert.match(rustLevelModels, /pub plugins: Vec<ZipLevelSetPlugin>/, '导入结果要带出插件清单')
  assert.match(rustLevelPlugins, /fn assign_plugins/, '插件归集只此一处')
  assert.match(planPresentation, /if \(node\?\.kind === 'map' \|\| node\?\.presentation_kind === 'mod_package'\)[\s\S]+bundled: true/)
  assert.match(planDetails, /:data-change-parent="row\.bundled \|\| row\.retired \? row\.nodeId : undefined"/)
})
