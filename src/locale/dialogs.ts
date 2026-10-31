import type { DomainMessages } from './types'

/**
 * dialogs 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const dialogs: DomainMessages = {
  // 发现新版本
  updateAvailable: {
    'zh-CN': '发现新版本',
    'zh-TW': '發現新版本',
    en: 'Update available',
    ko: '새 버전 발견',
    ja: '新しいバージョン',
  },
  // 更新内容：
  whatSNew: {
    'zh-CN': '更新内容：',
    'zh-TW': '更新內容：',
    en: 'What\\\'s new:',
    ko: '업데이트 내용:',
    ja: '更新内容：',
  },
  // 本次更新没有提供说明。
  noReleaseNotesWereProvided: {
    'zh-CN': '本次更新没有提供说明。',
    'zh-TW': '本次更新沒有提供說明。',
    en: 'No release notes were provided.',
    ko: '이번 업데이트에는 설명이 없습니다.',
    ja: '今回の更新に説明はありません。',
  },
  // 当前 {current} → 新版 {next}
  text: {
    'zh-CN': '当前 {current} → 新版 {next}',
    'zh-TW': '目前 {current} → 新版 {next}',
    en: '{current} → {next}',
    ko: '{current} → {next}',
    ja: '{current} → {next}',
  },
  // 准备下载更新…
  preparingDownload: {
    'zh-CN': '准备下载更新…',
    'zh-TW': '準備下載更新…',
    en: 'Preparing download…',
    ko: '업데이트 다운로드 준비 중…',
    ja: '更新のダウンロードを準備中…',
  },
  // 更新安装已开始，应用将自动退出
  updateInstallStartedTheAppWillExitAutomatically: {
    'zh-CN': '更新安装已开始，应用将自动退出',
    'zh-TW': '更新安裝已開始，應用程式將自動結束',
    en: 'The update installer has started. The app will exit automatically.',
    ko: '업데이트 설치가 시작되었습니다. 앱이 자동으로 종료됩니다.',
    ja: '更新のインストールを開始しました。アプリは自動で終了します。',
  },
  // 更新
  update: {
    'zh-CN': '更新',
    'zh-TW': '更新',
    en: 'Update',
    ko: '업데이트',
    ja: '更新',
  },
  // 稍后
  later: {
    'zh-CN': '稍后',
    'zh-TW': '稍後',
    en: 'Later',
    ko: '나중에',
    ja: '後で',
  },
  // 当前已是最新版本
  youAreUpToDate: {
    'zh-CN': '当前已是最新版本',
    'zh-TW': '目前已是最新版本',
    en: 'You are up to date',
    ko: '최신 버전입니다',
    ja: '最新バージョンです',
  },
  // 更新检查失败
  updateCheckFailed: {
    'zh-CN': '更新检查失败',
    'zh-TW': '檢查更新失敗',
    en: 'Update check failed',
    ko: '업데이트 확인 실패',
    ja: '更新確認に失敗しました',
  },
  // 发现新版本 {version}
  versionAvailable: {
    'zh-CN': '发现新版本 {version}',
    'zh-TW': '發現新版本 {version}',
    en: 'Version {version} available',
    ko: '{version} 버전이 있습니다',
    ja: 'バージョン {version} があります',
  },
  // 全部渠道检查失败，可前往发布页手动下载。
  allChannelsFailedDownloadManuallyFromTheReleasePage: {
    'zh-CN': '全部渠道检查失败，可前往发布页手动下载。',
    'zh-TW': '全部渠道檢查失敗，可前往發布頁手動下載。',
    en: 'All channels failed. Download manually from the release page.',
    ko: '모든 채널이 실패했습니다. 릴리스 페이지에서 직접 받으세요.',
    ja: 'すべての経路が失敗しました。リリースページから手動で取得してください。',
  },
  // 打开发布页
  openReleasePage: {
    'zh-CN': '打开发布页',
    'zh-TW': '開啟發布頁',
    en: 'Open release page',
    ko: '릴리스 페이지 열기',
    ja: 'リリースページを開く',
  },
  // 未归类仓库
  uncategorizedRepositories: {
    'zh-CN': '未归类仓库',
    'zh-TW': '未歸類倉庫',
    en: 'Uncategorized repositories',
    ko: '미분류 저장소',
    ja: '未分類リポジトリ',
  },
  // 未分组
  ungrouped: {
    'zh-CN': '未分组',
    'zh-TW': '未分組',
    en: 'Ungrouped',
    ko: '미분류',
    ja: '未分類',
  },
  // 最新版本
  latestVersion: {
    'zh-CN': '最新版本',
    'zh-TW': '最新版本',
    en: 'Latest version',
    ko: '최신 버전',
    ja: '最新バージョン',
  },
  // 下载量
  downloads: {
    'zh-CN': '下载量',
    'zh-TW': '下載量',
    en: 'Downloads',
    ko: '다운로드',
    ja: 'ダウンロード数',
  },
  // 已就绪
  ready: {
    'zh-CN': '已就绪',
    'zh-TW': '已就緒',
    en: 'Ready',
    ko: '준비됨',
    ja: '準備完了',
  },
  // 社区仓库
  communityRepositories: {
    'zh-CN': '社区仓库',
    'zh-TW': '社群倉庫',
    en: 'Community repositories',
    ko: '커뮤니티 저장소',
    ja: 'コミュニティリポジトリ',
  },
  // 来源未知
  unknownOrigin: {
    'zh-CN': '来源未知',
    'zh-TW': '來源未知',
    en: 'Unknown origin',
    ko: '출처 불명',
    ja: '出所未詳',
  },
  // 搜索
  search: {
    'zh-CN': '搜索',
    'zh-TW': '搜尋',
    en: 'Search',
    ko: '검색',
    ja: '検索',
  },
  // 权限
  permissions: {
    'zh-CN': '权限',
    'zh-TW': '權限',
    en: 'Permissions',
    ko: '권한',
    ja: '権限',
  },
  // 以管理员身份运行
  runAsAdministrator: {
    'zh-CN': '以管理员身份运行',
    'zh-TW': '以管理員身分執行',
    en: 'Run as administrator',
    ko: '관리자 권한으로 실행',
    ja: '管理者として実行',
  },
  // 当前已具备管理员权限
  administratorRightsAreActive: {
    'zh-CN': '当前已具备管理员权限',
    'zh-TW': '目前具備管理員權限',
    en: 'Administrator rights are active',
    ko: '관리자 권한이 있습니다',
    ja: '管理者権限があります',
  },
  // 仅在游戏目录不可写时需要
  onlyNeededWhenTheGameFolderIsNotWritable: {
    'zh-CN': '仅在游戏目录不可写时需要',
    'zh-TW': '僅在遊戲目錄不可寫時需要',
    en: 'Only needed when the game folder is not writable',
    ko: '게임 폴더에 쓸 수 없을 때만 필요합니다',
    ja: 'ゲームフォルダーに書き込めない場合のみ必要です',
  },
  // 已提权
  elevated: {
    'zh-CN': '已提权',
    'zh-TW': '已提權',
    en: 'Elevated',
    ko: '권한 있음',
    ja: '昇格済み',
  },
  // 普通权限
  standard: {
    'zh-CN': '普通权限',
    'zh-TW': '一般權限',
    en: 'Standard',
    ko: '일반 권한',
    ja: '通常権限',
  },
  // 以管理员重启
  restartAsAdministrator: {
    'zh-CN': '以管理员重启',
    'zh-TW': '以管理員重新啟動',
    en: 'Restart as administrator',
    ko: '관리자 권한으로 재시작',
    ja: '管理者として再起動',
  },
  // 游戏目录可写
  gameFolderWritable: {
    'zh-CN': '游戏目录可写',
    'zh-TW': '遊戲目錄可寫',
    en: 'Game folder writable',
    ko: '게임 폴더 쓰기 가능',
    ja: 'ゲームフォルダー書き込み可',
  },
  // 可以直接安装和卸载 MOD
  modsCanBeInstalledAndRemovedDirectly: {
    'zh-CN': '可以直接安装和卸载 MOD',
    'zh-TW': '可以直接安裝和解除安裝 MOD',
    en: 'MODs can be installed and removed directly',
    ko: 'MOD를 바로 설치·제거할 수 있습니다',
    ja: 'MOD を直接インストール・削除できます',
  },
  // 需要管理员权限才能写入
  administratorRightsAreRequiredToWrite: {
    'zh-CN': '需要管理员权限才能写入',
    'zh-TW': '需要管理員權限才能寫入',
    en: 'Administrator rights are required to write',
    ko: '쓰려면 관리자 권한이 필요합니다',
    ja: '書き込むには管理者権限が必要です',
  },
  // macOS 上游戏目录不可写时的做法：改目录权限，而不是提权运行管理器
  fixFolderPermissionsInFinder: {
    'zh-CN': '游戏目录不可写：在「访达」里给该目录（含 BepInEx）加上当前用户的读写权限',
    'zh-TW': '遊戲目錄不可寫入：在「Finder」中為該目錄（含 BepInEx）加上目前使用者的讀寫權限',
    en: 'The game folder is not writable: in Finder, give your user read/write access to it (and BepInEx)',
    ko: '게임 폴더에 쓸 수 없습니다. Finder에서 해당 폴더(BepInEx 포함)에 현재 사용자의 읽기/쓰기 권한을 주세요',
    ja: 'ゲームフォルダーに書き込めません。Finder でこのフォルダー（BepInEx を含む）に現在のユーザーの読み書き権限を付けてください',
  },
  // 可写
  writable: {
    'zh-CN': '可写',
    'zh-TW': '可寫',
    en: 'Writable',
    ko: '쓰기 가능',
    ja: '書き込み可',
  },
  // 不可写
  notWritable: {
    'zh-CN': '不可写',
    'zh-TW': '不可寫',
    en: 'Not writable',
    ko: '쓰기 불가',
    ja: '書き込み不可',
  },
  // 在 MOD 安装中查看
  viewInModInstall: {
    'zh-CN': '在 MOD 安装中查看',
    'zh-TW': '在 MOD 安裝中檢視',
    en: 'View in MOD Install',
    ko: 'MOD 설치에서 보기',
    ja: 'MOD インストールで表示',
  },
}
