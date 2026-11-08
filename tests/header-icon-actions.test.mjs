import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync, readdirSync } from 'node:fs'
const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const pages = ['ModsOnlineView.vue', 'ModsLocalView.vue', 'MapsOnlineView.vue', 'LevelsView.vue']
const header = source => source.slice(source.indexOf('<Teleport to="#page-header-actions"'), source.indexOf('</Teleport>', source.indexOf('<Teleport to="#page-header-actions"')))

test('四页只在页头操作区接入图标按钮，原动作与提示名称保留', () => {
  const expected = [
    ['ModsOnlineView.vue', [['tools.installMod', 'openInstall']]],
    ['ModsLocalView.vue', [['overview.check', 'checkUpdates'], ['modsLocal.openFolder', 'openModsDirectory']]],
    ['MapsOnlineView.vue', [['levelSet.installMap', 'installSourceOpen = true']]],
    ['LevelsView.vue', [['overview.check', 'checkUpdates'], ['levelSet.organize', 'organizeLevelSets'], ['modsLocal.openFolder', 'openLevelsDirectory']]],
  ]
  for (const [page, actions] of expected) {
    const source = read('src/views/' + page), area = header(source)
    assert.equal((area.match(/<HeaderIconAction\b/g) ?? []).length, actions.length, page)
    assert.equal((source.match(/<HeaderIconAction\b/g) ?? []).length, actions.length, page)
    assert.doesNotMatch(area, /<n-button\b|:title=/)
    for (const [label, action] of actions) {
      assert.ok(area.includes(`:label="t('${label}')"`), label)
      assert.ok(area.includes(`@click="${action}"`), action)
    }
  }
  for (const page of readdirSync(new URL('../src/views/', import.meta.url))) {
    if (!pages.includes(page)) assert.doesNotMatch(read('src/views/' + page), /HeaderIconAction|HeaderBulkSwitch/, page)
  }
})

test('两管理页共用受控图标批量开关，不改变聚合状态、过滤门控和禁用确认', () => {
  for (const page of ['ModsLocalView.vue', 'LevelsView.vue']) {
    const source = read('src/views/' + page), area = header(source)
    assert.match(area, /<HeaderBulkSwitch v-if="visibility.showBulkSwitch\(/)
    assert.match(area, /:value="bulkSwitchValue" :disabled="bulkWorking" @change="onBulkSwitch"/)
    assert.doesNotMatch(area, /<n-tabs|<n-tab-pane/)
    assert.match(source, /else if \(value === 'disable'\) confirmDisableAll\(\)/)
    assert.match(source, /const bulkSwitchValue = computed/)
    assert.match(source, /bulkSwitchValue as bulkSwitchValueOf/)
    assert.match(source, /bulkSwitchValueOf\(enabled/)
  }
  const bulk = read('src/components/HeaderBulkSwitch.vue')
  assert.match(bulk, /CheckmarkCircleOutline, CloseCircleOutline/)
  assert.match(bulk, /<template #tab>/)
  assert.match(bulk, /:value="value" @update:value="select"/)
  assert.match(bulk, /'is-mixed': !value/)
  assert.match(bulk, /!props.disabled/)
  assert.match(bulk, /'aria-pressed': props.value === value/)
  assert.match(bulk, /event.key === 'Enter' \|\| event.key === ' '/)
})

test('四页搜索使用同一内容自适应输入，移除固定宽度与空清除占位', () => {
  for (const page of pages) {
    const source = read('src/views/' + page)
    assert.match(header(source), /<HeaderSearch v-model:value="search"/)
    assert.doesNotMatch(source, /\.header-search\s*\{|<n-input v-model:value="search"|SearchOutline/)
  }
  const search = read('src/components/HeaderSearch.vue')
  assert.match(search, /v-model:value="value" autosize :clearable="value.length > 0"/)
  assert.match(search, /'header-search--empty': !value/)
  assert.match(search, /max-width: 240px/)
  assert.match(search, /'aria-label': t\('dialogs.search'\)/)
  assert.match(search, /\.n-input__placeholder\) \{ position: static;/)
  assert.match(search, /\.n-input__input-mirror\) \{ display: none;/)
})

test('页头图标有可访问名称，上方气泡复用原文字且保留按钮属性与忙碌态', () => {
  const button = read('src/components/HeaderIconAction.vue')
  assert.match(button, /inheritAttrs: false/)
  assert.match(button, /<n-button v-bind="\$attrs" size="small" :aria-label="label"/)
  assert.match(button, /<template #icon><slot \/><\/template>/)
  assert.match(button, /@focusin="focused = true" @focusout="focused = false"/)
  for (const path of ['HeaderIconAction.vue', 'HeaderBulkSwitch.vue']) {
    const source = read('src/components/' + path)
    assert.match(source, /<n-tooltip placement="top"/)
    assert.match(source, /:keep-alive-on-hover="false"/)
    assert.doesNotMatch(source, /\btitle=/)
  }
  const local = header(read('src/views/ModsLocalView.vue'))
  assert.match(local, /:loading="checkingBound" :disabled="checkingBound \|\| refreshingAll"/)
  for (const target of ['mods-local-bulk', 'mods-local-actions', 'mods-local-open-dir']) assert.ok(local.includes(`data-tutorial="${target}"`))
  const levels = header(read('src/views/LevelsView.vue'))
  assert.match(levels, /:loading="organizing" :disabled="organizing \|\| loading"/)
  assert.match(levels, /:loading="checkingBound" :disabled="checkingBound \|\| loading"/)
  const css = read('src/styles/global.css')
  assert.match(css, /\.page-header-actions \.bulk-switch \{[^}]*width: 68px;/)
})
