import type { DomainMessages } from './types'

/**
 * 框架管理（BepInEx 与 DIYLevel）的文案。
 *
 * 对应界面：设置页「路径管理」里的两个框架行，以及它们各自的右侧抽屉
 * （状态摘要、安装/重装/备份/恢复/卸载、备份列表）。
 *
 * 这两个框架的交互刻意保持一致，所以文案也放在同一个域里 —— 同一个动作
 * （创建备份、恢复、卸载）在两边叫同一个名字，用户学一次就够。
 */
export const framework: DomainMessages = {
  // ===== 标题与状态 =====
  'bepinex.title': {
    'zh-CN': 'BepInEx 框架',
    'zh-TW': 'BepInEx 框架',
    en: 'BepInEx framework',
    ko: 'BepInEx 프레임워크',
    ja: 'BepInEx フレームワーク',
  },
  'bepinex.manage': {
    'zh-CN': 'BepInEx 框架管理',
    'zh-TW': 'BepInEx 框架管理',
    en: 'BepInEx framework',
    ko: 'BepInEx 프레임워크 관리',
    ja: 'BepInEx フレームワーク管理',
  },
  'diyLevel.title': {
    'zh-CN': 'DIYLevel 框架',
    'zh-TW': 'DIYLevel 框架',
    en: 'DIYLevel framework',
    ko: 'DIYLevel 프레임워크',
    ja: 'DIYLevel フレームワーク',
  },
  'diyLevel.manage': {
    'zh-CN': 'DIYLevel 框架管理',
    'zh-TW': 'DIYLevel 框架管理',
    en: 'DIYLevel framework',
    ko: 'DIYLevel 프레임워크 관리',
    ja: 'DIYLevel フレームワーク管理',
  },

  // ===== 动作 =====
  'action.install': {
    'zh-CN': '安装框架',
    'zh-TW': '安裝框架',
    en: 'Install framework',
    ko: '프레임워크 설치',
    ja: 'フレームワークをインストール',
  },
  'action.reinstall': {
    'zh-CN': '重新安装',
    'zh-TW': '重新安裝',
    en: 'Reinstall',
    ko: '재설치',
    ja: '再インストール',
  },
  'action.createBackup': {
    'zh-CN': '创建备份',
    'zh-TW': '建立備份',
    en: 'Create backup',
    ko: '백업 만들기',
    ja: 'バックアップを作成',
  },
  'action.uninstall': {
    'zh-CN': '卸载框架',
    'zh-TW': '解除安裝框架',
    en: 'Uninstall framework',
    ko: '프레임워크 제거',
    ja: 'フレームワークをアンインストール',
  },
  'action.manage': {
    'zh-CN': '管理',
    'zh-TW': '管理',
    en: 'Manage',
    ko: '관리',
    ja: '管理',
  },
  'action.openPath': {
    'zh-CN': '打开路径',
    'zh-TW': '開啟路徑',
    en: 'Open folder',
    ko: '경로 열기',
    ja: 'フォルダーを開く',
  },
  'action.openMapDir': {
    'zh-CN': '打开地图目录',
    'zh-TW': '開啟地圖目錄',
    en: 'Open map folder',
    ko: '맵 폴더 열기',
    ja: 'マップフォルダーを開く',
  },
  'action.openBackupDir': {
    'zh-CN': '打开备份目录',
    'zh-TW': '開啟備份目錄',
    en: 'Open backup folder',
    ko: '백업 폴더 열기',
    ja: 'バックアップフォルダーを開く',
  },
  'action.configManage': {
    'zh-CN': '配置管理',
    'zh-TW': '設定管理',
    en: 'Configuration',
    ko: '설정 관리',
    ja: '設定管理',
  },

  // ===== 状态与摘要 =====
  'state.notInstalledHint': {
    'zh-CN': '尚未安装，点击右侧按钮安装',
    'zh-TW': '尚未安裝，點擊右側按鈕安裝',
    en: 'Not installed yet — use the button on the right',
    ko: '아직 설치되지 않았습니다. 오른쪽 버튼으로 설치하세요',
    ja: '未インストールです。右のボタンからインストールしてください',
  },
  'state.residueHint': {
    'zh-CN': '尚未安装框架，检测到已有的 plugins / config，安装后会保留',
    'zh-TW': '尚未安裝框架，偵測到既有的 plugins / config，安裝後會保留',
    en: 'Framework not installed. Existing plugins / config were found and will be kept.',
    ko: '프레임워크가 설치되지 않았습니다. 기존 plugins / config가 있으며 설치 후에도 유지됩니다.',
    ja: 'フレームワーク未インストール。既存の plugins / config を検出しました。インストール後も保持されます。',
  },
  'state.frameworkEnabled': {
    'zh-CN': '框架已启用',
    'zh-TW': '框架已啟用',
    en: 'Framework enabled',
    ko: '프레임워크 활성화됨',
    ja: 'フレームワーク有効',
  },
  'state.frameworkDisabled': {
    'zh-CN': '框架已禁用',
    'zh-TW': '框架已禁用',
    en: 'Framework disabled',
    ko: '프레임워크 비활성화됨',
    ja: 'フレームワーク無効',
  },
  'state.enableFramework': {
    'zh-CN': '框架启用（游戏加载 MOD）',
    'zh-TW': '框架啟用（遊戲載入 MOD）',
    en: 'Framework enabled (the game loads MODs)',
    ko: '프레임워크 활성화(게임이 MOD를 불러옵니다)',
    ja: 'フレームワーク有効（ゲームが MOD を読み込みます）',
  },
  'state.enableThisFramework': {
    'zh-CN': '框架启用（游戏加载此框架）',
    'zh-TW': '框架啟用（遊戲載入此框架）',
    en: 'Framework enabled (the game loads this framework)',
    ko: '프레임워크 활성화(게임이 이 프레임워크를 불러옵니다)',
    ja: 'フレームワーク有効（ゲームがこのフレームワークを読み込みます）',
  },
  'state.runtimeMode': {
    'zh-CN': '运行形态',
    'zh-TW': '執行形態',
    en: 'Runtime',
    ko: '실행 형태',
    ja: '実行形態',
  },

  // ===== 备份列表 =====
  'backup.section': {
    'zh-CN': '框架备份',
    'zh-TW': '框架備份',
    en: 'Framework backups',
    ko: '프레임워크 백업',
    ja: 'フレームワークのバックアップ',
  },
  'backup.empty': {
    'zh-CN': '暂无备份。卸载或重装前建议先创建备份。',
    'zh-TW': '尚無備份。解除安裝或重新安裝前建議先建立備份。',
    en: 'No backups yet. Create one before uninstalling or reinstalling.',
    ko: '백업이 없습니다. 제거하거나 재설치하기 전에 백업을 만드는 것이 좋습니다.',
    ja: 'バックアップがありません。アンインストールや再インストールの前に作成してください。',
  },
  'backup.deleteTitle': {
    'zh-CN': '删除备份',
    'zh-TW': '刪除備份',
    en: 'Delete backup',
    ko: '백업 삭제',
    ja: 'バックアップを削除',
  },
  'backup.deleteConfirm': {
    'zh-CN': '这个备份会被永久删除，无法恢复。继续？',
    'zh-TW': '這個備份會被永久刪除，無法還原。繼續？',
    en: 'This backup will be deleted permanently. Continue?',
    ko: '이 백업은 영구 삭제되며 되돌릴 수 없습니다. 계속할까요?',
    ja: 'このバックアップは完全に削除され、元に戻せません。続行しますか？',
  },

  // ===== 安装提示 =====
  'install.source': {
    'zh-CN': '安装来源',
    'zh-TW': '安裝來源',
    en: 'Install source',
    ko: '설치 소스',
    ja: 'インストール元',
  },
  'install.sourceMissing': {
    'zh-CN': '未找到可在线安装的仓库',
    'zh-TW': '找不到可線上安裝的倉庫',
    en: 'No repository can install it online',
    ko: '온라인 설치가 가능한 저장소가 없습니다',
    ja: 'オンラインインストールできるリポジトリがありません',
  },
  'install.hint': {
    'zh-CN': '安装后这里会出现启用开关、配置管理与备份入口。',
    'zh-TW': '安裝後這裡會出現啟用開關、設定管理與備份入口。',
    en: 'After installing, the enable switch, config manager and backups appear here.',
    ko: '설치하면 활성 스위치, 설정 관리, 백업 항목이 여기에 표시됩니다.',
    ja: 'インストールすると、有効スイッチ・設定管理・バックアップがここに表示されます。',
  },
  'install.noOnlineHint': {
    'zh-CN': '无法在线安装：MOD 安装页没有声明此 GUID 的仓库，请手动安装后回到这里管理。',
    'zh-TW': '無法線上安裝：MOD 安裝頁沒有宣告此 GUID 的倉庫，請手動安裝後回到這裡管理。',
    en: 'Not available online: no repository declares this GUID. Install it manually, then manage it here.',
    ko: '온라인 설치 불가: 이 GUID를 선언한 저장소가 없습니다. 직접 설치한 뒤 여기로 돌아와 관리하세요.',
    ja: 'オンラインインストール不可：この GUID を宣言したリポジトリがありません。手動でインストールしてからここで管理してください。',
  },
  'install.downloadChannel': {
    'zh-CN': '下载渠道',
    'zh-TW': '下載管道',
    en: 'Download source',
    ko: '다운로드 채널',
    ja: 'ダウンロード元',
  },

  // ===== 确认与遮罩 =====
  'uninstall.bepinexTitle': {
    'zh-CN': '卸载 BepInEx 框架',
    'zh-TW': '解除安裝 BepInEx 框架',
    en: 'Uninstall BepInEx',
    ko: 'BepInEx 제거',
    ja: 'BepInEx をアンインストール',
  },
  'uninstall.bepinexBody': {
    'zh-CN': '将移除游戏目录中的 BepInEx 框架文件。建议先创建备份。继续卸载？',
    'zh-TW': '將移除遊戲目錄中的 BepInEx 框架檔案。建議先建立備份。繼續解除安裝？',
    en: 'This removes BepInEx files from the game folder. Create a backup first. Continue?',
    ko: '게임 폴더의 BepInEx 파일을 제거합니다. 먼저 백업하는 것이 좋습니다. 계속할까요?',
    ja: 'ゲームフォルダーの BepInEx ファイルを削除します。先にバックアップを推奨します。続行しますか？',
  },
  'uninstall.diyLevelTitle': {
    'zh-CN': '卸载 DIYLevel 框架',
    'zh-TW': '解除安裝 DIYLevel 框架',
    en: 'Uninstall DIYLevel framework',
    ko: 'DIYLevel 프레임워크 제거',
    ja: 'DIYLevel フレームワークをアンインストール',
  },
  'uninstall.diyLevelBody': {
    'zh-CN': '卸载前会自动备份整个 DIYLevel 目录。其中的自制地图文件会一并移入回收站。继续？',
    'zh-TW': '解除安裝前會自動備份整個 DIYLevel 目錄。其中的自製地圖檔案會一併移入資源回收筒。繼續？',
    en: 'The whole DIYLevel folder is backed up before uninstalling. Custom maps inside are moved to the recycle bin too. Continue?',
    ko: '제거 전에 DIYLevel 폴더 전체를 자동으로 백업합니다. 안의 커스텀 맵도 휴지통으로 이동합니다. 계속할까요?',
    ja: 'アンインストール前に DIYLevel フォルダー全体を自動でバックアップします。中の自作マップもゴミ箱へ移動します。続行しますか？',
  },
  'backup.restoreTitle': {
    'zh-CN': '恢复备份',
    'zh-TW': '還原備份',
    en: 'Restore backup',
    ko: '백업 복원',
    ja: 'バックアップを復元',
  },
  'backup.restoreConfirm': {
    'zh-CN': '当前目录会被删除，然后还原成这个备份的内容。继续？',
    'zh-TW': '目前目錄會被刪除，然後還原成這個備份的內容。繼續？',
    en: 'The current folder will be deleted, then replaced with this backup. Continue?',
    ko: '현재 폴더를 삭제한 뒤 이 백업 내용으로 되돌립니다. 계속할까요?',
    ja: '現在のフォルダーを削除してから、このバックアップの内容に戻します。続行しますか？',
  },
  'busy.backingUp': {
    'zh-CN': '正在备份…',
    'zh-TW': '正在備份…',
    en: 'Backing up…',
    ko: '백업 중…',
    ja: 'バックアップ中…',
  },
  'busy.restoring': {
    'zh-CN': '正在恢复备份…',
    'zh-TW': '正在還原備份…',
    en: 'Restoring backup…',
    ko: '백업 복원 중…',
    ja: 'バックアップを復元中…',
  },
  'busy.uninstalling': {
    'zh-CN': '正在备份并卸载…',
    'zh-TW': '正在備份並解除安裝…',
    en: 'Backing up and uninstalling…',
    ko: '백업 및 제거 중…',
    ja: 'バックアップしてアンインストール中…',
  },

  // ===== 备份/恢复提示 =====
  'notice.backupCreated': {
    'zh-CN': '备份完成：{path}',
    'zh-TW': '備份完成：{path}',
    en: 'Backup created: {path}',
    ko: '백업 완료: {path}',
    ja: 'バックアップ完了：{path}',
  },
  'notice.backupRestored': {
    'zh-CN': '备份已恢复',
    'zh-TW': '備份已還原',
    en: 'Backup restored',
    ko: '백업이 복원되었습니다',
    ja: 'バックアップを復元しました',
  },
  'notice.backupDeleted': {
    'zh-CN': '备份已删除',
    'zh-TW': '備份已刪除',
    en: 'Backup deleted',
    ko: '백업이 삭제되었습니다',
    ja: 'バックアップを削除しました',
  },

  // ===== DIYLevel 地图集（原抽屉里的区块，已移到地图管理页备用）=====
  'mapPacks.title': {
    'zh-CN': '地图集',
    'zh-TW': '地圖集',
    en: 'Map packs',
    ko: '맵 팩',
    ja: 'マップパック',
  },
  'mapPacks.empty': {
    'zh-CN': '还没有地图集。把地图文件夹放进 levels 目录即可，每个文件夹算一个地图集。',
    'zh-TW': '還沒有地圖集。把地圖資料夾放進 levels 目錄即可，每個資料夾算一個地圖集。',
    en: 'No map packs yet. Drop map folders into the levels folder — each folder counts as one pack.',
    ko: '아직 맵 팩이 없습니다. 맵 폴더를 levels 폴더에 넣으면 폴더 하나가 맵 팩 하나입니다.',
    ja: 'マップパックがありません。マップフォルダーを levels フォルダーに入れてください（1 フォルダー＝1 パック）。',
  },
  'mapPacks.count': {
    'zh-CN': '当前有 {count} 个地图集（每张地图集是 levels 下的一个文件夹）。',
    'zh-TW': '目前有 {count} 個地圖集（每張地圖集是 levels 下的一個資料夾）。',
    en: 'Currently {count} map packs (each is a folder under levels).',
    ko: '현재 맵 팩 {count}개(각각 levels 아래의 폴더)입니다.',
    ja: '現在 {count} 個のマップパックがあります（各 levels 配下のフォルダー）。',
  },
  'mapPacks.openDir': {
    'zh-CN': '打开 levels 目录',
    'zh-TW': '開啟 levels 目錄',
    en: 'Open levels folder',
    ko: 'levels 폴더 열기',
    ja: 'levels フォルダーを開く',
  },
}
