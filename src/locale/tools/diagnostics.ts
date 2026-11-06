import type { DomainMessages } from '../types.ts'

/** 工具页日志、缓存、开发者工具与窗口诊断。同一 key 的五语保持集中。 */
export const toolsUiDiagnostics: DomainMessages = {
  // 日志
  logs: {
    'zh-CN': '日志',
    'zh-TW': '日誌',
    en: 'Logs',
    ko: '로그',
    ja: 'ログ',
  },
  // 打开 Rust 后端日志
  openRustLog: {
    'zh-CN': '打开 Rust 后端日志',
    'zh-TW': '開啟 Rust 後端日誌',
    en: 'Open Rust log',
    ko: 'Rust 로그 열기',
    ja: 'Rust ログを開く',
  },
  // 安装、下载与文件操作的详细记录
  detailedRecordOfInstallsDownloadsAndFileOperations: {
    'zh-CN': '安装、下载与文件操作的详细记录',
    'zh-TW': '安裝、下載與檔案操作的詳細記錄',
    en: 'Detailed record of installs, downloads and file operations',
    ko: '설치·다운로드·파일 작업의 상세 기록',
    ja: 'インストール・ダウンロード・ファイル操作の詳細',
  },
  // 打开前端日志
  openFrontendLog: {
    'zh-CN': '打开前端日志',
    'zh-TW': '開啟前端日誌',
    en: 'Open frontend log',
    ko: '프런트엔드 로그 열기',
    ja: 'フロントエンドログを開く',
  },
  // 界面渲染与调用的记录
  uiRenderingAndIpcCalls: {
    'zh-CN': '界面渲染与调用的记录',
    'zh-TW': '介面渲染與呼叫的記錄',
    en: 'UI rendering and IPC calls',
    ko: 'UI 렌더링과 호출 기록',
    ja: '画面描画と呼び出しの記録',
  },
  // 打开崩溃日志
  openCrashLog: {
    'zh-CN': '打开崩溃日志',
    'zh-TW': '開啟崩潰日誌',
    en: 'Open crash log',
    ko: '크래시 로그 열기',
    ja: 'クラッシュログを開く',
  },
  // 程序异常退出时的诊断信息
  diagnosticsFromAbnormalExits: {
    'zh-CN': '程序异常退出时的诊断信息',
    'zh-TW': '程式異常結束時的診斷資訊',
    en: 'Diagnostics from abnormal exits',
    ko: '비정상 종료 시 진단 정보',
    ja: '異常終了時の診断情報',
  },
  // 高级
  advanced: {
    'zh-CN': '高级',
    'zh-TW': '進階',
    en: 'Advanced',
    ko: '고급',
    ja: '詳細設定',
  },
  // 打开开发者工具
  openDeveloperTools: {
    'zh-CN': '打开开发者工具',
    'zh-TW': '開啟開發者工具',
    en: 'Open developer tools',
    ko: '개발자 도구 열기',
    ja: '開発者ツールを開く',
  },
  // WebView 开发者控制台（调试用）
  webviewDeveloperConsoleDebugging: {
    'zh-CN': 'WebView 开发者控制台（调试用）',
    'zh-TW': 'WebView 開發者主控台（偵錯用）',
    en: 'WebView developer console (debugging)',
    ko: 'WebView 개발자 콘솔(디버깅)',
    ja: 'WebView 開発者コンソール（デバッグ用）',
  },
  // 重置窗口状态
  resetWindowState: {
    'zh-CN': '重置窗口状态',
    'zh-TW': '重設視窗狀態',
    en: 'Reset window state',
    ko: '창 상태 초기화',
    ja: 'ウィンドウ状態をリセット',
  },
  // 窗口位置或尺寸异常时恢复默认布局
  restoreTheDefaultLayoutWhenTheWindowIsMisplaced: {
    'zh-CN': '窗口位置或尺寸异常时恢复默认布局',
    'zh-TW': '視窗位置或尺寸異常時恢復預設版面',
    en: 'Restore the default layout when the window is misplaced',
    ko: '창 위치·크기가 이상할 때 기본 배치로 복원',
    ja: 'ウィンドウの位置・サイズが異常なとき既定の配置に戻します',
  },
  // 末尾 512 KiB
  last512Kib: {
    'zh-CN': '末尾 512 KiB',
    'zh-TW': '末尾 512 KiB',
    en: 'Last 512 KiB',
    ko: '끝 512 KiB',
    ja: '末尾 512 KiB',
  },
  // （空）
  empty: {
    'zh-CN': '（空）',
    'zh-TW': '（空）',
    en: '(empty)',
    ko: '(비어 있음)',
    ja: '（空）',
  },
  // 缓存
  cache: {
    'zh-CN': '缓存',
    'zh-TW': '快取',
    en: 'Cache',
    ko: '캐시',
    ja: 'キャッシュ',
  },
  // 清除头像缓存
  clearAvatarCache: {
    'zh-CN': '清除头像缓存',
    'zh-TW': '清除頭像快取',
    en: 'Clear avatar cache',
    ko: '아바타 캐시 지우기',
    ja: 'アバターキャッシュを消去',
  },
  // 删除已下载的仓库头像；下次进入 MOD 安装页会重新获取
  deletesDownloadedRepositoryAvatarsTheyWillBeFetchedAgainTheNextTimeYouOpenTheModInstallPage: {
    'zh-CN': '删除已下载的仓库头像；下次进入 MOD 安装页会重新获取',
    'zh-TW': '刪除已下載的倉庫頭像；下次進入 MOD 安裝頁會重新取得',
    en: 'Deletes downloaded repository avatars. They will be fetched again the next time you open the MOD install page.',
    ko: '다운로드한 저장소 아바타를 삭제합니다. 다음에 MOD 설치 페이지를 열면 다시 가져옵니다.',
    ja: 'ダウンロード済みのリポジトリのアバターを削除します。次に MOD インストールページを開いたときに再取得します。',
  },
  // 已清除 {count} 个头像缓存
  clearedCachedAvatarS: {
    'zh-CN': '已清除 {count} 个头像缓存',
    'zh-TW': '已清除 {count} 個頭像快取',
    en: 'Cleared {count} cached avatar(s)',
    ko: '아바타 캐시 {count}개를 지웠습니다',
    ja: 'アバターキャッシュを {count} 件消去しました',
  },
  // 清除地图缓存
  clearLevelCache: {
    'zh-CN': '清除地图缓存',
    'zh-TW': '清除地圖快取',
    en: 'Clear map cache',
    ko: '지도 캐시 지우기',
    ja: 'マップキャッシュを消去',
  },
  // 删除已解析的地图元数据；下次进入地图页会重新解析
  deletesParsedMapInfoItWillBeReparsedTheNextTimeYouOpenTheMapPage: {
    'zh-CN': '删除已解析的地图元数据；下次进入地图页会重新解析',
    'zh-TW': '刪除已解析的地圖中繼資料；下次進入地圖頁會重新解析',
    en: 'Deletes parsed map metadata. It will be parsed again the next time you open the map page.',
    ko: '파싱된 지도 메타데이터를 삭제합니다. 다음에 지도 페이지를 열면 다시 파싱합니다.',
    ja: '解析済みのマップメタデータを削除します。次にマップページを開いたときに再解析します。',
  },
  // 已清除 {count} 份地图缓存
  clearedCachedLevelInfoS: {
    'zh-CN': '已清除 {count} 份地图缓存',
    'zh-TW': '已清除 {count} 份地圖快取',
    en: 'Cleared {count} cached map record(s)',
    ko: '지도 캐시 {count}개를 지웠습니다',
    ja: 'マップキャッシュを {count} 件消去しました',
  },
  // 已打开开发者工具
  developerToolsOpened: {
    'zh-CN': '已打开开发者工具',
    'zh-TW': '已開啟開發人員工具',
    en: 'Developer tools opened',
    ko: '개발자 도구를 열었습니다',
    ja: '開発者ツールを開きました',
  },
  // 窗口状态已重置
  windowStateWasReset: {
    'zh-CN': '窗口状态已重置',
    'zh-TW': '視窗狀態已重設',
    en: 'Window state reset',
    ko: '창 상태를 초기화했습니다',
    ja: 'ウィンドウ状態をリセットしました',
  },
  // （该等级没有日志）
  noLogsAtThisLevel: {
    'zh-CN': '（该等级没有日志）',
    'zh-TW': '（該等級沒有日誌）',
    en: '(no logs at this level)',
    ko: '(이 등급에는 로그가 없습니다)',
    ja: '（このレベルにログはありません）',
  },
  // 清空日志
  clearLog: {
    'zh-CN': '清空日志',
    'zh-TW': '清空日誌',
    en: 'Clear log',
    ko: '로그 지우기',
    ja: 'ログを消去',
  },
  // 当前日志的全部内容将被清空，此操作不可撤销。继续吗？
  clearLogConfirm: {
    'zh-CN': '当前日志的全部内容将被清空，此操作不可撤销。继续吗？',
    'zh-TW': '目前日誌的全部內容將被清空，此操作無法復原。繼續嗎？',
    en: 'All content of this log will be cleared. This cannot be undone. Continue?',
    ko: '현재 로그의 모든 내용이 지워집니다. 이 작업은 되돌릴 수 없습니다. 계속할까요?',
    ja: '現在のログの内容をすべて消去します。この操作は元に戻せません。続行しますか？',
  },
  // 日志已清空
  logCleared: {
    'zh-CN': '日志已清空',
    'zh-TW': '日誌已清空',
    en: 'Log cleared',
    ko: '로그를 지웠습니다',
    ja: 'ログを消去しました',
  },
}
