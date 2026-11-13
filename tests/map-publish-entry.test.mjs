import assert from 'node:assert/strict'
import test from 'node:test'
import { existsSync, readFileSync } from 'node:fs'
import { parse } from '@vue/compiler-sfc'
import { baseParse } from '@vue/compiler-dom'
import { readPublishProvider, savePublishProvider } from '../src/mapPublishPreferences.ts'

const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8')

test('工具页在支持开发下方显示独立地图发布分组，并直接进入发布路由',()=>{
  const {descriptor} = parse(read('src/views/ToolsView.vue'))
  const groups = baseParse(descriptor.template.content).children.filter(node=>node.type === 1 && node.tag === 'SettingsGroup')
  const title = node=>node.props.find(prop=>prop.type === 7 && prop.name === 'bind' && prop.arg?.content === 'title')?.exp?.content
  const index = groups.findIndex(node=>title(node) === "t('mapPublish.title')")
  assert.ok(index > 0)
  assert.equal(title(groups[index-1]),"t('tools.supportDevelopment')")
  assert.equal(groups[index].props.some(prop=>prop.type === 7 && ['if','show'].includes(prop.name)),false)
  const entry = groups[index].children.find(node=>node.type === 1 && node.tag === 'SettingsRow')
  assert.ok(entry)
  assert.equal(title(entry),"t('mapPublish.publishEntry')")
  const click = entry.props.find(prop=>prop.type === 7 && prop.name === 'on' && prop.arg?.content === 'click')
  assert.equal(click?.exp?.content,"router.push('/map-publish')")
})

test('版本号保持展示，标题栏和App不再注册秘密入口',()=>{
  const bar = read('src/components/AppTitleBar.vue')
  const version = bar.match(/<span\b[^>]*class="brand-version"[^>]*>[^<]*<\/span>/)?.[0]
  assert.ok(version)
  assert.match(version,/v\{\{ props\.version \}\}/)
  assert.doesNotMatch(version,/@click|@mousedown/)
  for (const path of ['src/App.vue','src/components/AppTitleBar.vue','src/views/ToolsView.vue']) {
    assert.doesNotMatch(read(path),/secretEntry|secret-entry|onVersionTap|MAP_PUBLISH_ENTRY_ENABLED/,path)
  }
  assert.equal(existsSync(new URL('../src/secretEntry.ts',import.meta.url)),false)
})

test('渠道恢复只接受两个支持平台，无值与非法旧偏好回到GitHub',()=>{
  for (const value of ['github','gitcode']) assert.equal(readPublishProvider({getItem:()=>value}),value)
  for (const value of [null,'','gitee','GitCode',' gitcode ','null']) assert.equal(readPublishProvider({getItem:()=>value}),'github')
  assert.equal(readPublishProvider(null),'github')
})

test('持久存储拒绝访问时使用默认渠道且不阻断保存',()=>{
  const unavailable = {getItem:()=>{throw Error('denied')},setItem:()=>{throw Error('quota')}}
  assert.equal(readPublishProvider(unavailable),'github')
  assert.doesNotThrow(()=>savePublishProvider('gitcode',unavailable))
  assert.doesNotThrow(()=>savePublishProvider('github',null))
})
