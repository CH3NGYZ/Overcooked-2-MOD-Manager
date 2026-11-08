<script setup lang="ts">
/**
 * MOD 关系计划 → 共享「安装前检查」弹窗的适配器。
 *
 * 弹窗本体（展示 / 按顺序执行 / 失败重试 / 放行判定）在 `InstallCheckDialog`，
 * 与地图导入的同身份冲突共用同一份。这里只剩两件 MOD 侧独有的东西：
 *   1. 把 `modRelations.buildInstallPlan` 的计划翻成检查模型；
 *   2. 提供「怎么卸一个 MOD」「怎么装一个依赖」—— 后者还要接下载进度与
 *      失败重试卡片，这是 MOD 安装特有的。
 *
 * 对外 props / emit 与拆分前完全一致，三个页面（两个 MOD 页 + 地图下载页）
 * 的挂载代码不用改。
 */
import { computed } from 'vue'
import { useGameOperationGate } from '../composables/useGameOperationGate'
import { t } from '../i18n'
import { beginDownloadTask, endDownloadTask, failDownloadTask } from '../downloadTask'
import { installCheckModelFromPlan } from '../installCheck'
import { api } from '../services/tauri'
import InstallCheckDialog from './InstallCheckDialog.vue'
import type { InstallCheckRow } from '../installCheck'
import type { InstallPlan } from '../modRelations'
import { confirmModInstallNotes, firstInstallNotes } from '../modInstallNotes'

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

const { blockModOperation } = useGameOperationGate()
const model = computed(() => installCheckModelFromPlan(props.plan, props.targetLabel))

async function confirmNotes(rows: readonly InstallCheckRow[]) {
  const statuses = rows.filter((row) => row.kind !== 'uninstall')
    .map((row) => props.plan.install[row.index])
  // 关系解析已按 GUID 找到本地 MOD，升级/降级不能因为暂时缺仓库绑定而误弹。
  const repositories = statuses.flatMap((status) =>
    status?.repository && !status.installedMod ? [status.repository] : [])
  return confirmModInstallNotes(firstInstallNotes(repositories, []))
}

/**
 * 执行一行。
 *
 * 失败一律抛：弹窗负责标记这一行失败、记下原因并留在原地，这里不自己弹提示，
 * 否则同一条失败会既有行下方红字、又有 toast。
 */
async function execute(row: InstallCheckRow) {
  if (blockModOperation()) throw new Error(t('game.running'))
  if (row.kind === 'uninstall') return uninstallAt(row.index)
  return installAt(row.index, row.label)
}

async function uninstallAt(index: number) {
  const status = props.plan.uninstall[index]
  const target = status?.installedMod
  if (!target) throw new Error(t('modsOnline.theModToUninstallWasNotFound'))
  /*
   * 后端可能要求「整包删除」二次确认（该 DLL 属于多文件包）。
   * 这里直接带 `allow_unmanaged = true` 走整包删除 —— 用户已经在
   * 本弹窗里确认过「为了装这个 MOD 要卸载它」，再问一次只是重复。
   */
  const result = await api.uninstallLocalMod(target.dll_file, true)
  if (!result.ok) throw new Error(result.message)
}

async function installAt(index: number, label: string) {
  const status = props.plan.install[index]
  const target = status?.repository
  if (!target) throw new Error(t('modsOnline.noInstallableSourceFound'))
  const tag = status?.release?.tag_name
  /**
   * 下载 + 确认安装。
   *
   * 失败卡片上的「重试」重跑的就是这一个函数：确认安装依赖这次下载出来的
   * 预览，两者拆不开；好处是它不重跑整张计划里已经完成的卸载与安装步骤。
   * `endDownloadTask` 之后任务号已经失效，确认安装再抛错不会点亮下载卡片。
   */
  const install = async (): Promise<void> => {
    if (blockModOperation()) throw new Error(t('game.running'))
    const token = beginDownloadTask({ label, retry: install })
    try {
      // 先下载预览，再直接确认安装：暂存目录与用户确认已在上一层完成
      const outcome = await api.installModRepository(target.id, tag, true)
      endDownloadTask(token)
      if (!outcome.ok || !outcome.preview) {
        throw new Error(outcome.message
          || t('modsOnline.downloadFinishedButNoInstallableContentWasParsed'))
      }
      if (blockModOperation()) {
        await api.cancelModPreview(outcome.preview.preview_id)
        throw new Error(t('game.running'))
      }
      const confirmed = await api.confirmModInstall(outcome.preview.preview_id)
      if (!confirmed.ok) throw new Error(confirmed.message)
    } catch (reason) {
      failDownloadTask(token, reason)
      throw reason
    }
  }
  await install()
}
</script>

<template>
  <InstallCheckDialog :show="show" :model="model" :execute="execute"
    :before-execute="confirmNotes"
    @update:show="(value: boolean) => emit('update:show', value)"
    @finished="(ready: boolean) => emit('finished', ready)" />
</template>
