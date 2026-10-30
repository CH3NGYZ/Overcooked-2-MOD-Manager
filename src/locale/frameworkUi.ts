import type { DomainMessages } from './types'

/**
 * framework 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const frameworkUi: DomainMessages = {
  // BepInEx 框架
  bepinexFramework: {
    'zh-CN': 'BepInEx 框架',
    'zh-TW': 'BepInEx 框架',
    en: 'BepInEx framework',
    ko: 'BepInEx 프레임워크',
    ja: 'BepInEx フレームワーク',
  },
  // BepInEx 框架管理
  bepinexFramework2: {
    'zh-CN': 'BepInEx 框架管理',
    'zh-TW': 'BepInEx 框架管理',
    en: 'BepInEx framework',
    ko: 'BepInEx 프레임워크 관리',
    ja: 'BepInEx フレームワーク管理',
  },
  // BepInEx 配置
  bepinexConfig: {
    'zh-CN': 'BepInEx 配置',
    'zh-TW': 'BepInEx 設定',
    en: 'BepInEx config',
    ko: 'BepInEx 설정',
    ja: 'BepInEx 設定',
  },
  // 配置管理
  manageConfig: {
    'zh-CN': '配置管理',
    'zh-TW': '設定管理',
    en: 'Manage config',
    ko: '설정 관리',
    ja: '設定を管理',
  },
  // 运行形态
  runtime: {
    'zh-CN': '运行形态',
    'zh-TW': '執行形態',
    en: 'Runtime',
    ko: '실행 형태',
    ja: '実行形態',
  },
  // 原生
  native: {
    'zh-CN': '原生',
    'zh-TW': '原生',
    en: 'Native',
    ko: '네이티브',
    ja: 'ネイティブ',
  },
  // 游戏版本
  gameVersion: {
    'zh-CN': '游戏版本',
    'zh-TW': '遊戲版本',
    en: 'Game version',
    ko: '게임 버전',
    ja: 'ゲームのバージョン',
  },
  // CrossOver 运行 Windows 版，或使用 macOS 原生版
  runTheWindowsBuildThroughCrossoverOrUseTheNativeMacosBuild: {
    'zh-CN': 'CrossOver 运行 Windows 版，或使用 macOS 原生版',
    'zh-TW': 'CrossOver 執行 Windows 版，或使用 macOS 原生版',
    en: 'Run the Windows build through CrossOver, or use the native macOS build',
    ko: 'CrossOver로 Windows 버전을 실행하거나 macOS 네이티브 버전을 사용합니다',
    ja: 'CrossOver で Windows 版を実行するか、macOS ネイティブ版を使用します',
  },
  // 尚未安装，点击右侧按钮安装
  notInstalledUseTheButtonOnTheRight: {
    'zh-CN': '尚未安装，点击右侧按钮安装',
    'zh-TW': '尚未安裝，點擊右側按鈕安裝',
    en: 'Not installed — use the button on the right',
    ko: '설치되지 않았습니다. 오른쪽 버튼으로 설치하세요',
    ja: '未インストールです。右のボタンからインストールしてください',
  },
  // 尚未安装框架，检测到已有的 plugins / config，安装后会保留
  frameworkNotInstalledExistingPluginsConfigFoundAndWillBeKeptAfterInstall: {
    'zh-CN': '尚未安装框架，检测到已有的 plugins / config，安装后会保留',
    'zh-TW': '尚未安裝框架，偵測到既有的 plugins / config，安裝後會保留',
    en: 'Framework not installed. Existing plugins / config found and will be kept after install.',
    ko: '프레임워크가 설치되지 않았습니다. 기존 plugins / config 가 있으며 설치 후 유지됩니다.',
    ja: 'フレームワーク未インストール。既存の plugins / config はインストール後も保持されます。',
  },
  // 管理
  manage: {
    'zh-CN': '管理',
    'zh-TW': '管理',
    en: 'Manage',
    ko: '관리',
    ja: '管理',
  },
  // 安装前准备
  beforeInstalling: {
    'zh-CN': '安装前准备',
    'zh-TW': '安裝前準備',
    en: 'Before installing',
    ko: '설치 전 준비',
    ja: 'インストール前の準備',
  },
  // 还需要一步
  oneMoreStep: {
    'zh-CN': '还需要一步',
    'zh-TW': '還需要一步',
    en: 'One more step',
    ko: '한 단계 더',
    ja: 'あと 1 ステップ',
  },
  // 准备 CrossOver 容器
  prepareTheCrossoverBottle: {
    'zh-CN': '准备 CrossOver 容器',
    'zh-TW': '準備 CrossOver 容器',
    en: 'Prepare the CrossOver bottle',
    ko: 'CrossOver 병 준비',
    ja: 'CrossOver ボトルを準備',
  },
  // 确认游戏能在 CrossOver 中正常启动
  makeSureTheGameLaunchesInCrossover: {
    'zh-CN': '确认游戏能在 CrossOver 中正常启动',
    'zh-TW': '確認遊戲能在 CrossOver 中正常啟動',
    en: 'Make sure the game launches in CrossOver',
    ko: 'CrossOver에서 게임이 정상 실행되는지 확인',
    ja: 'CrossOver でゲームが起動することを確認',
  },
  // 安装字体
  installFonts: {
    'zh-CN': '安装字体',
    'zh-TW': '安裝字型',
    en: 'Install fonts',
    ko: '글꼴 설치',
    ja: 'フォントをインストール',
  },
  // 补上 corefonts，否则 BepInEx 控制台会显示乱码
  addCorefontsOrTheBepinexConsoleShowsGarbledText: {
    'zh-CN': '补上 corefonts，否则 BepInEx 控制台会显示乱码',
    'zh-TW': '補上 corefonts，否則 BepInEx 主控台會顯示亂碼',
    en: 'Add corefonts or the BepInEx console shows garbled text',
    ko: 'corefonts가 없으면 BepInEx 콘솔이 깨져 보입니다',
    ja: 'corefonts がないと BepInEx コンソールが文字化けします',
  },
  // 安装 BepInEx
  installBepinex: {
    'zh-CN': '安装 BepInEx',
    'zh-TW': '安裝 BepInEx',
    en: 'Install BepInEx',
    ko: 'BepInEx 설치',
    ja: 'BepInEx をインストール',
  },
  // 将把 BepInEx 框架文件（不含 plugins 与 config）移入回收站，可在回收站中恢复。继续卸载？
  theBepinexFrameworkFilesExcludingPluginsAndConfigWillBeMovedToTheRecycleBinAndCanBeRestoredContinueUninstalling: {
    'zh-CN': '将把 BepInEx 框架文件（不含 plugins 与 config）移入回收站，可在回收站中恢复。继续卸载？',
    'zh-TW': '將把 BepInEx 框架檔案（不含 plugins 與 config）移入資源回收筒，可在資源回收筒中還原。繼續解除安裝？',
    en: 'The BepInEx framework files (excluding plugins and config) will be moved to the recycle bin and can be restored. Continue uninstalling?',
    ko: 'BepInEx 프레임워크 파일(plugins·config 제외)을 휴지통으로 옮깁니다. 복원할 수 있습니다. 제거할까요?',
    ja: 'BepInEx フレームワークのファイル（plugins・config を除く）をゴミ箱へ移動します（復元可能）。アンインストールを続行しますか？',
  },
  // DIYLevel 框架
  diylevelFramework: {
    'zh-CN': 'DIYLevel 框架',
    'zh-TW': 'DIYLevel 框架',
    en: 'DIYLevel framework',
    ko: 'DIYLevel 프레임워크',
    ja: 'DIYLevel フレームワーク',
  },
  // DIYLevel 框架管理
  diylevelFramework2: {
    'zh-CN': 'DIYLevel 框架管理',
    'zh-TW': 'DIYLevel 框架管理',
    en: 'DIYLevel framework',
    ko: 'DIYLevel 프레임워크 관리',
    ja: 'DIYLevel フレームワーク管理',
  },
  // 地图集
  mapPacks: {
    'zh-CN': '地图集',
    'zh-TW': '地圖集',
    en: 'Map packs',
    ko: '맵 팩',
    ja: 'マップパック',
  },
  // 安装来源
  installSource: {
    'zh-CN': '安装来源',
    'zh-TW': '安裝來源',
    en: 'Install source',
    ko: '설치 소스',
    ja: 'インストール元',
  },
  // 未找到可在线安装的仓库
  noRepositoryCanInstallItOnline: {
    'zh-CN': '未找到可在线安装的仓库',
    'zh-TW': '找不到可線上安裝的倉庫',
    en: 'No repository can install it online',
    ko: '온라인 설치가 가능한 저장소가 없습니다',
    ja: 'オンラインインストールできるリポジトリがありません',
  },
  // 打开地图目录
  openMapFolder: {
    'zh-CN': '打开地图目录',
    'zh-TW': '開啟地圖目錄',
    en: 'Open map folder',
    ko: '맵 폴더 열기',
    ja: 'マップフォルダーを開く',
  },
  // 无法在线安装：MOD 安装页没有声明此 GUID 的仓库，请手动安装后回到这里管理。
  notAvailableOnlineNoRepositoryDeclaresThisGuidInstallItManuallyThenManageItHere: {
    'zh-CN': '无法在线安装：MOD 安装页没有声明此 GUID 的仓库，请手动安装后回到这里管理。',
    'zh-TW': '無法線上安裝：MOD 安裝頁沒有宣告此 GUID 的倉庫，請手動安裝後回到這裡管理。',
    en: 'Not available online: no repository declares this GUID. Install it manually, then manage it here.',
    ko: '온라인 설치 불가: 이 GUID를 선언한 저장소가 없습니다. 직접 설치한 뒤 여기로 돌아와 관리하세요.',
    ja: 'オンラインインストール不可：この GUID を宣言したリポジトリがありません。手動でインストールしてからここで管理してください。',
  },
  // 安装后才能加载自制地图
  customMapsLoadOnlyAfterInstallingThis: {
    'zh-CN': '安装后才能加载自制地图',
    'zh-TW': '安裝後才能載入自製地圖',
    en: 'Custom maps load only after installing this',
    ko: '설치해야 커스텀 맵을 불러올 수 있습니다',
    ja: 'インストール後に自作マップを読み込めます',
  },
  // 无法在线安装 DIYLevel，请手动安装后在这里管理
  diylevelIsNotAvailableOnlineInstallItManuallyThenManageItHere: {
    'zh-CN': '无法在线安装 DIYLevel，请手动安装后在这里管理',
    'zh-TW': '無法線上安裝 DIYLevel，請手動安裝後在這裡管理',
    en: 'DIYLevel is not available online. Install it manually, then manage it here.',
    ko: 'DIYLevel은 온라인 설치가 불가능합니다. 직접 설치한 뒤 여기에서 관리하세요.',
    ja: 'DIYLevel はオンラインインストールできません。手動でインストールしてからここで管理してください。',
  },
  // 卸载 DIYLevel 框架
  uninstallDiylevelFramework: {
    'zh-CN': '卸载 DIYLevel 框架',
    'zh-TW': '解除安裝 DIYLevel 框架',
    en: 'Uninstall DIYLevel framework',
    ko: 'DIYLevel 프레임워크 제거',
    ja: 'DIYLevel フレームワークをアンインストール',
  },
  // 卸载前会自动备份整个 DIYLevel 目录。其中的自制地图文件会一并移入回收站。继续？
  theWholeDiylevelFolderIsBackedUpBeforeUninstallingCustomMapsInsideAreMovedToTheRecycleBinTooContinue: {
    'zh-CN': '卸载前会自动备份整个 DIYLevel 目录。其中的自制地图文件会一并移入回收站。继续？',
    'zh-TW': '解除安裝前會自動備份整個 DIYLevel 目錄。其中的自製地圖檔案會一併移入資源回收筒。繼續？',
    en: 'The whole DIYLevel folder is backed up before uninstalling. Custom maps inside are moved to the recycle bin too. Continue?',
    ko: '제거 전에 DIYLevel 폴더 전체를 자동으로 백업합니다. 안의 커스텀 맵도 휴지통으로 이동합니다. 계속할까요?',
    ja: 'アンインストール前に DIYLevel フォルダー全体を自動でバックアップします。中の自作マップもゴミ箱へ移動します。続行しますか？',
  },
  // 正在备份并卸载 DIYLevel…
  backingUpAndUninstallingDiylevel: {
    'zh-CN': '正在备份并卸载 DIYLevel…',
    'zh-TW': '正在備份並解除安裝 DIYLevel…',
    en: 'Backing up and uninstalling DIYLevel…',
    ko: 'DIYLevel 백업 및 제거 중…',
    ja: 'DIYLevel をバックアップしてアンインストール中…',
  },
  // 正在备份 DIYLevel 目录…
  backingUpTheDiylevelFolder: {
    'zh-CN': '正在备份 DIYLevel 目录…',
    'zh-TW': '正在備份 DIYLevel 目錄…',
    en: 'Backing up the DIYLevel folder…',
    ko: 'DIYLevel 폴더 백업 중…',
    ja: 'DIYLevel フォルダーをバックアップ中…',
  },
  // 恢复 DIYLevel 备份
  restoreDiylevelBackup: {
    'zh-CN': '恢复 DIYLevel 备份',
    'zh-TW': '還原 DIYLevel 備份',
    en: 'Restore DIYLevel backup',
    ko: 'DIYLevel 백업 복원',
    ja: 'DIYLevel バックアップを復元',
  },
  // 当前的 DIYLevel 目录会被删除，然后还原成这个备份的内容。继续？
  theCurrentDiylevelFolderWillBeDeletedThenReplacedWithThisBackupContinue: {
    'zh-CN': '当前的 DIYLevel 目录会被删除，然后还原成这个备份的内容。继续？',
    'zh-TW': '目前的 DIYLevel 目錄會被刪除，然後還原成這個備份的內容。繼續？',
    en: 'The current DIYLevel folder will be deleted, then replaced with this backup. Continue?',
    ko: '현재 DIYLevel 폴더를 삭제한 뒤 이 백업 내용으로 되돌립니다. 계속할까요?',
    ja: '現在の DIYLevel フォルダーを削除してから、このバックアップの内容に戻します。続行しますか？',
  },
  // 正在恢复 DIYLevel 备份…
  restoringDiylevelBackup: {
    'zh-CN': '正在恢复 DIYLevel 备份…',
    'zh-TW': '正在還原 DIYLevel 備份…',
    en: 'Restoring DIYLevel backup…',
    ko: 'DIYLevel 백업 복원 중…',
    ja: 'DIYLevel バックアップを復元中…',
  },
  // 这个备份会被永久删除，无法恢复。继续？
  thisBackupWillBeDeletedPermanentlyContinue: {
    'zh-CN': '这个备份会被永久删除，无法恢复。继续？',
    'zh-TW': '這個備份會被永久刪除，無法還原。繼續？',
    en: 'This backup will be deleted permanently. Continue?',
    ko: '이 백업은 영구 삭제되며 되돌릴 수 없습니다. 계속할까요?',
    ja: 'このバックアップは完全に削除され、元に戻せません。続行しますか？',
  },
  // DIYLevel 框架已启用
  diylevelFrameworkEnabled: {
    'zh-CN': 'DIYLevel 框架已启用',
    'zh-TW': 'DIYLevel 框架已啟用',
    en: 'DIYLevel framework enabled',
    ko: 'DIYLevel 프레임워크 활성화됨',
    ja: 'DIYLevel フレームワークを有効にしました',
  },
  // DIYLevel 框架已禁用
  diylevelFrameworkDisabled: {
    'zh-CN': 'DIYLevel 框架已禁用',
    'zh-TW': 'DIYLevel 框架已禁用',
    en: 'DIYLevel framework disabled',
    ko: 'DIYLevel 프레임워크 비활성화됨',
    ja: 'DIYLevel フレームワークを無効にしました',
  },
  // 尚未安装 DIYLevel 框架
  diylevelFrameworkIsNotInstalled: {
    'zh-CN': '尚未安装 DIYLevel 框架',
    'zh-TW': '尚未安裝 DIYLevel 框架',
    en: 'DIYLevel framework is not installed',
    ko: 'DIYLevel 프레임워크가 설치되어 있지 않습니다',
    ja: 'DIYLevel フレームワークがインストールされていません',
  },
  // 已打开 DIYLevel 目录
  openedTheDiylevelFolder: {
    'zh-CN': '已打开 DIYLevel 目录',
    'zh-TW': '已開啟 DIYLevel 目錄',
    en: 'Opened the DIYLevel folder',
    ko: 'DIYLevel 폴더를 열었습니다',
    ja: 'DIYLevel フォルダーを開きました',
  },
  // 尚未安装 DIYLevel 框架，或安装结构与预期不同
  diylevelIsNotInstalledOrItsLayoutDiffersFromTheExpectedOne: {
    'zh-CN': '尚未安装 DIYLevel 框架，或安装结构与预期不同',
    'zh-TW': '尚未安裝 DIYLevel 框架，或安裝結構與預期不同',
    en: 'DIYLevel is not installed, or its layout differs from the expected one.',
    ko: 'DIYLevel이 설치되어 있지 않거나 설치 구조가 예상과 다릅니다.',
    ja: 'DIYLevel がインストールされていないか、構成が想定と異なります。',
  },
}
