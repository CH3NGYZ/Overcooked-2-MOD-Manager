import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 安装详情弹窗的两级卡片接线（`src/components/InstallDetailDialog.vue`）。
 *
 * 这些不是纯逻辑，无法在 Node 里渲染组件；但每一条都是「改回去就会悄悄退化」的接法，
 * 所以按源码文本守住 —— 一旦有人把随包插件换回手写卡片、把展开滚动摘掉，
 * 这里会直接变红，而不是等爸爸在真机上发现错位。
 */
const dialog = readFileSync(new URL('../src/components/InstallDetailDialog.vue', import.meta.url), 'utf8')
const modal = readFileSync(new URL('../src/components/AppModal.vue', import.meta.url), 'utf8')

test('随包插件复用管理页的 LocalItemCard，而不是手写一份卡片', () => {
  assert.match(dialog, /import LocalItemCard from '\.\/LocalItemCard\.vue'/)
  // 无开关：随包 DLL 跟着整张图启停，单独开关会造出第二套状态
  assert.match(dialog, /<LocalItemCard[\s\S]{0,200}?switchable="false"/)
  // 标题后的版本号与灰色角色标签，与管理页同一套版式
  assert.match(dialog, /<LocalItemCard[\s\S]{0,200}?:version="child\.meta"/)
  assert.match(dialog, /<LocalItemCard[\s\S]{0,200}?:titleTag="child\.tag"/)
})

test('展开状态走共用的 useCardExpansion：外层收起连带清内层', () => {
  assert.match(dialog, /import \{ useCardExpansion \} from '\.\.\/composables\/useCardExpansion'/)
  assert.match(dialog, /expansion\.closeOuter\(/)
  assert.match(dialog, /expansion\.openInner\(/)
  assert.match(dialog, /expansion\.closeInner\(/)
})

test('展开 / 收起都接上管理页那套滚动策略', () => {
  assert.match(dialog, /scrollExpandedCardIntoView\(\{/)
  assert.match(dialog, /collapseCardWithScroll\(\{ card, body:/)
})

test('内容层在展开之后才查：折叠时 n-collapse-transition 不渲染子节点', () => {
  // 点开的那一刻 `.detail-body` 还不存在，必须先改状态、等一帧再取
  assert.match(dialog, /expansion\.openOuter\(key\)\n[\s\S]{0,400}?void nextTick\(\(\) => \{\n\s+scrollExpandedCardIntoView/)
  assert.match(dialog, /expansion\.openInner\(parentKey, childKey\)\n[\s\S]{0,300}?void nextTick\(/)
})

test('内容层单独包一层，供滚动策略量自然高度', () => {
  assert.match(dialog, /<div class="detail-body" @click\.stop>/)
  assert.match(dialog, /return card\.querySelector\('\.detail-body'\)/)
})

test('整条 flex 链不收缩：内容超高只让弹窗内容区滚', () => {
  assert.match(dialog, /\.install-detail \{[\s\S]{0,400}?flex: 0 0 auto;/)
  assert.match(dialog, /\.detail-list \{[\s\S]{0,200}?flex: 0 0 auto;/)
})

test('弹窗内容区关掉滚动锚定，滚动只由 expandScroll 一套执行', () => {
  // 窗口放宽到 1200：这条规则块里还写着 scrollbar-gutter 等注释，限度太紧会误红
  assert.match(modal, /\.app-modal-scroll \{[\s\S]{0,1200}?overflow-anchor: none;/)
})
