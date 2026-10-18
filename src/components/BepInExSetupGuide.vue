<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { Copy, ExternalLink, X } from 'lucide-vue-next'
import type { BepInExInstallResult, GameInfo, PublicConfig } from '../types'
import { corefontsCommand, gameExecutableName, nativeSteamLaunchOptions } from '../gameRuntime'
import { trapModalFocus } from '../composables/modalFocus'
import { api } from '../services/tauri'
import { t } from '../i18n'
import openPropertiesImage from '../assets/tutorial/MacOS/Open properties.png'
import pasteCommandImage from '../assets/tutorial/MacOS/Paste command to startup settings.png'

const props = defineProps<{
  visible: boolean
  config?: PublicConfig
  game?: GameInfo
  result?: BepInExInstallResult | null
  busy: boolean
  repairNativeOverlay: () => Promise<boolean>
}>()
const emit = defineEmits<{ close: [] }>()
const dialog = ref<HTMLElement | null>(null)
const feedback = ref('')
const feedbackError = ref(false)
const repairingOverlay = ref(false)
const backdropPressed = ref(false)
let trigger: HTMLElement | null = null
const visible = computed(() => props.visible && !!props.config && props.config.platform !== 'windows')
const native = computed(() => props.config?.game.runtime === 'native')
const macosNative = computed(() => native.value && props.config?.platform === 'macos')
let feedbackGeneration = 0
const launchOptions = computed(() => props.config ? nativeSteamLaunchOptions(props.config.platform, props.game?.path || '') : '')
const fontsCommand = computed(() => corefontsCommand(props.game?.wine_prefix))
const executable = computed(() => gameExecutableName(props.config?.platform, props.config?.game.runtime))

async function copy(value: string) {
  try {
    await api.copyText(value)
    feedback.value = t('已复制')
    feedbackError.value = false
  } catch (error) {
    feedback.value = t(String(error))
    feedbackError.value = true
  }
}

async function repairOverlay() {
  if (repairingOverlay.value || props.busy || props.game?.kitchen_running || !macosNative.value) return
  const generation = feedbackGeneration
  repairingOverlay.value = true
  feedback.value = ''
  try {
    const repaired = await props.repairNativeOverlay()
    if (repaired && visible.value && generation === feedbackGeneration) {
      feedback.value = t('Steam Overlay 注入修复完成')
      feedbackError.value = false
    }
  } catch (error) {
    if (visible.value && generation === feedbackGeneration) {
      feedback.value = t(String(error))
      feedbackError.value = true
    }
  } finally {
    repairingOverlay.value = false
  }
}

async function openDocumentation() {
  try {
    await api.openReleasePage(native.value
      ? 'https://docs.bepinex.dev/articles/advanced/steam_interop.html'
      : 'https://docs.bepinex.dev/articles/advanced/proton_wine.html')
    feedback.value = ''
  } catch (error) {
    feedback.value = t(String(error))
    feedbackError.value = true
  }
}

watch(visible, async (shown) => {
  feedbackGeneration += 1
  feedback.value = ''
  backdropPressed.value = false
  if (shown) trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
  await nextTick()
  if (shown && visible.value) dialog.value?.focus()
  else if (!shown && trigger?.isConnected) trigger.focus()
}, { immediate: true })
onUnmounted(() => { if (trigger?.isConnected) trigger.focus() })
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="modal-backdrop"
      @pointerdown="backdropPressed = $event.target === $event.currentTarget"
      @pointercancel="backdropPressed = false"
      @click.self="backdropPressed && emit('close')"
      @keydown.esc.stop.prevent="emit('close')">
      <section ref="dialog" class="modal bepinex-setup-modal" role="dialog" aria-modal="true"
        aria-labelledby="bepinex-setup-title" tabindex="-1" @keydown.tab.stop="trapModalFocus">
        <header class="modal-head">
          <h3 id="bepinex-setup-title">{{ t('Steam / Wine 启动设置') }}</h3>
          <button class="icon-button" :title="t('关闭')" :aria-label="t('关闭')" @click="emit('close')"><X :size="17" /></button>
        </header>
        <div class="bepinex-setup-content">
          <template v-if="native">
            <template v-if="macosNative">
              <p v-if="result?.macos_overlay_repaired" class="bepinex-setup-success">{{ t('已修复启动脚本的 Steam Overlay 注入。请继续配置 Steam 启动项。') }}</p>
              <p v-else>{{ t('修复 run_bepinex.sh 的 Steam Overlay 注入后，请配置 Steam 启动项。') }}</p>
              <button class="secondary-button" :disabled="repairingOverlay || busy || game?.kitchen_running || !game?.game_exists" @click="repairOverlay">{{ repairingOverlay ? t('正在修复 macOS Steam Overlay') : t('修复 Steam Overlay 注入') }}</button>
            </template>
            <p>{{ t('管理器安装原生框架时会配置 run_bepinex.sh 并设置执行权限。Steam 启动项仍需手动填写。') }}</p>
            <h4>{{ t('原生 Steam 启动项') }}</h4>
            <p>{{ t('在 Steam 中右键游戏，选择"属性"，在"启动选项"中输入以下命令：') }}</p>
            <div v-if="launchOptions" class="bepinex-setup-command">
              <pre>{{ launchOptions }}</pre>
              <button class="secondary-button small-button" @click="copy(launchOptions)"><Copy :size="14" />{{ t('复制') }}</button>
            </div>
            <p v-else class="guard-note">{{ t('请先选择包含 {executable} 的厨房目录。').replace('{executable}', executable) }}</p>
            <div v-if="macosNative" class="bepinex-setup-tutorial">
              <figure><figcaption>{{ t('打开游戏属性') }}</figcaption><img :src="openPropertiesImage" :alt="t('打开游戏属性')"></figure>
              <figure><figcaption>{{ t('将上方命令粘贴到启动选项，替换原有内容') }}</figcaption><img :src="pasteCommandImage" :alt="t('将上方命令粘贴到启动选项，替换原有内容')"></figure>
            </div>
            <p>{{ t('请使用原生 Steam 启动原生厨房，不要强制启用 Proton 或其他兼容层。') }}</p>
          </template>
          <template v-else>
            <h4>{{ t('Wine 容器的 winhttp 覆盖') }}</h4>
            <p>{{ t('Wine 版使用 Windows Steam、Overcooked2.exe 和 Windows x86 BepInEx 包。') }}</p>
            <div v-if="game?.wine_prefix" class="bepinex-setup-path"><b>{{ t('目标 Wine 容器') }}</b><code>{{ game.wine_prefix }}</code></div>
            <p v-else class="guard-note">{{ t('无法确定游戏使用的 Wine 容器，请重新选择厨房目录') }}</p>
            <p v-if="result?.wine_override_applied" class="bepinex-setup-success">{{ t('已在游戏 Wine 容器中设置 winhttp 原装优先覆盖。') }}</p>
            <p v-if="result?.wine_override_error" class="guard-note">{{ t(result.wine_override_error) }}</p>
            <p>{{ config?.platform === 'macos'
              ? t('在 CrossOver 中选择游戏所在容器，打开 Wine 配置，在“函数库”中添加 winhttp 并设为“原装先于内建”。')
              : t('打开同一 WINEPREFIX 的 winecfg，在“函数库”中添加 winhttp 并设为“原装先于内建”（native,builtin）。') }}</p>
            <p>{{ t('如果 Steam 已在运行，完成覆盖设置后请退出并重新启动同一容器内的 Steam。') }}</p>
            <h4>{{ t('MOD 界面缺少文字') }}</h4>
            <p>{{ config?.platform === 'macos'
              ? t('在同一个 CrossOver 容器中安装 Core Fonts；中文显示还可安装中文字体。')
              : t('请在游戏使用的同一 Wine 容器中通过 winetricks 安装 corefonts，不要安装到其他容器。') }}</p>
            <div v-if="config?.platform === 'linux' && fontsCommand" class="bepinex-setup-command">
              <pre>{{ fontsCommand }}</pre>
              <button class="secondary-button small-button" @click="copy(fontsCommand)"><Copy :size="14" />{{ t('复制') }}</button>
            </div>
          </template>
          <p>{{ t('通过 Steam 首次启动后，检查 BepInEx/config/BepInEx.cfg 和 BepInEx/LogOutput.log。') }}</p>
          <p v-if="feedback" role="status" :class="feedbackError ? 'guard-note' : 'bepinex-setup-success'">{{ feedback }}</p>
        </div>
        <footer class="modal-actions">
          <button class="secondary-button" @click="openDocumentation"><ExternalLink :size="15" />{{ t('BepInEx 官方说明') }}</button>
          <button class="primary-button" @click="emit('close')">{{ t('我已完成所需操作') }}</button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>
