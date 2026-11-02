<script setup lang="ts">
/**
 * 共享的「安装前检查」弹窗：把要处理的事一次列清并按顺序执行。
 *
 * 三种来源共用这一个弹窗：
 *   · MOD 安装的关系计划（依赖 / 冲突 / 兼容）—— 见 `installCheckModelFromPlan`
 *   · MOD 安装的同身份冲突（同一个 GUID 已装）
 *   · 地图导入的同身份冲突（同一张地图已装）
 * 后两种只含「卸载」一种行，模型由 `conflictCheckModel` 生成。
 *
 * 为什么顺序不能由宿主各写一份：这些事互相影响（冲突要先腾地方，版本要换成
 * 区间内的那个，缺件最后补），拆成三处提醒的话用户会在三个角落被提醒三次，
 * 中间状态（装了一半、冲突还在）最难解释。所以「按顺序执行、失败留在原地、
 * 全部成功才放行」只在这里实现一次。
 *
 * 组件**不做任何卸载/安装**：每一行都交回宿主注入的 `execute(row)`。
 * 两种域调的 IPC 不同（MOD 走 `uninstallLocalMod`，地图走 `deleteLevelSet`），
 * 而 `InstallConflict` 里没有域标记，让弹窗猜域只会猜错。
 *
 * 失败时不停在原地等用户自己看：失败行标 ✕ 并列出原因，再点「全部处理」
 * 只跑还没成功的那几行 —— 已经进了回收站的不能重复卸载，否则第二次必报
 * 「找不到」，用户永远装不上。
 */
import { computed, ref, watch } from 'vue'
import { NAlert, NButton, NSpin, NTag } from 'naive-ui'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { useNotify } from '../useNotify'
import { tagColorFor } from '../theme'
import AppModal from './AppModal.vue'
import type { InstallCheckModel, InstallCheckRow } from '../installCheck'

const props = defineProps<{
  show: boolean
  /** 本次要处理的内容；`null` 时弹窗内容为空（只会在关闭动画期间出现）。 */
  model: InstallCheckModel | null
  /**
   * 执行一行；**失败必须抛**（`Error` 或后端 tag 字符串都可以）。
   *
   * 抛出的原因会经 `backendMessageOf` 翻译后挂在行下方，宿主不需要自己弹
   * 错误提示，也不需要维护「哪一行失败过」—— 重试就是再点一次「全部处理」。
   */
  execute: (row: InstallCheckRow) => Promise<void>
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

/** 每一行的执行状态：按 `row.key` 索引。 */
const done = ref<Record<string, 'ok' | 'failed'>>({})
const failures = ref<{ label: string; reason: string }[]>([])
const running = ref(false)
/** 防重入：本组件自己 emit `update:show false`，模板又在那事件上调 close。 */
const closing = ref(false)

/**
 * 每次打开弹窗都清空上一次的执行状态。
 *
 * 这个组件实例是常驻的（`<InstallCheckDialog>` 一直挂在页面里，只有内层
 * n-modal 被销毁），所以 `done` / `failures` 会跨次残留。而行的 key 是按
 * 身份生成的（`install-<仓库 id>-<版本>`），新一批内容若与上一批相同就会
 * 撞上残留的「已完成」—— 表现是刚打开检查窗口那一行已经打着 ✓、按钮直接
 * 是「完成」，点下去跳过该步骤，装完仍缺依赖。所以 keys 带身份与打开即重置
 * 这两条必须同时成立。
 */
watch(() => props.show, (visible) => {
  if (!visible) return
  done.value = {}
  failures.value = []
  running.value = false
  closing.value = false
})

const rows = computed(() => props.model?.rows ?? [])
/** 提醒类条目：无法自动处理，只能让用户知道。 */
const blocked = computed(() => props.model?.blocked ?? [])

/** 有可执行步骤，且都还没做。 */
const pending = computed(() => rows.value.filter((row) => !done.value[row.key]))
const allDone = computed(() => rows.value.length > 0 && pending.value.length === 0)
/** 全部成功：没有失败、没有提醒。 */
const allOk = computed(() =>
  allDone.value && failures.value.length === 0 && blocked.value.length === 0)

/** 按顺序执行全部步骤。 */
async function runAll() {
  if (running.value || !rows.value.length) return
  running.value = true
  failures.value = []
  try {
    for (const row of rows.value) {
      if (done.value[row.key]) continue
      try {
        await props.execute(row)
        done.value = { ...done.value, [row.key]: 'ok' }
      } catch (reason) {
        markFailed(row, reason)
      }
    }
    report()
  } finally {
    running.value = false
  }
  /*
   * 全部处理成功就直接放行下一步：用户点「全部处理」时已经授权了这些改动
   * （卸载/升级/装依赖），再点一次「完成」只是把同一个意思说两遍，
   * 还让「装主包」白多一次点击。有失败或存在无法自动处理的提醒时仍停在这里，
   * 那种情况需要用户自己判断要不要继续。
   */
  if (allOk.value) await close()
}

function markFailed(row: InstallCheckRow, reason: unknown) {
  /*
   * 收口翻译：调用方抛的既可能是后端 tag（`result.message`），也可能是
   * 已经翻译过的文案。统一在这里过一遍桥接层，比在每个执行点各写一次
   * `translateBackendMessage` 更难漏。
   */
  const text = backendMessageOf(reason)
  done.value = { ...done.value, [row.key]: 'failed' }
  failures.value = [...failures.value, { label: row.label, reason: text }]
}

/** 汇总结果：成功时只提示一次，失败逐条点名。 */
function report() {
  const okCount = rows.value.filter((row) => done.value[row.key] === 'ok').length
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

const title = computed(() => props.model?.title ?? t('modsOnline.preInstallCheck'))
</script>

<template>
  <AppModal :show="show" :mask-closable="false" :title="title"
    max-width="min(680px, 94%)"
    @update:show="(value: boolean) => { if (!value) void close() }">
    <p class="plan-lead">
      {{ model?.lead }}
    </p>

    <div v-if="rows.length" class="plan-list">
      <div v-for="row in rows" :key="row.key" class="plan-row">
        <n-tag size="tiny" :bordered="false" class="plan-kind"
          :color="tagColorFor(row.kind === 'uninstall' ? 'critical' : 'moderate')">
          {{ row.actionLabel }}
        </n-tag>
        <div class="plan-main">
          <b class="plan-name" :title="row.label">{{ row.label }}</b>
          <span v-if="row.version" class="plan-version mono">{{ row.version }}</span>
          <!-- 同版本只影响行尾标签：动作仍然是「先卸载再安装」 -->
          <n-tag v-if="row.sameVersion" size="tiny" :bordered="false" class="plan-same"
            :color="tagColorFor('muted')">
            {{ model?.sameVersionLabel }}
          </n-tag>
        </div>
        <n-spin v-if="running && !done[row.key]" size="small" />
        <span v-else-if="done[row.key] === 'ok'" class="plan-state ok">✓</span>
        <span v-else-if="done[row.key] === 'failed'" class="plan-state bad">✕</span>
      </div>
    </div>

    <!-- 无法自动处理的关系：只说明原因，不给按钮，避免假的可操作入口 -->
    <n-alert v-if="blocked.length" type="warning" :bordered="false" class="plan-blocked">
      <div v-for="(text, index) in blocked" :key="index" class="plan-blocked-row">
        {{ text }}
      </div>
    </n-alert>

    <n-alert v-if="failures.length" type="error" :bordered="false" class="plan-failures">
      <div v-for="failure in failures" :key="failure.label" class="plan-failure">
        <b>{{ failure.label }}</b>：{{ failure.reason }}
      </div>
    </n-alert>

    <template #footer>
      <n-button v-if="!allDone" size="small" type="primary" :loading="running"
        :disabled="running || !rows.length" @click="runAll">
        {{ t('modsOnline.handleAll') }}
      </n-button>
      <!--
        没有可执行步骤时只有提醒：让用户明确「我看过了仍要继续」，
        而不是把提醒当成错误挡住安装。
      -->
      <n-button v-if="!rows.length && blocked.length" size="small" type="primary"
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
.plan-same {
  flex: 0 0 auto;
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
