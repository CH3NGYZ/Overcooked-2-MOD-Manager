import type { DomainMessages } from './types'

/**
 * 框架状态（BepInEx / DIYLevel）的后端 tag。
 *
 * 与 `framework.ts` 的分工：那边是**界面标签**（按钮、标题、抽屉标题），
 * 这里是后端 `framework.*` tag 的文案（状态判定、冲突提示、切换结果）。
 *
 * 两者最终合并到同一个 `framework` 域，所以这里用**短 key**（`loaderConflict`），
 * 上面用**带对象前缀的长 key**（`bepinex.title`），拼出的完整 tag 不会打架。
 */
export const frameworkBackend: DomainMessages = {
  bepInExDirMissing: {
    'zh-CN': 'BepInEx 目录不存在，请重新检测框架状态',
    'zh-TW': 'BepInEx 目錄不存在，請重新偵測框架狀態',
    en: 'The BepInEx directory is missing. Re-check the framework status.',
    ko: 'BepInEx 디렉터리가 없습니다. 프레임워크 상태를 다시 확인하세요.',
    ja: 'BepInEx ディレクトリがありません。フレームワーク状態を再検出してください。',
  },
  bepInExDirOpened: {
    'zh-CN': '已打开 BepInEx 目录',
    'zh-TW': '已開啟 BepInEx 目錄',
    en: 'Opened the BepInEx directory',
    ko: 'BepInEx 디렉터리를 열었습니다',
    ja: 'BepInEx ディレクトリを開きました',
  },
  bepInExInstalled: {
    'zh-CN': 'BepInEx 安装完成',
    'zh-TW': 'BepInEx 安裝完成',
    en: 'BepInEx installed',
    ko: 'BepInEx 설치 완료',
    ja: 'BepInEx のインストールが完了しました',
  },
  bepInExRequiredBeforeMods: {
    'zh-CN': '请先安装 BepInEx 框架，再安装 MOD',
    'zh-TW': '請先安裝 BepInEx 框架，再安裝 MOD',
    en: 'Install the BepInEx framework before installing mods',
    ko: 'MOD를 설치하기 전에 BepInEx 프레임워크를 먼저 설치하세요',
    ja: 'MOD をインストールする前に BepInEx フレームワークをインストールしてください',
  },
  disabledGameNotRunning: {
    'zh-CN': '框架已禁用，尚未启动厨房',
    'zh-TW': '框架已停用，尚未啟動廚房',
    en: 'Framework disabled; the game has not been launched',
    ko: '프레임워크가 비활성화되었고 주방을 아직 실행하지 않았습니다',
    ja: 'フレームワークは無効です。キッチンはまだ起動していません',
  },
  enabledGameNotRunning: {
    'zh-CN': '框架已启用，尚未启动厨房',
    'zh-TW': '框架已啟用，尚未啟動廚房',
    en: 'Framework enabled; the game has not been launched',
    ko: '프레임워크가 활성화되었고 주방을 아직 실행하지 않았습니다',
    ja: 'フレームワークは有効です。キッチンはまだ起動していません',
  },
  loaderConflict: {
    'zh-CN': 'winhttp.dll 与禁用文件同时存在，请先切换为框架已启用状态',
    'zh-TW': 'winhttp.dll 與停用檔案同時存在，請先切換為框架已啟用狀態',
    en: 'Both winhttp.dll and its disabled copy exist. Switch the framework to enabled first.',
    ko: 'winhttp.dll과 비활성화 파일이 함께 있습니다. 먼저 프레임워크를 활성화하세요.',
    ja: 'winhttp.dll と無効化ファイルが同時に存在します。先にフレームワークを有効にしてください。',
  },
  loaderMissing: {
    'zh-CN': '未找到框架加载器，请先安装框架',
    'zh-TW': '找不到框架載入器，請先安裝框架',
    en: 'Framework loader not found. Install the framework first.',
    ko: '프레임워크 로더를 찾지 못했습니다. 먼저 프레임워크를 설치하세요.',
    ja: 'フレームワークのローダーが見つかりません。先にフレームワークをインストールしてください。',
  },
  nativePackageMissingScript: {
    'zh-CN': '原生 BepInEx 包缺少 run_bepinex.sh',
    'zh-TW': '原生 BepInEx 套件缺少 run_bepinex.sh',
    en: 'The native BepInEx package is missing run_bepinex.sh',
    ko: '네이티브 BepInEx 패키지에 run_bepinex.sh가 없습니다',
    ja: 'ネイティブ BepInEx パッケージに run_bepinex.sh がありません',
  },
  nativeScriptMissing: {
    'zh-CN': '未找到原生 BepInEx 启动脚本，请先安装框架',
    'zh-TW': '找不到原生 BepInEx 啟動腳本，請先安裝框架',
    en: 'The native BepInEx launch script is missing. Install the framework first.',
    ko: '네이티브 BepInEx 실행 스크립트를 찾지 못했습니다. 먼저 프레임워크를 설치하세요.',
    ja: 'ネイティブ BepInEx の起動スクリプトが見つかりません。先にフレームワークをインストールしてください。',
  },
  nativeScriptUnrecognized: {
    'zh-CN': '无法识别原生 BepInEx 启动脚本，请重新安装框架',
    'zh-TW': '無法辨識原生 BepInEx 啟動腳本，請重新安裝框架',
    en: 'The native BepInEx launch script is not recognized. Reinstall the framework.',
    ko: '네이티브 BepInEx 실행 스크립트를 인식할 수 없습니다. 프레임워크를 다시 설치하세요.',
    ja: 'ネイティブ BepInEx の起動スクリプトを認識できません。フレームワークを再インストールしてください。',
  },
  notInstalledCannotToggle: {
    'zh-CN': 'BepInEx 框架未安装，无法切换框架状态',
    'zh-TW': 'BepInEx 框架未安裝，無法切換框架狀態',
    en: 'BepInEx is not installed, so the framework state cannot be toggled',
    ko: 'BepInEx 프레임워크가 설치되지 않아 상태를 전환할 수 없습니다',
    ja: 'BepInEx フレームワークが未インストールのため、状態を切り替えられません',
  },
  overlayMacNativeOnly: {
    'zh-CN': 'Steam Overlay 修复仅适用于 macOS 原生运行模式',
    'zh-TW': 'Steam Overlay 修復僅適用於 macOS 原生執行模式',
    en: 'The Steam Overlay fix only applies to macOS native mode',
    ko: 'Steam Overlay 수정은 macOS 네이티브 실행 모드에서만 적용됩니다',
    ja: 'Steam Overlay の修復は macOS ネイティブ実行モードでのみ有効です',
  },
  overlayRepaired: {
    'zh-CN': 'Steam Overlay 注入修复完成',
    'zh-TW': 'Steam Overlay 注入修復完成',
    en: 'Steam Overlay injection repaired',
    ko: 'Steam Overlay 주입을 복구했습니다',
    ja: 'Steam Overlay の注入を修復しました',
  },
  overlayScriptUnrecognized: {
    'zh-CN': '无法识别 macOS BepInEx 启动脚本，无法修复 Steam Overlay',
    'zh-TW': '無法辨識 macOS BepInEx 啟動腳本，無法修復 Steam Overlay',
    en: 'The macOS BepInEx launch script is not recognized, so Steam Overlay cannot be repaired',
    ko: 'macOS BepInEx 실행 스크립트를 인식할 수 없어 Steam Overlay를 복구할 수 없습니다',
    ja: 'macOS BepInEx の起動スクリプトを認識できないため、Steam Overlay を修復できません',
  },
  stateRestored: {
    'zh-CN': '框架状态已恢复',
    'zh-TW': '框架狀態已還原',
    en: 'Framework state restored',
    ko: '프레임워크 상태를 복구했습니다',
    ja: 'フレームワークの状態を復元しました',
  },
  installFailed: {
    'zh-CN': 'BepInEx 安装失败: {reason}',
    'zh-TW': 'BepInEx 安裝失敗: {reason}',
    en: 'BepInEx installation failed: {reason}',
    ko: 'BepInEx 설치 실패: {reason}',
    ja: 'BepInEx のインストールに失敗しました: {reason}',
  },
  diyLevelStructureUnexpected: {
    'zh-CN': '尚未安装 DIYLevel 框架，或安装结构与预期不同',
    'zh-TW': '尚未安裝 DIYLevel 框架，或安裝結構與預期不同',
    en: 'The DIYLevel framework is not installed, or its layout is not what was expected',
    ko: 'DIYLevel 프레임워크가 설치되지 않았거나 설치 구조가 예상과 다릅니다',
    ja: 'DIYLevel フレームワークが未インストールか、構成が想定と異なります',
  },
  bepInExManifestMissing: {
    'zh-CN': '未找到 BepInEx 安装清单',
    'zh-TW': '找不到 BepInEx 安裝清單',
    en: 'The BepInEx install manifest was not found',
    ko: 'BepInEx 설치 목록을 찾지 못했습니다',
    ja: 'BepInEx のインストール記録が見つかりません',
  },
  diyLevelEnabled: {
    'zh-CN': 'DIYLevel 框架已启用',
    'zh-TW': 'DIYLevel 框架已啟用',
    en: 'DIYLevel framework enabled',
    ko: 'DIYLevel 프레임워크가 활성화되었습니다',
    ja: 'DIYLevel フレームワークを有効にしました',
  },
  diyLevelDisabled: {
    'zh-CN': 'DIYLevel 框架已禁用',
    'zh-TW': 'DIYLevel 框架已停用',
    en: 'DIYLevel framework disabled',
    ko: 'DIYLevel 프레임워크가 비활성화되었습니다',
    ja: 'DIYLevel フレームワークを無効にしました',
  },
  diyLevelDirOpened: {
    'zh-CN': '已打开 DIYLevel 目录',
    'zh-TW': '已開啟 DIYLevel 目錄',
    en: 'Opened the DIYLevel directory',
    ko: 'DIYLevel 디렉터리를 열었습니다',
    ja: 'DIYLevel ディレクトリを開きました',
  },
}
