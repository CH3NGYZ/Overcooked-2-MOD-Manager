<template>
  <div v-if="shouldShowGuide" class="platform-setup-overlay" @click="handleOverlayClick">
    <div class="platform-setup-dialog" @click.stop>
      <div class="platform-setup-header">
        <h2>{{ t('平台设置向导') }}</h2>
        <button class="close-button" @click="closeGuide" :title="t('关闭')">×</button>
      </div>

      <div class="platform-setup-content">
        <!-- Linux Setup Guide -->
        <template v-if="currentPlatform === 'linux'">
          <div class="setup-step">
            <h3>{{ t('步骤 1：安装 BepInEx 框架') }}</h3>
            <p>{{ t('使用管理器安装 BepInEx 框架到游戏目录') }}</p>
          </div>

          <div class="setup-step">
            <h3>{{ t('步骤 2：配置 Steam 启动参数') }}</h3>
            <p>{{ t('在 Steam 中右键游戏，选择"属性"，在"启动选项"中输入以下命令：') }}</p>
            <div class="command-box">
              <pre>{{ linuxLaunchCommand }}</pre>
              <button @click="copyCommand" class="copy-button">
                {{ copied ? t('已复制') : t('复制') }}
              </button>
            </div>
          </div>

          <div class="setup-step">
            <h3>{{ t('步骤 3：启动游戏测试') }}</h3>
            <p>{{ t('通过 Steam 启动游戏，确认 BepInEx 框架正常加载') }}</p>
          </div>
        </template>

        <!-- macOS Setup Guide -->
        <template v-if="currentPlatform === 'darwin'">
          <!-- Step 1: UU Accelerator -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 1：下载 UU 加速器') }}</h3>
            <p>{{ t('下载 UU 加速器，使用手机号登录后加速 Steam 平台，之后放到后台即可。') }}</p>
          </div>

          <!-- Step 2: Download CrossOver -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 2：下载 CrossOver') }}</h3>
            <p>{{ t('加 QQ 群 1103401780，下载群文件 CrossOver，输入密码 digit77.com（输入两次）后打开安装页面。') }}</p>
            <p>{{ t('将 CrossOver 拖到 Applications 文件夹，等待复制完毕。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Drag crossover to applications.png')"
                 :src="getTutorialImagePath('MacOS/Drag crossover to applications.png')"
                 alt="Drag CrossOver to Applications"
                 class="tutorial-image" />
          </div>

          <!-- Step 3: CROSLIC folder -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 3：准备 CROSLIC 文件夹') }}</h3>
            <p>{{ t('将 CROSLIC 文件夹拖动到桌面。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Drag croslic folder to desktop.png')"
                 :src="getTutorialImagePath('MacOS/Drag croslic folder to desktop.png')"
                 alt="Drag CROSLIC to Desktop"
                 class="tutorial-image" />
          </div>

          <!-- Step 4: Terminal cd -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 4：终端进入 CROSLIC 目录') }}</h3>
            <p>{{ t('在启动台打开终端，输入 cd（空格），然后将桌面的 croslic 文件夹拖入终端，最后回车。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Drag croslic folder to terminal.png')"
                 :src="getTutorialImagePath('MacOS/Drag croslic folder to terminal.png')"
                 alt="Drag CROSLIC folder to Terminal"
                 class="tutorial-image" />
          </div>

          <!-- Step 5: Run license.tool -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 5：执行授权工具') }}</h3>
            <p>{{ t('打开 croslic 文件夹，将 license.tool 文件拖到终端并回车。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Drag license.tool to Terminal.png')"
                 :src="getTutorialImagePath('MacOS/Drag license.tool to Terminal.png')"
                 alt="Drag license.tool to Terminal"
                 class="tutorial-image" />
          </div>

          <!-- Step 6: Password -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 6：输入密码') }}</h3>
            <p>{{ t('输入两次电脑密码，最后出现 "replacing existing signature" 即 CrossOver 注入完毕。') }}</p>
          </div>

          <!-- Step 7: Security Settings -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 7：允许运行 CrossOver') }}</h3>
            <p>{{ t('关闭终端和文件夹，打开 CrossOver。若提示无法打开，按以下步骤：') }}</p>
            <img v-if="hasTutorialImage('MacOS/Can\'t open crossover.png')"
                 :src="getTutorialImagePath('MacOS/Can\'t open crossover.png')"
                 alt="Can't open CrossOver warning"
                 class="tutorial-image" />
            <ol>
              <li>{{ t('点击"完成"') }}</li>
              <li>{{ t('去"设置"打开"隐私与安全性"') }}</li>
              <li>{{ t('滑到下方，针对 CrossOver 点击"仍要允许"并输入 Touch ID 或密码') }}</li>
            </ol>
            <img v-if="hasTutorialImage('MacOS/Use Touch ID or password to open Crossover.png')"
                 :src="getTutorialImagePath('MacOS/Use Touch ID or password to open Crossover.png')"
                 alt="Allow CrossOver in Security Settings"
                 class="tutorial-image" />
          </div>

          <!-- Step 8: Download Steam -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 8：下载 Windows 版 Steam') }}</h3>
            <p>{{ t('打开 CrossOver 后，先下载 Windows 版 Steam 安装包：https://store.steampowered.com，选择 Windows 平台。下载后备用。') }}</p>
          </div>

          <!-- Step 9: Install to CrossOver -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 9：在 CrossOver 中安装 Steam') }}</h3>
            <p>{{ t('切换到 CrossOver 主界面，右上角点击"安装一个不在列表里的应用程序"。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Install an application.png')"
                 :src="getTutorialImagePath('MacOS/Install an application.png')"
                 alt="Install an application"
                 class="tutorial-image" />
          </div>

          <!-- Step 10: Select installer -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 10：选择 Steam 安装包') }}</h3>
            <p>{{ t('在"详情"的"提供安装程序"处，选择右侧的"编辑"，选择刚下载的 SteamSetup.exe。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Details select steam setup.png')"
                 :src="getTutorialImagePath('MacOS/Details select steam setup.png')"
                 alt="Select Steam setup"
                 class="tutorial-image" />
          </div>

          <!-- Step 11: Create bottle -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 11：创建容器') }}</h3>
            <p>{{ t('在"安装容器"处点击"编辑"，选择"新建容器"，命名为"厨房"，类型选 Windows 10。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Create a bottle.png')"
                 :src="getTutorialImagePath('MacOS/Create a bottle.png')"
                 alt="Create a bottle"
                 class="tutorial-image" />
          </div>

          <!-- Step 12: Install Steam -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 12：完成 Steam 安装') }}</h3>
            <p>{{ t('点击右上角"安装"，授予必要权限。出现 Steam 安装界面时一路下一步，安装完成时取消勾选"运行 Steam"。') }}</p>
          </div>

          <!-- Step 13: Install to bottle -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 13：安装应用到容器') }}</h3>
            <p>{{ t('容器和 Steam 已安装完毕。选择右侧"安装应用程序到容器"按钮。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Install an application to Bottles.png')"
                 :src="getTutorialImagePath('MacOS/Install an application to Bottles.png')"
                 alt="Install application to bottles"
                 class="tutorial-image" />
          </div>

          <!-- Step 14: Chinese support -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 14：安装中文支持') }}</h3>
            <p>{{ t('在左上角搜索"中文"，安装将容器语言切换到简体中文的软件。再次进行同样步骤安装"微软雅黑"。') }}</p>
          </div>

          <!-- Step 15: Wine configuration -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 15：配置 Wine') }}</h3>
            <p>{{ t('所有软件安装完毕后，在容器界面选择"Wine 配置"。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Click Wine configuration.png')"
                 :src="getTutorialImagePath('MacOS/Click Wine configuration.png')"
                 alt="Wine configuration"
                 class="tutorial-image" />
          </div>

          <!-- Step 16: Add winhttp -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 16：添加 winhttp 库') }}</h3>
            <p>{{ t('切换到"函数库"，在下拉列表选择"winhttp"并点击右侧"添加"。确认已添加的 winhttp 在列表内且括号内是"原装先于内建"后，点击"应用"并确定，关闭 Wine 配置。') }}</p>
            <img v-if="hasTutorialImage('MacOS/Execute Wine configuration.png')"
                 :src="getTutorialImagePath('MacOS/Execute Wine configuration.png')"
                 alt="Execute Wine configuration"
                 class="tutorial-image" />
          </div>

          <!-- Step 17: Run Steam and download game -->
          <div class="setup-step">
            <h3>{{ t('macOS 步骤 17：下载游戏') }}</h3>
            <p>{{ t('回到容器界面，运行 Steam，登录并下载《胡闹厨房2》。下载完毕后，在此管理器中刷新游戏目录。若成功获取游戏目录，即可安装框架和 MOD。') }}</p>
            <div class="info-box">
              <strong>{{ t('提示：') }}</strong>
              {{ t('macOS 获取的游戏目录为 CrossOver 容器内路径，例如：/Users/用户名/Library/Application Support/CrossOver/Bottles/厨房/drive_c/Program Files (x86)/Steam/steamapps/common/Overcooked! 2') }}
            </div>
          </div>
        </template>
      </div>

      <div class="platform-setup-footer">
        <button @click="markCompleted" class="primary-button">
          {{ t('我已完成所需操作') }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { t } from '../i18n'
import { api } from '../services/tauri'

const props = defineProps<{
  visible: boolean
  onCompleted?: () => void
}>()

const emit = defineEmits<{
  close: []
  completed: []
}>()

const currentPlatform = ref<string>('')
const crossoverRunning = ref(false)
const copied = ref(false)

const shouldShowGuide = computed(() => {
  return props.visible && (currentPlatform.value === 'linux' || currentPlatform.value === 'darwin')
})

const linuxLaunchCommand = computed(() => {
  return 'DOORSTOP_ENABLE=1 DOORSTOP_INVOKE_DLL_PATH="./BepInEx/core/BepInEx.Preloader.dll" %command%'
})

async function detectPlatform() {
  // Tauri platform detection
  const { platform } = await import('@tauri-apps/plugin-os')
  currentPlatform.value = platform()
}

async function checkCrossover() {
  if (currentPlatform.value === 'darwin') {
    try {
      crossoverRunning.value = await api.checkCrossoverRunning()
    } catch (error) {
      console.error('Failed to check CrossOver status:', error)
    }
  }
}

function copyCommand() {
  navigator.clipboard.writeText(linuxLaunchCommand.value)
  copied.value = true
  setTimeout(() => {
    copied.value = false
  }, 2000)
}

function closeGuide() {
  emit('close')
}

function handleOverlayClick() {
  // Allow clicking overlay to close
  closeGuide()
}

function markCompleted() {
  emit('completed')
  if (props.onCompleted) {
    props.onCompleted()
  }
  closeGuide()
}

function getTutorialImagePath(imageName: string): string {
  return new URL(`../assets/tutorial/${imageName}`, import.meta.url).href
}

function hasTutorialImage(imageName: string): boolean {
  try {
    getTutorialImagePath(imageName)
    return true
  } catch {
    return false
  }
}

onMounted(async () => {
  await detectPlatform()
  await checkCrossover()
})
</script>

<style scoped>
.platform-setup-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.75);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10000;
  padding: 20px;
}

.platform-setup-dialog {
  background: #1e1e1e;
  border-radius: 8px;
  max-width: 800px;
  width: 100%;
  max-height: 90vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.5);
}

.platform-setup-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 20px 24px;
  border-bottom: 1px solid #333;
}

.platform-setup-header h2 {
  margin: 0;
  font-size: 20px;
  color: #fff;
}

.close-button {
  background: none;
  border: none;
  color: #888;
  font-size: 32px;
  line-height: 1;
  cursor: pointer;
  padding: 0;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-button:hover {
  color: #fff;
}

.platform-setup-content {
  flex: 1;
  overflow-y: auto;
  padding: 24px;
}

.setup-step {
  margin-bottom: 24px;
}

.setup-step h3 {
  margin: 0 0 12px 0;
  color: #4a9eff;
  font-size: 16px;
}

.setup-step p {
  margin: 0 0 12px 0;
  color: #ccc;
  line-height: 1.6;
}

.setup-step ol {
  margin: 12px 0;
  padding-left: 24px;
  color: #ccc;
}

.setup-step ol li {
  margin-bottom: 8px;
  line-height: 1.6;
}

.command-box {
  background: #2a2a2a;
  border: 1px solid #444;
  border-radius: 4px;
  padding: 12px;
  margin-top: 12px;
  position: relative;
}

.command-box pre {
  margin: 0;
  color: #a5d6ff;
  font-family: 'Consolas', 'Monaco', monospace;
  font-size: 13px;
  white-space: pre-wrap;
  word-break: break-all;
}

.copy-button {
  position: absolute;
  top: 8px;
  right: 8px;
  background: #4a9eff;
  border: none;
  color: #fff;
  padding: 4px 12px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
}

.copy-button:hover {
  background: #3a8eef;
}

.warning-box {
  background: #3a2a00;
  border: 1px solid #8b6914;
  border-radius: 4px;
  padding: 12px;
  margin-top: 12px;
  color: #ffcc00;
}

.success-box {
  background: #0f3a1a;
  border: 1px solid #2a8b4a;
  border-radius: 4px;
  padding: 12px;
  margin-top: 12px;
  color: #66ff99;
}

.platform-setup-footer {
  padding: 20px 24px;
  border-top: 1px solid #333;
  display: flex;
  justify-content: flex-end;
}

.primary-button {
  background: #4a9eff;
  border: none;
  color: #fff;
  padding: 10px 24px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
}

.primary-button:hover {
  background: #3a8eef;
}

.tutorial-image {
  max-width: 100%;
  height: auto;
  margin: 12px 0;
  border: 1px solid #444;
  border-radius: 4px;
}

.info-box {
  background: #1a2a3a;
  border: 1px solid #2a4a6a;
  border-radius: 4px;
  padding: 12px;
  margin-top: 12px;
  color: #99ccff;
  line-height: 1.6;
}

.info-box strong {
  color: #66aaff;
}
</style>
