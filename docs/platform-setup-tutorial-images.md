# 平台设置教程图片准备文档

本文档说明跨平台 MOD 管理器所需的教程图片规格、命名格式和内容要求。

## 图片存放位置

所有教程图片存放在 `src/assets/platform-tutorials/` 目录下,按平台分类:

```
src/assets/platform-tutorials/
├── linux/
│   ├── steam-properties-zh-CN.png
│   ├── steam-properties-zh-TW.png
│   ├── steam-properties-en.png
│   ├── steam-properties-ko.png
│   ├── launch-options-zh-CN.png
│   ├── launch-options-zh-TW.png
│   ├── launch-options-en.png
│   ├── launch-options-ko.png
│   └── command-example-zh-CN.png (等)
└── macos/
    ├── crossover-create-bottle-zh-CN.png
    ├── crossover-create-bottle-zh-TW.png
    ├── crossover-create-bottle-en.png
    ├── crossover-create-bottle-ko.png
    └── (其他步骤图片)
```

## 命名规范

### 格式
```
{操作步骤}-{语言代码}.png
```

### 语言代码
- `zh-CN`: 简体中文
- `zh-TW`: 繁體中文
- `en`: English
- `ko`: 한국어

### 示例
- `steam-properties-zh-CN.png` - Steam 属性界面的简体中文截图
- `launch-options-en.png` - 启动选项输入框的英文截图

## 图片规格要求

### 技术规格
- **格式**: PNG (支持透明背景)
- **色彩模式**: RGB
- **最大宽度**: 1200px
- **推荐分辨率**: 72-96 DPI
- **文件大小**: 单张不超过 500KB

### 内容要求
1. **清晰度**: 截图必须清晰,文字可读
2. **标注**: 关键操作区域使用红色方框或箭头标注
3. **隐私**: 不包含个人账号、真实用户名等敏感信息
4. **语言一致性**: 截图界面语言与文件名语言代码一致
5. **简洁性**: 只截取必要的操作区域,不包含无关内容

## Linux 平台教程图片清单（后续制作）

本轮不启用完整 Linux 图文教程，只提供框架安装后的 Steam/Wine 启动设置。Linux 默认 Wine；以下原生 Steam 截图清单是后续制作材料，不代表软件已经展示这些步骤。当前模式、包选择和验收规则见 [跨平台测试指南](cross-platform-testing.md)。

### 1. Steam 游戏属性入口
**文件名**: `steam-properties-{语言}.png`

**内容要求**:
- Steam 库界面
- 鼠标悬停或右键菜单显示"属性"选项
- 用红框标注"属性"菜单项

**文字说明** (前端显示,不在图片内):
- zh-CN: "在 Steam 库中找到《胡闹厨房2》,右键点击游戏,选择"属性""
- zh-TW: "在 Steam 庫中找到《胡鬧廚房2》,右鍵點選遊戲,選擇「內容」"
- en: "Find Overcooked! 2 in your Steam library, right-click the game, and select 'Properties'"
- ko: "Steam 라이브러리에서 《Overcooked! 2》를 찾아 마우스 오른쪽 버튼으로 클릭하고 '속성'을 선택하세요"

### 2. 启动选项输入框位置
**文件名**: `launch-options-{语言}.png`

**内容要求**:
- Steam 游戏属性窗口
- "启动选项"输入框清晰可见
- 用红框或箭头标注输入框位置

**文字说明**:
- zh-CN: "在属性窗口中找到"启动选项"输入框"
- zh-TW: "在內容視窗中找到「啟動選項」輸入框"
- en: "Locate the 'Launch Options' input field in the Properties window"
- ko: "속성 창에서 '실행 옵션' 입력란을 찾으세요"

### 3. 启动参数命令示例
**文件名**: `command-example-{语言}.png`

**内容要求**:
- 启动选项输入框
- 已填入的完整 BepInEx 启动命令
- 命令文本清晰可读

**示例命令** (实际命令可能需要根据 BepInEx 版本调整):
```
./run_bepinex.sh %command%
```

**文字说明**:
- zh-CN: "在启动选项中输入以下命令(可点击复制按钮):"
- zh-TW: "在啟動選項中輸入以下指令(可點選複製按鈕):"
- en: "Enter the following command in Launch Options (click the copy button):"
- ko: "실행 옵션에 다음 명령어를 입력하세요 (복사 버튼 클릭):"

### 4. Steam 启动选项保存确认
**文件名**: `save-confirmation-{语言}.png` (可选)

**内容要求**:
- 输入命令后的启动选项界面
- 关闭按钮或确认按钮
- 用绿色勾标注已正确填写

## macOS Wine/CrossOver 平台教程图片清单

CrossOver 向导仅适用于 Wine 模式；macOS 原生使用原生 Steam 和 `Overcooked! 2.app`，不显示这些 CrossOver 步骤。

### 1. Crossover 是否运行检测
**文件名**: `crossover-running-check-{语言}.png`

**内容要求**:
- macOS 活动监视器或 Dock 栏
- Crossover 应用图标
- 标注 Crossover 已运行状态

**文字说明**:
- zh-CN: "确保 Crossover 已经启动运行"
- zh-TW: "確保 Crossover 已經啟動執行"
- en: "Make sure Crossover is running"
- ko: "Crossover가 실행 중인지 확인하세요"

### 2. Crossover 创建容器
**文件名**: `crossover-create-bottle-{语言}.png`

**内容要求**:
- Crossover 主界面
- "新建容器"或"Install Windows Software"按钮
- 用红框标注创建按钮

**文字说明**:
- zh-CN: "打开 Crossover,点击"安装 Windows 软件"或创建新容器"
- zh-TW: "開啟 Crossover,點選「安裝 Windows 軟體」或建立新容器"
- en: "Open Crossover and click 'Install Windows Software' or create a new bottle"
- ko: "Crossover를 열고 'Install Windows Software'를 클릭하거나 새 병을 만드세요"

### 3. 在 Crossover 容器内安装 Steam
**文件名**: `crossover-install-steam-{语言}.png`

**内容要求**:
- Crossover 软件安装向导
- Steam 安装程序选项
- 或显示下载 Steam 安装程序的界面

**文字说明**:
- zh-CN: "在容器内安装 Steam 客户端"
- zh-TW: "在容器內安裝 Steam 用戶端"
- en: "Install Steam client inside the bottle"
- ko: "병 안에 Steam 클라이언트를 설치하세요"

### 4. Steam 内安装《胡闹厨房2》
**文件名**: `steam-install-game-{语言}.png`

**内容要求**:
- Crossover 容器内运行的 Steam 界面
- 《胡闹厨房2》商店页面或库页面
- 安装按钮

**文字说明**:
- zh-CN: "在 Steam 中搜索并安装《胡闹厨房2》(Overcooked! 2)"
- zh-TW: "在 Steam 中搜尋並安裝《胡鬧廚房2》(Overcooked! 2)"
- en: "Search for and install Overcooked! 2 in Steam"
- ko: "Steam에서 《Overcooked! 2》를 검색하고 설치하세요"

### 5. 游戏目录位置示例
**文件名**: `game-directory-example-{语言}.png`

**内容要求**:
- Finder 窗口显示游戏安装路径
- 完整路径可见: `~/Library/Application Support/CrossOver/Bottles/{容器名}/drive_c/...`
- 标注容器中的 `Overcooked2.exe` 位置，不使用原生 `.app` 路径

**文字说明**:
- zh-CN: "游戏通常安装在 Crossover 容器的以下位置:"
- zh-TW: "遊戲通常安裝在 Crossover 容器的以下位置:"
- en: "The game is typically installed at the following location in the Crossover bottle:"
- ko: "게임은 일반적으로 Crossover 병의 다음 위치에 설치됩니다:"

### 6. BepInEx 安装完成确认
**文件名**: `bepinex-installed-{语言}.png`

**内容要求**:
- Finder 显示游戏目录
- BepInEx 文件夹已存在
- 用绿色勾标注 BepInEx 目录

**文字说明**:
- zh-CN: "确认 BepInEx 文件夹已正确安装到游戏目录"
- zh-TW: "確認 BepInEx 資料夾已正確安裝到遊戲目錄"
- en: "Confirm that the BepInEx folder is correctly installed in the game directory"
- ko: "BepInEx 폴더가 게임 디렉토리에 올바르게 설치되었는지 확인하세요"

## 图片后期处理

### 标注规范
1. **红色方框**: `#FF0000`, 线宽 3-4px, 用于标注关键操作区域
2. **箭头**: 红色实心箭头, 用于指示操作方向
3. **绿色勾**: `#00CC00`, 用于标注完成状态
4. **文字标注**: 使用无衬线字体(如 Arial), 14-16pt, 黑色文字白色描边

### 推荐工具
- **截图**: macOS 自带截图 (Cmd+Shift+4) 或 Linux 的 gnome-screenshot
- **标注**: GIMP, Krita, 或在线工具如 Photopea
- **压缩**: TinyPNG, ImageOptim

## 前端集成说明

这些图片将通过前端的 `PlatformSetupGuide.vue` 组件动态加载:

```typescript
import steamPropertiesZhCN from '@/assets/platform-tutorials/linux/steam-properties-zh-CN.png'
import steamPropertiesEn from '@/assets/platform-tutorials/linux/steam-properties-en.png'
// ...

const tutorialImages = {
  linux: {
    steamProperties: {
      'zh-CN': steamPropertiesZhCN,
      'en': steamPropertiesEn,
      // ...
    },
    // ...
  },
  macos: {
    // ...
  }
}
```

## 版权声明

所有教程图片的截图内容版权归原软件所有者(Valve Corporation, CodeWeavers Inc., 等)所有。
本项目仅用于教学和指导目的,属于合理使用范围。

## 更新记录

- 2026-10-12: 初始文档创建
