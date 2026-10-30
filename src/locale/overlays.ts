import type { DomainMessages } from './types'

/**
 * overlays 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const overlays: DomainMessages = {
  // 正在备份 BepInEx 框架…
  backingUpTheBepinexFramework: {
    'zh-CN': '正在备份 BepInEx 框架…',
    'zh-TW': '正在備份 BepInEx 框架…',
    en: 'Backing up the BepInEx framework…',
    ko: 'BepInEx 프레임워크 백업 중…',
    ja: 'BepInEx フレームワークをバックアップ中…',
  },
  // 正在恢复 BepInEx 框架…
  restoringTheBepinexFramework: {
    'zh-CN': '正在恢复 BepInEx 框架…',
    'zh-TW': '正在還原 BepInEx 框架…',
    en: 'Restoring the BepInEx framework…',
    ko: 'BepInEx 프레임워크 복원 중…',
    ja: 'BepInEx フレームワークを復元中…',
  },
  // 正在备份并卸载 BepInEx 框架…
  backingUpAndRemovingTheBepinexFramework: {
    'zh-CN': '正在备份并卸载 BepInEx 框架…',
    'zh-TW': '正在備份並解除安裝 BepInEx 框架…',
    en: 'Backing up and removing the BepInEx framework…',
    ko: 'BepInEx 프레임워크 백업 및 제거 중…',
    ja: 'BepInEx フレームワークをバックアップして削除中…',
  },
  // 回到这里点击下方按钮继续
  comeBackHereAndContinueBelow: {
    'zh-CN': '回到这里点击下方按钮继续',
    'zh-TW': '回到這裡點擊下方按鈕繼續',
    en: 'Come back here and continue below',
    ko: '여기로 돌아와 아래 버튼을 누르세요',
    ja: 'ここに戻って下のボタンを押してください',
  },
  // 未检测到 CrossOver 容器。请先在 CrossOver 里启动一次《胡闹厨房2》，让容器生成后再回来。
  noCrossoverBottleDetectedLaunchOvercooked2OnceThroughCrossoverSoTheBottleIsCreatedThenComeBack: {
    'zh-CN': '未检测到 CrossOver 容器。请先在 CrossOver 里启动一次《胡闹厨房2》，让容器生成后再回来。',
    'zh-TW': '未偵測到 CrossOver 容器。請先在 CrossOver 裡啟動一次《胡鬧廚房2》，讓容器產生後再回來。',
    en: 'No CrossOver bottle detected. Launch Overcooked! 2 once through CrossOver so the bottle is created, then come back.',
    ko: 'CrossOver 병을 찾지 못했습니다. CrossOver에서 《오버쿡드! 2》를 한 번 실행해 병을 만든 뒤 다시 오세요.',
    ja: 'CrossOver ボトルが見つかりません。CrossOver で『オーバークック2』を一度起動してボトルを作成してから戻ってください。',
  },
  // 在终端执行以下命令安装字体
  runThisInATerminalToInstallFonts: {
    'zh-CN': '在终端执行以下命令安装字体',
    'zh-TW': '在終端機執行以下指令安裝字型',
    en: 'Run this in a terminal to install fonts',
    ko: '터미널에서 아래 명령을 실행해 글꼴을 설치하세요',
    ja: 'ターミナルで次のコマンドを実行してフォントをインストールします',
  },
  // 这一步不是必须的：跳过只会让 BepInEx 控制台里的中文显示成方块，不影响 MOD 加载。
  optionalSkippingItOnlyMakesChineseTextInTheBepinexConsoleShowAsBoxesModsStillLoad: {
    'zh-CN': '这一步不是必须的：跳过只会让 BepInEx 控制台里的中文显示成方块，不影响 MOD 加载。',
    'zh-TW': '這一步不是必須的：跳過只會讓 BepInEx 主控台裡的中文顯示成方塊，不影響 MOD 載入。',
    en: 'Optional: skipping it only makes Chinese text in the BepInEx console show as boxes; MODs still load.',
    ko: '필수는 아닙니다. 건너뛰면 BepInEx 콘솔의 한글이 네모로 보일 뿐 MOD 로드에는 영향이 없습니다.',
    ja: '必須ではありません。省略すると BepInEx コンソールの日本語が□で表示されるだけで、MOD の読み込みには影響しません。',
  },
  // BepInEx 已安装。最后一步：把下面的命令粘贴到 Steam 里《胡闹厨房2》的启动选项中。
  bepinexIsInstalledLastStepPasteTheCommandBelowIntoTheSteamLaunchOptionsForOvercooked2: {
    'zh-CN': 'BepInEx 已安装。最后一步：把下面的命令粘贴到 Steam 里《胡闹厨房2》的启动选项中。',
    'zh-TW': 'BepInEx 已安裝。最後一步：把下面的指令貼到 Steam 裡《胡鬧廚房2》的啟動選項中。',
    en: 'BepInEx is installed. Last step: paste the command below into the Steam launch options for Overcooked! 2.',
    ko: 'BepInEx가 설치되었습니다. 마지막 단계: 아래 명령을 Steam의 《오버쿡드! 2》 시작 옵션에 붙여넣으세요.',
    ja: 'BepInEx をインストールしました。最後の手順：以下のコマンドを Steam の『オーバークック2』起動オプションに貼り付けてください。',
  },
  // 启动选项
  launchOptions: {
    'zh-CN': '启动选项',
    'zh-TW': '啟動選項',
    en: 'Launch options',
    ko: '시작 옵션',
    ja: '起動オプション',
  },
  // 尚未探测到游戏目录，无法生成启动命令。请先在设置里确认游戏目录。
  noGameFolderDetectedSoTheLaunchCommandCannotBeGeneratedCheckTheGameFolderInSettingsFirst: {
    'zh-CN': '尚未探测到游戏目录，无法生成启动命令。请先在设置里确认游戏目录。',
    'zh-TW': '尚未偵測到遊戲目錄，無法產生啟動指令。請先在設定裡確認遊戲目錄。',
    en: 'No game folder detected, so the launch command cannot be generated. Check the game folder in Settings first.',
    ko: '게임 경로를 찾지 못해 시작 명령을 만들 수 없습니다. 설정에서 게임 경로를 먼저 확인하세요.',
    ja: 'ゲームフォルダーが見つからないため起動コマンドを生成できません。設定でゲームフォルダーを確認してください。',
  },
  // 库中右键游戏
  rightClickTheGameInYourLibrary: {
    'zh-CN': '库中右键游戏',
    'zh-TW': '在媒體庫中右鍵遊戲',
    en: 'Right-click the game in your library',
    ko: '라이브러리에서 게임 우클릭',
    ja: 'ライブラリでゲームを右クリック',
  },
  // Steam → 库 → 右键《胡闹厨房2》
  steamLibraryRightClickOvercooked2: {
    'zh-CN': 'Steam → 库 → 右键《胡闹厨房2》',
    'zh-TW': 'Steam → 媒體庫 → 右鍵《胡鬧廚房2》',
    en: 'Steam → Library → right-click Overcooked! 2',
    ko: 'Steam → 라이브러리 → 《오버쿡드! 2》 우클릭',
    ja: 'Steam → ライブラリ → 『オーバークック2』を右クリック',
  },
  // 打开属性
  openProperties: {
    'zh-CN': '打开属性',
    'zh-TW': '開啟內容',
    en: 'Open Properties',
    ko: '속성 열기',
    ja: 'プロパティを開く',
  },
  // 选择「属性」→「通用」
  choosePropertiesGeneral: {
    'zh-CN': '选择「属性」→「通用」',
    'zh-TW': '選擇「內容」→「一般」',
    en: 'Choose Properties → General',
    ko: '"속성" → "일반" 선택',
    ja: '「プロパティ」→「一般」を選択',
  },
  // 粘贴启动选项
  pasteTheLaunchOptions: {
    'zh-CN': '粘贴启动选项',
    'zh-TW': '貼上啟動選項',
    en: 'Paste the launch options',
    ko: '시작 옵션 붙여넣기',
    ja: '起動オプションを貼り付け',
  },
  // 把上面的命令粘贴到「启动选项」
  pasteTheCommandAboveIntoLaunchOptions: {
    'zh-CN': '把上面的命令粘贴到「启动选项」',
    'zh-TW': '把上面的指令貼到「啟動選項」',
    en: 'Paste the command above into "Launch options"',
    ko: '위 명령을 "시작 옵션"에 붙여넣으세요',
    ja: '上のコマンドを「起動オプション」に貼り付けてください',
  },
  // 我已完成所需操作
  iHaveCompletedTheseSteps: {
    'zh-CN': '我已完成所需操作',
    'zh-TW': '我已完成所需操作',
    en: 'I have completed these steps',
    ko: '필요한 작업을 마쳤습니다',
    ja: '必要な操作を完了しました',
  },
  // 版本：{version}
  version: {
    'zh-CN': '版本：{version}',
    'zh-TW': '版本：{version}',
    en: 'Version: {version}',
    ko: '버전: {version}',
    ja: 'バージョン: {version}',
  },
  // 加载器：{state}
  loader: {
    'zh-CN': '加载器：{state}',
    'zh-TW': '載入器：{state}',
    en: 'Loader: {state}',
    ko: '로더: {state}',
    ja: 'ローダー: {state}',
  },
  // 框架已启用
  frameworkEnabled: {
    'zh-CN': '框架已启用',
    'zh-TW': '框架已啟用',
    en: 'Framework enabled',
    ko: '프레임워크 활성',
    ja: 'フレームワーク有効',
  },
  // 框架已禁用
  frameworkDisabled: {
    'zh-CN': '框架已禁用',
    'zh-TW': '框架已停用',
    en: 'Framework disabled',
    ko: '프레임워크 비활성',
    ja: 'フレームワーク無効',
  },
  // 未找到加载器
  loaderMissing: {
    'zh-CN': '未找到加载器',
    'zh-TW': '未找到載入器',
    en: 'Loader missing',
    ko: '로더 없음',
    ja: 'ローダーが見つかりません',
  },
  // 状态冲突
  stateConflict: {
    'zh-CN': '状态冲突',
    'zh-TW': '狀態衝突',
    en: 'State conflict',
    ko: '상태 충돌',
    ja: '状態の競合',
  },
  // 运行形态：{runtime}
  runtime: {
    'zh-CN': '运行形态：{runtime}',
    'zh-TW': '執行形態：{runtime}',
    en: 'Runtime: {runtime}',
    ko: '실행 형태: {runtime}',
    ja: '実行形態: {runtime}',
  },
  // 下载渠道
  downloadChannel: {
    'zh-CN': '下载渠道',
    'zh-TW': '下載渠道',
    en: 'Download channel',
    ko: '다운로드 채널',
    ja: 'ダウンロード経路',
  },
  // Gitee（中国大陆推荐）
  giteeRecommendedInMainlandChina: {
    'zh-CN': 'Gitee（中国大陆推荐）',
    'zh-TW': 'Gitee（中國大陸推薦）',
    en: 'Gitee (recommended in mainland China)',
    ko: 'Gitee(중국 본토 권장)',
    ja: 'Gitee（中国本土推奨）',
  },
  // GitHub
  github: {
    'zh-CN': 'GitHub',
    'zh-TW': 'GitHub',
    en: 'GitHub',
    ko: 'GitHub',
    ja: 'GitHub',
  },
  // 重新安装
  reinstall: {
    'zh-CN': '重新安装',
    'zh-TW': '重新安裝',
    en: 'Reinstall',
    ko: '재설치',
    ja: '再インストール',
  },
  // 安装框架
  installFramework: {
    'zh-CN': '安装框架',
    'zh-TW': '安裝框架',
    en: 'Install framework',
    ko: '프레임워크 설치',
    ja: 'フレームワークをインストール',
  },
  // 创建备份
  createBackup: {
    'zh-CN': '创建备份',
    'zh-TW': '建立備份',
    en: 'Create backup',
    ko: '백업 생성',
    ja: 'バックアップを作成',
  },
  // 卸载框架
  uninstallFramework: {
    'zh-CN': '卸载框架',
    'zh-TW': '解除安裝框架',
    en: 'Uninstall framework',
    ko: '프레임워크 제거',
    ja: 'フレームワークを削除',
  },
  // 打开备份目录
  openBackupFolder: {
    'zh-CN': '打开备份目录',
    'zh-TW': '開啟備份目錄',
    en: 'Open backup folder',
    ko: '백업 폴더 열기',
    ja: 'バックアップフォルダーを開く',
  },
  // 框架启用（游戏加载 MOD）
  frameworkEnabledGameLoadsMods: {
    'zh-CN': '框架启用（游戏加载 MOD）',
    'zh-TW': '框架啟用（遊戲載入 MOD）',
    en: 'Framework enabled (game loads MODs)',
    ko: '프레임워크 활성(게임이 MOD 로드)',
    ja: 'フレームワーク有効（ゲームがMODを読み込む）',
  },
  // 框架备份
  frameworkBackups: {
    'zh-CN': '框架备份',
    'zh-TW': '框架備份',
    en: 'Framework backups',
    ko: '프레임워크 백업',
    ja: 'フレームワークのバックアップ',
  },
  // 暂无备份。卸载或重装前建议先创建备份。
  noBackupsYetCreateOneBeforeUninstallingOrReinstalling: {
    'zh-CN': '暂无备份。卸载或重装前建议先创建备份。',
    'zh-TW': '暫無備份。解除安裝或重新安裝前建議先建立備份。',
    en: 'No backups yet. Create one before uninstalling or reinstalling.',
    ko: '백업이 없습니다. 제거·재설치 전에 백업을 만드세요.',
    ja: 'バックアップはありません。削除・再インストール前に作成してください。',
  },
  // 恢复
  restore: {
    'zh-CN': '恢复',
    'zh-TW': '還原',
    en: 'Restore',
    ko: '복원',
    ja: '復元',
  },
  // 检测到 MOD 目录，安装后会保留
  existingModFolderFoundKeptAfterInstall: {
    'zh-CN': '检测到 MOD 目录，安装后会保留',
    'zh-TW': '偵測到 MOD 目錄，安裝後會保留',
    en: 'Existing MOD folder found, kept after install',
    ko: 'MOD 폴더가 있습니다. 설치 후에도 유지됩니다',
    ja: 'MOD フォルダーがあります。インストール後も保持されます',
  },
  // 检测到已有的 plugins / config，安装框架时会原样保留。
  existingPluginsConfigFoundTheyAreKeptAsIsWhenInstallingTheFramework: {
    'zh-CN': '检测到已有的 plugins / config，安装框架时会原样保留。',
    'zh-TW': '偵測到既有的 plugins / config，安裝框架時會原樣保留。',
    en: 'Existing plugins / config found; they are kept as-is when installing the framework.',
    ko: '기존 plugins / config 가 있습니다. 프레임워크 설치 시 그대로 유지됩니다.',
    ja: '既存の plugins / config があります。フレームワークのインストール時にそのまま保持されます。',
  },
  // 安装后这里会出现启用开关、配置管理与备份入口。
  afterInstallingTheEnableSwitchConfigManagerAndBackupsAppearHere: {
    'zh-CN': '安装后这里会出现启用开关、配置管理与备份入口。',
    'zh-TW': '安裝後這裡會出現啟用開關、設定管理與備份入口。',
    en: 'After installing, the enable switch, config manager and backups appear here.',
    ko: '설치하면 활성 스위치, 설정 관리, 백업 항목이 여기에 표시됩니다.',
    ja: 'インストールすると、有効スイッチ・設定管理・バックアップがここに表示されます。',
  },
}
