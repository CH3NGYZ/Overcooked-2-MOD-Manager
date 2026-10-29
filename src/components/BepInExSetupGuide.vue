<script setup lang="ts">
/**
 * BepInEx 安装后的平台引导（仅非 Windows）。
 *
 * 两条分支的操作完全不同，所以各自一套步骤：
 *
 * **CrossOver（macOS 的 Wine 容器）**
 *   安装前就要把容器准备好，否则装完也起不来。步骤是「装 CrossOver 字体 →
 *   确认容器 → 安装 BepInEx」，做完点「我已完成所需操作」才真正开始下载安装。
 *
 * **原生 macOS**
 *   安装本身能自动完成（路径可探测），但必须手动往 Steam 启动项里粘贴命令，
 *   否则 BepInEx 不会随游戏加载。所以流程是「先安装 → 修 Steam Overlay 注入
 *   → 再弹这个窗给启动命令」。Overlay 修复放在弹窗之前，因为它是自动的，
 *   不该占用用户的手动步骤。
 *
 * 不引入图片素材：文字步骤已能说明操作，缺图不影响可用性（见 PLAN.md 第 5 步）。
 */
import { computed, ref, watch } from 'vue'
import { NAlert, NButton, NSteps, NStep } from 'naive-ui'
import { CopyOutline } from '@vicons/ionicons5'
import AppModal from './AppModal.vue'
import { t } from '../i18n'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { corefontsCommand, nativeSteamLaunchOptions } from '../gameRuntime'
import type { AppPlatform, GameInfo, GameRuntime, PublicConfig } from '../types'

const props = defineProps<{
  show: boolean
  config: PublicConfig | null
  game: GameInfo | null
  /** `before-install` = CrossOver 的安装前引导；`after-install` = 原生安装后。 */
  phase: 'before-install' | 'after-install'
}>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /** 用户点了「我已完成所需操作」：宿主据此继续安装。 */
  (event: 'confirmed'): void
}>()

const message = useNotify()

const platform = computed<AppPlatform>(() => props.config?.platform ?? 'windows')
const runtime = computed<GameRuntime>(() => props.config?.game.runtime ?? 'wine')
const isMacNative = computed(() => platform.value === 'macos' && runtime.value === 'native')

/** 当前步骤（0 基）。用户可点步骤条回看，但不影响按钮可用性。 */
const step = ref(0)
watch(() => props.show, (visible) => { if (visible) step.value = 0 })

/** CrossOver 容器路径（后端探测结果）。 */
const winePrefix = computed(() => props.game?.wine_prefix ?? '')
/** 补字体命令；没有容器路径时为空，界面会隐藏这一步。 */
const fontsCommand = computed(() => corefontsCommand(winePrefix.value || undefined))
/** Steam 启动项命令（仅原生）。 */
const launchOptions = computed(() =>
  nativeSteamLaunchOptions(platform.value, props.game?.path ?? ''),
)

/** 复制一段文本，用 toast 反馈（与全局复制入口同一套）。 */
async function copy(value: string) {
  if (!value) return
  try {
    await api.copyText(value)
    message.success(t('已复制'))
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  }
}

/** 「我已完成所需操作」：关闭并让宿主继续下一步。 */
function confirm() {
  emit('update:show', false)
  emit('confirmed')
}
</script>

<template>
  <AppModal :show="show" :mask-closable="false" display-directive="if"
    :title="phase === 'before-install' ? t('安装前准备') : t('还需要一步')"
    max-width="min(620px, 92%)"
    @update:show="(value: boolean) => emit('update:show', value)">
    <!--
      安装前（CrossOver）：容器没准备好时装完也起不来，所以先引导。
      容器路径由后端探测；探测不到就说明游戏还没在 CrossOver 里跑过。
    -->
    <template v-if="phase === 'before-install'">
      <n-alert v-if="!winePrefix" type="warning" :bordered="false" class="guide-alert">
        {{ t('未检测到 CrossOver 容器。请先在 CrossOver 里启动一次《胡闹厨房2》，让容器生成后再回来。') }}
      </n-alert>
      <n-steps :current="step + 1" size="small" class="guide-steps">
        <n-step :title="t('准备 CrossOver 容器')"
          :description="t('确认游戏能在 CrossOver 中正常启动')" />
        <n-step :title="t('安装字体')"
          :description="t('补上 corefonts，否则 BepInEx 控制台会显示乱码')" />
        <n-step :title="t('安装 BepInEx')"
          :description="t('回到这里点击下方按钮继续')" />
      </n-steps>

      <div v-if="fontsCommand" class="guide-block">
        <span class="guide-label">{{ t('在终端执行以下命令安装字体') }}</span>
        <div class="guide-code">
          <!-- 纯文本命令：不传 language，n-code 也就不会去找 hljs 实例 -->
          <code class="guide-command">{{ fontsCommand }}</code>
          <n-button size="tiny" quaternary :title="t('复制')" @click="copy(fontsCommand)">
            <template #icon><CopyOutline width="14" height="14" /></template>
          </n-button>
        </div>
      </div>
      <p class="guide-note muted">
        {{ t('这一步不是必须的：跳过只会让 BepInEx 控制台里的中文显示成方块，不影响 MOD 加载。') }}
      </p>
    </template>

    <!--
      安装后（原生 macOS）：BepInEx 已装好，只差把启动命令写进 Steam。
      命令里带游戏目录里的 run_bepinex.sh，路径含空格，必须整段复制。
    -->
    <template v-else>
      <n-alert v-if="isMacNative" type="info" :bordered="false" class="guide-alert">
        {{ t('BepInEx 已安装。最后一步：把下面的命令粘贴到 Steam 里《胡闹厨房2》的启动选项中。') }}
      </n-alert>

      <div v-if="launchOptions" class="guide-block">
        <span class="guide-label">{{ t('启动选项') }}</span>
        <div class="guide-code">
          <code class="guide-command">{{ launchOptions }}</code>
          <n-button size="tiny" quaternary :title="t('复制')" @click="copy(launchOptions)">
            <template #icon><CopyOutline width="14" height="14" /></template>
          </n-button>
        </div>
      </div>
      <n-alert v-else type="warning" :bordered="false" class="guide-alert">
        {{ t('尚未探测到游戏目录，无法生成启动命令。请先在设置里确认游戏目录。') }}
      </n-alert>

      <n-steps :current="3" size="small" class="guide-steps">
        <n-step :title="t('库中右键游戏')" :description="t('Steam → 库 → 右键《胡闹厨房2》')" />
        <n-step :title="t('打开属性')" :description="t('选择「属性」→「通用」')" />
        <n-step :title="t('粘贴启动选项')" :description="t('把上面的命令粘贴到「启动选项」')" />
      </n-steps>
    </template>

    <template #footer>
      <n-button size="small" type="primary" @click="confirm">
        {{ t('我已完成所需操作') }}
      </n-button>
      <n-button size="small" quaternary @click="emit('update:show', false)">
        {{ t('取消') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.guide-alert { margin-bottom: 14px; }
.guide-steps { margin-bottom: 16px; }
.guide-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 12px;
}
.guide-label {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--oc2-text-2);
}
/*
 * 命令块：等宽字体 + 右侧复制按钮。
 * 命令可能很长（含完整路径），必须能横向滚动而不能撑破弹窗。
 */
.guide-code {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 9px 10px;
  border-radius: 9px;
  background: var(--oc2-sunken);
  border: 1px solid var(--oc2-row-line);
  min-width: 0;
}
/*
 * 命令文本：等宽 + 可横向滚动。
 * 启动命令含完整游戏路径，可能很长，必须能滚而不能撑破弹窗。
 */
.guide-command {
  flex: 1 1 auto;
  min-width: 0;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.6;
  color: var(--oc2-text-2);
  white-space: pre-wrap;
  word-break: break-all;
}
.guide-note {
  margin: 0;
  font-size: 12px;
  line-height: 1.6;
}
</style>
