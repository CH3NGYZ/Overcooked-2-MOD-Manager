<script setup lang="ts">
/**
 * 安装前检查弹窗：把依赖 / 冲突 / 兼容三件事一次列清并执行。
 *
 * 为什么要有这一步：这三件事的解法互相影响。
 *   · 冲突要先腾地方（或换版本），否则装上也白装；
 *   · 兼容关系与依赖版本要换成「区间内」的那个版本；
 *   · 缺的东西最后补，此时其它关系已经理顺。
 * 分开做的话，用户会在三个不同的角落被提醒三次，而且中间状态
 * （装了一半、冲突还在）最难解释。
 *
 * 本组件只负责「展示计划 + 按顺序执行 + 报告结果」，计划的生成
 * （哪些能自动处理、哪些只能提醒）属于 `modRelations.buildInstallPlan`，
 * 因为那是纯判定，不该依赖弹窗。
 *
 * 执行顺序固定：卸载 → 升级/降级 → 装依赖。只有全部成功才放行主包、
 * 发 `ready: true`；任何一步失败都停在原地并保留已完成的部分 ——
 * 已卸载的 MOD 会进回收站，用户仍可恢复。
 */
import { computed, ref, watch } from 'vue'
import { NAlert, NButton, NSpin, NTag } from 'naive-ui'
import { t, pickI18n } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { tagColorFor } from '../theme'
import AppModal from './AppModal.vue'
import type { InstallPlan, RelationStatus } from '../modRelations'

const props = defineProps<{
  show: boolean
  plan: InstallPlan
  /** 主包名：用于「安装 X 前需要先处理这些」的文案。 */
  targetLabel: string
}>()

const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /**
   * 检查结果。
   *
   * `true`  = 需要处理的问题都已处理完，可以继续安装主包
   * `false` = 用户取消、或某一步失败、或存在无法自动处理的关系
   */
  (event: 'finished', ready: boolean): void
}>()

const message = useNotify()

/** 每一步的执行状态：按 `step-<index>` 索引。 */
const done = ref<Record<string, 'ok' | 'failed'>>({})
const failures = ref<{ label: string; reason: string }[]>([])
const running = ref(false)
/** 防重入：本组件自己 emit `update:show false`，模板又在那事件上调 close。 */
const closing = ref(false)

/**
 * 每次打开弹窗都清空上一次的执行状态。
 *
 * 这个组件实例是常驻的（`<InstallPlanDialog>` 一直挂在页面里，只有内层
 * n-modal 被销毁），所以 `done` / `failures` 会跨次残留。而步骤 key 是
 * **按索引**生成的（`install-0`、`install-1`），新计划的 `install-0` 会撞上
 * 上一次残留的「已完成」——表现就是刚打开「安装前检查」，依赖那一行已经
 * 打着 ✓、按钮直接是「完成」，点下去跳过依赖直接装主包，装完仍显示缺依赖。
 */
watch(() => props.show, (visible) => {
  if (!visible) return
  done.value = {}
  failures.value = []
  running.value = false
  closing.value = false
})

/** 扁平化的执行序列：卸载在前，安装在后。 */
interface Step {
  key: string
  kind: 'uninstall' | 'install'
  /** 展示名：卸载是本地 MOD 名，安装是目标仓库名。 */
  label: string
  /** 目标版本（安装步骤才有）。 */
  version?: string
  /** 关系种类，用于解释为什么做这一步。 */
  action: 'uninstall' | 'upgrade' | 'downgrade' | 'install-dependency'
  index: number
}

const steps = computed<Step[]>(() => {
  const list: Step[] = []
  props.plan.uninstall.forEach((status, index) => {
    list.push({
      key: `uninstall-${status.installedMod?.dll_file ?? status.relation.guid}`,
      kind: 'uninstall',
      label: status.installedMod?.name ?? status.relation.guid,
      action: 'uninstall',
      index,
    })
  })
  props.plan.install.forEach((status, index) => {
    const target = status.repository
    const version = status.release?.tag_name || status.release?.version || ''
    list.push({
      key: `install-${target?.id ?? status.relation.guid}-${version}`,
      kind: 'install',
      label: target ? pickI18n(target.name_i18n, target.name) : status.relation.guid,
      version,
      action: installActionOf(status),
      index,
    })
  })
  return list
})

/**
 * 这一步是「升级」「降级」还是「装依赖」。
 *
 * 依赖关系上没装过 → 装依赖；装过而新版本更高 → 升级，更低 → 降级。
 * 兼容关系同理：对方存在但不兼容，装的版本比本地高就是升级。
 */
function installActionOf(status: InstallPlan['install'][number]): Step['action'] {
  const kind = status.relation.kind ?? 'requires'
  if (kind === 'requires' && !status.installedMod) return 'install-dependency'
  const current = status.installedVersion
  const next = status.release?.tag_name || status.release?.version || ''
  if (!current) return 'install-dependency'
  const compared = compare(current, next)
  if (compared === null) return 'install-dependency'
  return compared > 0 ? 'downgrade' : 'upgrade'
}

/** 只做数值/后缀比较，够判断「升还是降」；解析不出时返回 null。 */
function compare(left: string, right: string): number | null {
  const parse = (value: string) => {
    const match = value.trim().replace(/^[vV]/, '').match(/^(\d+(?:\.\d+)*)(.*)$/)
    if (!match) return null
    return {
      numbers: match[1].split('.').map(Number),
      suffix: match[2],
    }
  }
  const a = parse(left)
  const b = parse(right)
  if (!a || !b) return null
  const length = Math.max(a.numbers.length, b.numbers.length)
  for (let index = 0; index < length; index += 1) {
    const delta = (a.numbers[index] ?? 0) - (b.numbers[index] ?? 0)
    if (delta !== 0) return delta
  }
  // 数字相同：没有后缀的正式版更高
  if (!a.suffix && !b.suffix) return 0
  if (!a.suffix) return 1
  if (!b.suffix) return -1
  return a.suffix.localeCompare(b.suffix)
}

/** 提醒类条目：无法自动处理，只能让用户知道。 */
const blocked = computed(() => props.plan.blocked)

/** 有可执行步骤，且都还没做。 */
const pending = computed(() => steps.value.filter((step) => !done.value[step.key]))
const allDone = computed(() => steps.value.length > 0 && pending.value.length === 0)
/** 全部成功：没有失败、没有提醒。 */
const allOk = computed(() =>
  allDone.value && failures.value.length === 0 && blocked.value.length === 0)

/** 按顺序执行全部步骤。 */
async function runAll() {
  if (running.value || !steps.value.length) return
  running.value = true
  failures.value = []
  try {
    for (const step of steps.value) {
      if (done.value[step.key]) continue
      try {
        if (step.kind === 'uninstall') {
          await runUninstall(step)
        } else {
          await runInstall(step)
        }
      } catch (reason) {
        markFailed(step, reason)
      }
    }
    report()
  } finally {
    running.value = false
  }
}

async function runUninstall(step: Step) {
  const status = props.plan.uninstall[step.index]
  const target = status.installedMod
  if (!target) {
    markFailed(step, new Error(t('modsOnline.theModToUninstallWasNotFound')))
    return
  }
  /*
   * 后端可能要求「整包删除」二次确认（该 DLL 属于多文件包）。
   * 这里直接带 `allow_unmanaged = true` 走整包删除 —— 用户已经在
   * 本弹窗里确认过「为了装这个 MOD 要卸载它」，再问一次只是重复。
   */
  const result = await api.uninstallLocalMod(target.dll_file, true)
  if (result.ok) done.value = { ...done.value, [step.key]: 'ok' }
  else markFailed(step, new Error(result.message))
}

async function runInstall(step: Step) {
  const status = props.plan.install[step.index]
  const target = status.repository
  if (!target) {
    markFailed(step, new Error(t('modsOnline.noInstallableSourceFound')))
    return
  }
  const tag = status.release?.tag_name
  // 先下载预览，再直接确认安装：暂存目录与用户确认已在上一层完成
  const outcome = await api.installRepository(target.id, tag, true)
  if (!outcome.ok || !outcome.preview) {
    markFailed(step, new Error(outcome.message || t('modsOnline.downloadFinishedButNoInstallableContentWasParsed')))
    return
  }
  const confirmed = await api.confirmModInstall(outcome.preview.preview_id)
  if (confirmed.ok) done.value = { ...done.value, [step.key]: 'ok' }
  else markFailed(step, new Error(confirmed.message))
}

function markFailed(step: Step, reason: unknown) {
  /*
   * 收口翻译：三处调用分别来自 `result.message`、`outcome.message` 和
   * `t(...)` 兜底，都是后端 tag 或已翻译文案。统一在这里过一遍桥接层，
   * 比在每个调用点各写一次 `translateBackendMessage` 更难漏。
   */
  const text = backendMessageOf(reason)
  done.value = { ...done.value, [step.key]: 'failed' }
  failures.value = [...failures.value, { label: step.label, reason: text }]
}

/** 汇总结果：成功时只提示一次，失败逐条点名。 */
function report() {
  const okCount = steps.value.filter((step) => done.value[step.key] === 'ok').length
  if (okCount) {
    message.success(t('modsOnline.handledItemS', { count: okCount }))
  }
  for (const failure of failures.value) {
    message.error(`${failure.label}：${failure.reason}`)
  }
}

/** 关闭：把「能否继续装主包」交给宿主。 */
async function close() {
  if (closing.value) return
  closing.value = true
  try {
    emit('finished', allOk.value)
    emit('update:show', false)
  } finally {
    closing.value = false
  }
}

/** 用户点「仍然继续」：只在没有可执行步骤、且提醒不会阻断时出现。 */
async function continueAnyway() {
  if (closing.value) return
  closing.value = true
  try {
    emit('finished', true)
    emit('update:show', false)
  } finally {
    closing.value = false
  }
}

/** 步骤行的中文动作名。 */
const ACTION_LABELS: Record<Step['action'], string> = {
  uninstall: 'recycle.uninstall',
  upgrade: 'recycle.upgrade',
  downgrade: 'recycle.downgrade',
  'install-dependency': 'modsOnline.installDependency',
}

/**
 * 提醒文案：说清「跟谁」「差在哪」。
 *
 * 这些条目无法自动处理，所以文案必须能让用户自己判断该怎么做：
 * 兼容关系要说明需要的区间，冲突要说明该卸载，缺依赖要说明缺什么。
 */
function blockedText(status: RelationStatus): string {
  const kind = status.relation.kind ?? 'requires'
  const target = status.installedMod?.name ?? status.relation.guid
  const min = status.relation.min_version?.trim()
  const max = status.relation.max_version?.trim()
  const range = min && max ? `${min} ~ ${max}` : min ? `≥${min}` : max ? `≤${max}` : ''

  if (kind === 'compatible') {
    // 兼容列表：只能提醒版本不对，卸载对方没有意义
    return range
      ? t('modsOnline.isIncompatibleAVersionInIsRequired', {
        name: target, version: status.installedVersion ?? '', range,
      })
      : t('modsOnline.isIncompatiblePleaseCheck', {
        name: target, version: status.installedVersion ?? '',
      })
  }
  if (kind === 'conflicts') {
    return t('modsOnline.conflictsPleaseUninstallIt', {
      name: target, version: status.installedVersion ?? '',
    })
  }
  // requires：缺或版本不符
  return range
    ? t('modsOnline.missingDependencyAVersionInIsRequired', { name: target, range })
    : t('modsOnline.missingDependency2', { name: target })
}
</script>

<template>
  <AppModal :show="show" :mask-closable="false" :title="t('modsOnline.preInstallCheck')"
    max-width="min(680px, 94%)"
    @update:show="(value: boolean) => { if (!value) void close() }">
    <p class="plan-lead">
      {{ t('modsOnline.beforeInstallingTheseRelationsMustBeHandledFirst', { name: targetLabel }) }}
    </p>

    <div v-if="steps.length" class="plan-list">
      <div v-for="step in steps" :key="step.key" class="plan-row">
        <n-tag size="tiny" :bordered="false" class="plan-kind"
          :color="tagColorFor(step.action === 'uninstall' ? 'critical' : 'moderate')">
          {{ t(ACTION_LABELS[step.action]) }}
        </n-tag>
        <div class="plan-main">
          <b class="plan-name" :title="step.label">{{ step.label }}</b>
          <span v-if="step.version" class="plan-version mono">{{ step.version }}</span>
        </div>
        <n-spin v-if="running && !done[step.key]" size="small" />
        <span v-else-if="done[step.key] === 'ok'" class="plan-state ok">✓</span>
        <span v-else-if="done[step.key] === 'failed'" class="plan-state bad">✕</span>
      </div>
    </div>

    <!-- 无法自动处理的关系：只说明原因，不给按钮，避免假的可操作入口 -->
    <n-alert v-if="blocked.length" type="warning" :bordered="false" class="plan-blocked">
      <div v-for="(status, index) in blocked" :key="index" class="plan-blocked-row">
        {{ blockedText(status) }}
      </div>
    </n-alert>

    <n-alert v-if="failures.length" type="error" :bordered="false" class="plan-failures">
      <div v-for="failure in failures" :key="failure.label" class="plan-failure">
        <b>{{ failure.label }}</b>：{{ failure.reason }}
      </div>
    </n-alert>

    <template #footer>
      <n-button v-if="!allDone" size="small" type="primary" :loading="running"
        :disabled="running || !steps.length" @click="runAll">
        {{ t('modsOnline.handleAll') }}
      </n-button>
      <!--
        没有可执行步骤时只有提醒：让用户明确「我看过了仍要继续」，
        而不是把提醒当成错误挡住安装。
      -->
      <n-button v-if="!steps.length && blocked.length" size="small" type="primary"
        @click="continueAnyway">
        {{ t('modsOnline.installAnyway') }}
      </n-button>
      <n-button size="small" :quaternary="!allDone" :type="allDone ? 'primary' : 'default'"
        :disabled="running" @click="close">
        {{ allDone ? t('modsOnline.done') : t('recycle.cancel') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.plan-lead {
  margin: 0 0 12px;
  font-size: 13px;
  line-height: 1.6;
  color: var(--oc2-text-2);
}
.plan-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.plan-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  border-radius: 9px;
  background: var(--oc2-sunken);
  border: 1px solid var(--oc2-row-line);
  min-width: 0;
}
.plan-kind {
  flex: 0 0 auto;
}
.plan-main {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.plan-name {
  font-size: 13px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.plan-version {
  flex: 0 0 auto;
  font-size: 11.5px;
  color: var(--oc2-text-3);
}
.plan-state {
  flex: 0 0 auto;
  font-size: 15px;
  font-weight: 700;
}
.plan-state.ok { color: var(--oc2-accent); }
.plan-state.bad { color: #ee8585; }
.plan-blocked,
.plan-failures {
  margin-top: 12px;
}
.plan-blocked-row,
.plan-failure {
  font-size: 12.5px;
  line-height: 1.6;
}
</style>
