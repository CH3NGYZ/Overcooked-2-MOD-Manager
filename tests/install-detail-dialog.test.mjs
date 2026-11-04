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
// Windows 上 actions/checkout 的 core.autocrlf 会把工作区换成 CRLF，而下面的断言
// 都按 `\n` 写（要匹配「这行之后紧跟那行」），所以读入时统一成 LF 再比对。
const readSource = file => readFileSync(new URL(file, import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const dialog = readSource('../src/components/InstallDetailDialog.vue')
const modal = readSource('../src/components/AppModal.vue')

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

test('内容层不与卡片点击抢事件：字段区可收起，子卡与依赖表各自 stop', () => {
  // `.detail-body` 整块不许 `@click.stop`：展开出来的字段区就是这张卡的正文，
  // 点它应当等于点卡片（收起）。整块吞掉点击，展开后就没有收回来的地方 ——
  // 那时卡片底部往往已经滚出视口，用户只能再去找上面那行标题。
  assert.match(dialog, /<div class="detail-body">/)
  assert.doesNotMatch(dialog, /class="detail-body"[^>]*@click\.stop/)
  // 真正自己处理点击的两块各自拦一下，不让子卡展开 / 依赖跳转连带收起外层
  assert.match(dialog, /class="detail-plugins" @click\.stop/)
  assert.match(dialog, /class="detail-deps" @click\.stop/)
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

test('两处展开都显式声明弹窗滚动容器与顶部边界', () => {
  // 内容还没溢出时 `.app-modal-scroll` 不算「可滚动祖先」，启发式找不到它；
  // 弹窗也没有固定页头，顶部边界就是内容区自己的顶（0）。两处不传就会滚不到位。
  const calls = [...dialog.matchAll(/scrollExpandedCardIntoView\(\{[\s\S]{0,400}?\n\s*\}\)/g)]
  assert.equal(calls.length, 2, '展开滚动应只有外层条目与内层插件两处')
  for (const [call] of calls) {
    assert.match(call, /container: card\.closest<HTMLElement>\('\[data-scroll-container\]'\)/)
    assert.match(call, /bounds: \{ top: 0 \}/)
  }
})
