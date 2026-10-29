<script setup lang="ts">
/**
 * 批量安装依赖的确认弹窗。
 *
 * 场景：一个 MOD 声明了多条未满足的依赖。逐个让用户走一遍安装预览太啰嗦，
 * 所以这里一次列全部，确认后依次写入。
 *
 * **两步式，与单个安装保持同一语义**：
 *   1. 宿主先把每条依赖都 `preview_only` 下载并解析（`items` 已是预览结果）
 *   2. 用户在这里确认后，逐个 `confirm_mod_install`
 *
 * 为什么不合并成一个后端命令：预览与写入之间的「用户确认」是安全边界。
 * 预览阶段只用暂存目录，取消时 `cancel_mod_preview` 全部清掉；
 * 写入失败也不会影响已成功的那些（逐个独立事务）。
 *
 * 部分失败的处理：继续装剩下的，最后汇总报告。中途停下会让用户
 * 面对「装了 2 个还有 1 个」的不确定状态，不如一次做完再看结果。
 */
import { computed, ref, watch } from 'vue'
import { NAlert, NButton, NSpin } from 'naive-ui'
import { t } from '../i18n'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import AppModal from './AppModal.vue'
import type { ModPreview } from '../types'

/** 一条待安装的依赖：仓库名 + 预览结果。 */
export interface DependencyInstallItem {
  /** 依赖声明里的 GUID，用于失败时点名。 */
  guid: string
  /** 展示用的仓库名（跟随界面语言）。 */
  label: string
  preview: ModPreview
}

const props = defineProps<{
  show: boolean
  items: DependencyInstallItem[]
}>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /** 全部处理完（无论成败），宿主据此刷新 Dashboard。 */
  (event: 'finished'): void
}>()

const message = useNotify()
const installing = ref(false)
/** 逐条状态：成功后打勾，失败保留原因。 */
const done = ref<Record<string, 'ok' | 'failed'>>({})
const failures = ref<{ label: string; reason: string }[]>([])

/** 弹窗关闭时重置状态，下次打开是干净的。 */
watch(() => props.show, (visible) => {
  if (visible) {
    done.value = {}
    failures.value = []
    return
  }
  /*
   * 关闭时把**尚未安装**的预览全部丢弃，避免暂存目录留下下载好的包。
   * 已装成功的 preview_id 后端在 confirm 里已删，重复 cancel 是幂等的。
   */
  if (!installing.value) void discardPending()
})

/** 找出还没处理的预览（没打勾也没失败记录）。 */
function pendingItems() {
  return props.items.filter((item) => !done.value[item.preview.preview_id])
}

async function discardPending() {
  for (const item of pendingItems()) {
    await api.cancelModPreview(item.preview.preview_id).catch(() => undefined)
  }
}

const allDone = computed(() => props.items.length > 0 && pendingItems().length === 0)
const successCount = computed(() =>
  Object.values(done.value).filter((state) => state === 'ok').length)

/**
 * 依次写入。
 *
 * 顺序执行而不是并发：安装要拿操作锁并改游戏目录，并发没有收益，
 * 而且失败时日志与回收记录会交错、难以排查。
 */
async function installAll() {
  if (installing.value || !props.items.length) return
  installing.value = true
  failures.value = []
  try {
    for (const item of props.items) {
      if (done.value[item.preview.preview_id]) continue
      try {
        const result = await api.confirmModInstall(item.preview.preview_id)
        if (result.ok) {
          done.value = { ...done.value, [item.preview.preview_id]: 'ok' }
        } else {
          done.value = { ...done.value, [item.preview.preview_id]: 'failed' }
          failures.value = [...failures.value, { label: item.label, reason: result.message }]
        }
      } catch (reason) {
        done.value = { ...done.value, [item.preview.preview_id]: 'failed' }
        failures.value = [...failures.value, {
          label: item.label,
          reason: reason instanceof Error ? reason.message : String(reason),
        }]
      }
    }
    if (successCount.value) {
      message.success(t('已安装 {count} 个依赖', { count: successCount.value }))
    }
    for (const failure of failures.value) {
      message.error(`${failure.label}：${failure.reason}`)
    }
    emit('finished')
  } finally {
    installing.value = false
  }
}

/**
 * 关闭弹窗。全部处理完后由用户点「完成」；中途关闭则丢弃未安装的暂存。
 */
async function close() {
  await discardPending()
  emit('update:show', false)
}
</script>

<template>
  <AppModal :show="show" :mask-closable="false" :title="t('安装依赖')"
    max-width="min(560px, 92%)"
    @update:show="(value: boolean) => { if (!value) void close() }">
    <p class="dep-lead">
      {{ t('以下依赖尚未安装，确认后将依次安装：') }}
    </p>

    <div class="dep-list">
      <div v-for="item in items" :key="item.preview.preview_id" class="dep-row">
        <div class="dep-main">
          <b class="dep-name" :title="item.label">{{ item.label }}</b>
          <span class="dep-meta mono">
            <template v-if="item.preview.release_tag">{{ item.preview.release_tag }}</template>
            <template v-if="item.preview.total_dlls">
              {{ item.preview.total_dlls }} 个 DLL
            </template>
          </span>
        </div>
        <n-spin v-if="installing && !done[item.preview.preview_id]" size="small" />
        <span v-else-if="done[item.preview.preview_id] === 'ok'" class="dep-state ok">✓</span>
        <span v-else-if="done[item.preview.preview_id] === 'failed'" class="dep-state bad">✕</span>
      </div>
    </div>

    <n-alert v-if="failures.length" type="warning" :bordered="false" class="dep-failures">
      <div v-for="failure in failures" :key="failure.label" class="dep-failure">
        <b>{{ failure.label }}</b>：{{ failure.reason }}
      </div>
    </n-alert>

    <template #footer>
      <n-button v-if="!allDone" size="small" type="primary" :loading="installing"
        :disabled="installing" @click="installAll">
        {{ t('全部安装') }}
      </n-button>
      <n-button size="small" :quaternary="!allDone" :type="allDone ? 'primary' : 'default'"
        :disabled="installing" @click="close">
        {{ allDone ? t('完成') : t('取消') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.dep-lead {
  margin: 0 0 12px;
  font-size: 13px;
  line-height: 1.6;
  color: var(--oc2-text-2);
}
.dep-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.dep-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 9px 12px;
  border-radius: 9px;
  background: var(--oc2-sunken);
  border: 1px solid var(--oc2-row-line);
  min-width: 0;
}
.dep-main {
  display: flex;
  flex-direction: column;
  gap: 3px;
  flex: 1 1 auto;
  min-width: 0;
}
.dep-name {
  font-size: 13px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dep-meta {
  font-size: 11.5px;
  color: var(--oc2-text-3);
  display: flex;
  gap: 8px;
}
.dep-state {
  flex: 0 0 auto;
  font-size: 15px;
  font-weight: 700;
}
.dep-state.ok { color: var(--oc2-accent); }
.dep-state.bad { color: #ee8585; }
.dep-failures { margin-top: 12px; }
.dep-failure {
  font-size: 12.5px;
  line-height: 1.6;
}
</style>
