import type { DomainMessages } from './types'

/**
 * tools 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const toolsUi: DomainMessages = {
  // 支持开发
  supportDevelopment: {
    'zh-CN': '支持开发',
    'zh-TW': '支持開發',
    en: 'Support development',
    ko: '개발 후원',
    ja: '開発を支援',
  },
  // 觉得好用，请作者喝杯咖啡
  buyTheAuthorACoffee: {
    'zh-CN': '觉得好用，请作者喝杯咖啡',
    'zh-TW': '覺得好用，請作者喝杯咖啡',
    en: 'Buy the author a coffee',
    ko: '유용했다면 커피 한 잔 부탁드립니다',
    ja: '役に立ったらコーヒー一杯お願いします',
  },
  // 所有功能永久免费，赞助用于服务器与开发动力。感谢支持！
  allFeaturesStayFreeSponsorshipsFundServersAndDevelopmentThankYou: {
    'zh-CN': '所有功能永久免费，赞助用于服务器与开发动力。感谢支持！',
    'zh-TW': '所有功能永久免費，贊助用於伺服器與開發動力。感謝支持！',
    en: 'All features stay free; sponsorships fund servers and development. Thank you!',
    ko: '모든 기능은 무료이며 후원은 서버와 개발에 사용됩니다. 감사합니다!',
    ja: 'すべての機能は無料です。支援はサーバーと開発に使われます。ありがとうございます！',
  },
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
  // 安装 MOD
  installMod: {
    'zh-CN': '安装 MOD',
    'zh-TW': '安裝 MOD',
    en: 'Install MOD',
    ko: 'MOD 설치',
    ja: 'MOD をインストール',
  },
  // 安装详情
  installDetails: {
    'zh-CN': '安装详情',
    'zh-TW': '安裝詳情',
    en: 'Install details',
    ko: '설치 세부 정보',
    ja: 'インストールの詳細',
  },
  // 本地文件
  localFile: {
    'zh-CN': '本地文件',
    'zh-TW': '本機檔案',
    en: 'Local file',
    ko: '로컬 파일',
    ja: 'ローカルファイル',
  },
  // 在线直链
  directUrl: {
    'zh-CN': '在线直链',
    'zh-TW': '線上直連',
    en: 'Direct URL',
    ko: '직접 링크',
    ja: '直リンク',
  },
  // 依赖
  dependency: {
    'zh-CN': '依赖',
    'zh-TW': '相依',
    en: 'Dependency',
    ko: '의존성',
    ja: '依存ファイル',
  },
  // 依赖表：名称列（目标 MOD）
  modColumn: {
    'zh-CN': 'MOD',
    'zh-TW': 'MOD',
    en: 'MOD',
    ko: 'MOD',
    ja: 'MOD',
  },
  // 依赖表：关系（requires / conflicts / compatible）
  relation: {
    'zh-CN': '关系',
    'zh-TW': '關係',
    en: 'Relation',
    ko: '관계',
    ja: '関係',
  },
  // 依赖表：关系值 requires
  relationRequires: {
    'zh-CN': '依赖',
    'zh-TW': '相依',
    en: 'Requires',
    ko: '의존',
    ja: '依存',
  },
  // 依赖表：关系值 conflicts（与指定版本区间互斥）
  relationConflicts: {
    'zh-CN': '排斥',
    'zh-TW': '排斥',
    en: 'Conflicts',
    ko: '충돌',
    ja: '競合',
  },
  // 依赖表：关系值 compatible
  relationCompatible: {
    'zh-CN': '兼容',
    'zh-TW': '相容',
    en: 'Compatible',
    ko: '호환',
    ja: '互換',
  },
  // 依赖表：关系只对测试版成立（与另一条发布线互斥）
  testLine: {
    'zh-CN': '测试版',
    'zh-TW': '測試版',
    en: 'Beta',
    ko: '테스트 버전',
    ja: 'テスト版',
  },
  // 依赖表：关系只对正式版成立
  stableLine: {
    'zh-CN': '正式版',
    'zh-TW': '正式版',
    en: 'Stable',
    ko: '정식 버전',
    ja: '正式版',
  },
  // 依赖表：版本要求
  requirement: {
    'zh-CN': '要求',
    'zh-TW': '要求',
    en: 'Requirement',
    ko: '요구 사항',
    ja: '要件',
  },
  // 依赖表：状态列
  status: {
    'zh-CN': '状态',
    'zh-TW': '狀態',
    en: 'Status',
    ko: '상태',
    ja: '状態',
  },
  // 依赖已满足（本地装了且版本在区间内）
  satisfied: {
    'zh-CN': '已满足',
    'zh-TW': '已滿足',
    en: 'Satisfied',
    ko: '충족',
    ja: '充足',
  },
  // 依赖未装、但线上有满足区间的版本可装
  installable: {
    'zh-CN': '可安装',
    'zh-TW': '可安裝',
    en: 'Installable',
    ko: '설치 가능',
    ja: 'インストール可',
  },
  // 依赖已装但版本不满足：线上有满足区间的版本，去把它换上
  updatable: {
    'zh-CN': '可更新',
    'zh-TW': '可更新',
    en: 'Updatable',
    ko: '업데이트 가능',
    ja: '更新可能',
  },
  // conflicts 关系未命中（对方不在冲突区间内）
  noConflict: {
    'zh-CN': '无冲突',
    'zh-TW': '無衝突',
    en: 'No conflict',
    ko: '충돌 없음',
    ja: '競合なし',
  },
  // compatible 关系满足
  compatible: {
    'zh-CN': '兼容',
    'zh-TW': '相容',
    en: 'Compatible',
    ko: '호환',
    ja: '互換',
  },
  // compatible 关系不满足
  incompatible: {
    'zh-CN': '不兼容',
    'zh-TW': '不相容',
    en: 'Incompatible',
    ko: '비호환',
    ja: '非互換',
  },
  // 本地安装
  localInstall: {
    'zh-CN': '本地安装',
    'zh-TW': '本機安裝',
    en: 'Local install',
    ko: '로컬 설치',
    ja: 'ローカルインストール',
  },
  // 单个 MOD
  singleMod: {
    'zh-CN': '单个 MOD',
    'zh-TW': '單一 MOD',
    en: 'Single MOD',
    ko: '단일 MOD',
    ja: '単一 MOD',
  },
  // MOD 包
  modPackage: {
    'zh-CN': 'MOD 包',
    'zh-TW': 'MOD 包',
    en: 'MOD package',
    ko: 'MOD 패키지',
    ja: 'MOD パッケージ',
  },
  // 把 {types} 拖到这里，或点击选择文件（{types} 形如「.dll / .zip」）
  dropFilesHereOrClickToChoose: {
    'zh-CN': '把 {types} 拖到这里，或点击选择文件',
    'zh-TW': '把 {types} 拖到這裡，或點擊選擇檔案',
    en: 'Drop a {types} file here, or click to choose one',
    ko: '{types} 파일을 여기로 끌어오거나 클릭해 선택하세요',
    ja: '{types} をここにドロップするか、クリックして選択します',
  },
  // 下载并解析
  downloadAndAnalyze: {
    'zh-CN': '下载并解析',
    'zh-TW': '下載並解析',
    en: 'Download and analyze',
    ko: '다운로드 후 분석',
    ja: 'ダウンロードして解析',
  },
  // 重新选择
  chooseAgain: {
    'zh-CN': '重新选择',
    'zh-TW': '重新選擇',
    en: 'Choose again',
    ko: '다시 선택',
    ja: '選び直す',
  },
  // 立即安装
  installNow: {
    'zh-CN': '立即安装',
    'zh-TW': '立即安裝',
    en: 'Install now',
    ko: '지금 설치',
    ja: '今すぐインストール',
  },
  // 这个压缩包里没有 DLL 文件
  thisArchiveContainsNoDllFiles: {
    'zh-CN': '这个压缩包里没有 DLL 文件',
    'zh-TW': '這個壓縮檔裡沒有 DLL 檔案',
    en: 'This archive contains no DLL files',
    ko: '이 압축 파일에는 DLL이 없습니다',
    ja: 'このアーカイブに DLL は含まれていません',
  },
  // 只支持 {types} 文件（扩展名白名单由来源卡片的 accept 决定）
  onlyFileTypesAreSupported: {
    'zh-CN': '只支持 {types} 文件',
    'zh-TW': '只支援 {types} 檔案',
    en: 'Only {types} files are supported',
    ko: '{types} 파일만 지원합니다',
    ja: '{types} のみ対応しています',
  },
  // {count} 个 DLL · {mods} 个 MOD · {deps} 个依赖
  dllsModsDependencies: {
    'zh-CN': '{count} 个 DLL · {mods} 个 MOD · {deps} 个依赖',
    'zh-TW': '{count} 個 DLL · {mods} 個 MOD · {deps} 個相依',
    en: '{count} DLLs · {mods} MODs · {deps} dependencies',
    ko: 'DLL {count}개 · MOD {mods}개 · 의존성 {deps}개',
    ja: 'DLL {count} 件 · MOD {mods} 件 · 依存 {deps} 件',
  },
  // 下载并安装
  downloadAndInstall: {
    'zh-CN': '下载并安装',
    'zh-TW': '下載並安裝',
    en: 'Download and install',
    ko: '다운로드 후 설치',
    ja: 'ダウンロードしてインストール',
  },
  // 选择本地的 .dll 或 .zip 文件，按标准布局安装到厨房目录。
  pickALocalDllOrZipFileItIsInstalledIntoTheKitchenFolderUsingTheStandardLayoutRules: {
    'zh-CN': '选择本地的 .dll 或 .zip 文件，按标准布局安装到厨房目录。',
    'zh-TW': '選擇本機的 .dll 或 .zip 檔案，依標準版面配置安裝到廚房目錄。',
    en: 'Pick a local .dll or .zip file; it is installed into the kitchen folder using the standard layout rules.',
    ko: '로컬 .dll 또는 .zip 파일을 선택하면 표준 배치 규칙으로 게임 폴더에 설치합니다',
    ja: 'ローカルの .dll または .zip を選ぶと、標準レイアウト規則でゲームフォルダーにインストールします',
  },
  // 粘贴 {types} 文件的下载直链，由管理器下载并解析。
  pasteADirectDownloadUrlForFiles: {
    'zh-CN': '粘贴 {types} 文件的下载直链，由管理器下载并解析。',
    'zh-TW': '貼上 {types} 檔案的下載直連，由管理器下載並解析。',
    en: 'Paste a direct download URL for a {types} file; the manager downloads and analyzes it.',
    ko: '{types} 파일의 직접 다운로드 링크를 붙여넣으면 관리자가 내려받아 분석합니다',
    ja: '{types} ファイルの直リンクを貼り付けると、マネージャーがダウンロードして解析します',
  },
  // 请输入 {types} 文件的直链网址
  enterADirectUrlForFiles: {
    'zh-CN': '请输入 {types} 文件的直链网址',
    'zh-TW': '請輸入 {types} 檔案的直連網址',
    en: 'Enter a direct URL for a {types} file',
    ko: '{types} 파일의 직접 링크를 입력하세요',
    ja: '{types} ファイルの直リンクを入力してください',
  },
  // 删除 MOD 包
  deleteModPackage: {
    'zh-CN': '删除 MOD 包',
    'zh-TW': '刪除 MOD 包',
    en: 'Delete MOD package',
    ko: 'MOD 패키지 삭제',
    ja: 'MOD パッケージを削除',
  },
  // 「{name}」的 {count} 个文件将被整包移入回收站，可在回收站中恢复。继续？
  allFilesOfWillBeMovedToTheRecycleBinAndCanBeRestoredContinue: {
    'zh-CN': '「{name}」的 {count} 个文件将被整包移入回收站，可在回收站中恢复。继续？',
    'zh-TW': '「{name}」的 {count} 個檔案將被整包移入資源回收筒，可在資源回收筒中還原。繼續？',
    en: 'All {count} files of "{name}" will be moved to the recycle bin and can be restored. Continue?',
    ko: '"{name}"의 파일 {count}개를 패키지째 휴지통으로 옮깁니다. 복원할 수 있습니다. 계속할까요?',
    ja: '「{name}」の {count} ファイルをまとめてゴミ箱へ移動します（復元可能）。続行しますか？',
  },
  // 「{name}」的 {count} 个文件将被直接删除，无法恢复。继续？
  allFilesOfWillBeDeletedPermanentlyThisCannotBeUndoneContinue: {
    'zh-CN': '「{name}」的 {count} 个文件将被直接删除，无法恢复。继续？',
    'zh-TW': '「{name}」的 {count} 個檔案將被直接刪除，無法還原。繼續？',
    en: 'All {count} files of "{name}" will be deleted permanently. This cannot be undone. Continue?',
    ko: '"{name}"의 파일 {count}개를 바로 삭제합니다. 복원할 수 없습니다. 계속할까요?',
    ja: '「{name}」の {count} ファイルを直接削除します。元に戻せません。続行しますか？',
  },
  // 配置编辑
  editConfig: {
    'zh-CN': '配置编辑',
    'zh-TW': '設定編輯',
    en: 'Edit config',
    ko: '설정 편집',
    ja: '設定を編集',
  },
  // API 凭据
  apiCredentials: {
    'zh-CN': 'API 凭据',
    'zh-TW': 'API 憑證',
    en: 'API credentials',
    ko: 'API 자격 증명',
    ja: 'API 認証情報',
  },
  // 访问令牌
  accessTokens: {
    'zh-CN': '访问令牌',
    'zh-TW': '存取權杖',
    en: 'Access tokens',
    ko: '액세스 토큰',
    ja: 'アクセストークン',
  },
  // 配置后 API 请求直接使用 Token，避免匿名限流
  withATokenApiRequestsUseItDirectlyInsteadOfTheAnonymousQuota: {
    'zh-CN': '配置后 API 请求直接使用 Token，避免匿名限流',
    'zh-TW': '設定後 API 請求直接使用 Token，避免匿名限流',
    en: 'With a token, API requests use it directly instead of the anonymous quota',
    ko: '토큰을 설정하면 API 요청이 익명 대신 토큰을 사용합니다',
    ja: 'トークンを設定すると API リクエストは匿名ではなくトークンを使用します',
  },
  // 使用 Authorization: Bearer 请求头
  sentAsAnAuthorizationBearerHeader: {
    'zh-CN': '使用 Authorization: Bearer 请求头',
    'zh-TW': '使用 Authorization: Bearer 請求頭',
    en: 'Sent as an Authorization: Bearer header',
    ko: 'Authorization: Bearer 헤더로 전송됩니다',
    ja: 'Authorization: Bearer ヘッダーで送信します',
  },
  // 使用 access_token 请求头
  sentAsAnAccessTokenHeader: {
    'zh-CN': '使用 access_token 请求头',
    'zh-TW': '使用 access_token 請求頭',
    en: 'Sent as an access_token header',
    ko: 'access_token 헤더로 전송됩니다',
    ja: 'access_token ヘッダーで送信します',
  },
  // 已配置
  configured: {
    'zh-CN': '已配置',
    'zh-TW': '已設定',
    en: 'Configured',
    ko: '설정됨',
    ja: '設定済み',
  },
  // 已保存，留空则不修改
  savedLeaveBlankToKeep: {
    'zh-CN': '已保存，留空则不修改',
    'zh-TW': '已儲存，留空則不修改',
    en: 'Saved — leave blank to keep',
    ko: '저장됨 — 비워 두면 유지',
    ja: '保存済み — 空欄なら変更しません',
  },
  // 粘贴 Gitee 私人令牌
  pasteYourGiteePrivateToken: {
    'zh-CN': '粘贴 Gitee 私人令牌',
    'zh-TW': '貼上 Gitee 私人權杖',
    en: 'Paste your Gitee private token',
    ko: 'Gitee 개인 토큰을 붙여넣으세요',
    ja: 'Gitee のプライベートトークンを貼り付け',
  },
  // 清除全部
  clearAll: {
    'zh-CN': '清除全部',
    'zh-TW': '清除全部',
    en: 'Clear all',
    ko: '모두 지우기',
    ja: 'すべて削除',
  },
  // 已清除
  cleared: {
    'zh-CN': '已清除',
    'zh-TW': '已清除',
    en: 'Cleared',
    ko: '지워짐',
    ja: '削除しました',
  },
  // 配置 Token 后，API 请求直接使用 Token；Token 自身被限流时才回退到代理。
  withTokensSetRequestsUseThemDirectlyAndOnlyFallBackToProxiesWhenTheTokenItselfIsRateLimited: {
    'zh-CN': '配置 Token 后，API 请求直接使用 Token；Token 自身被限流时才回退到代理。',
    'zh-TW': '設定 Token 後，API 請求直接使用 Token；Token 自身被限流時才回退到代理。',
    en: 'With tokens set, requests use them directly and only fall back to proxies when the token itself is rate limited.',
    ko: '토큰을 설정하면 요청에 직접 사용하고, 토큰이 제한될 때만 프록시로 전환합니다.',
    ja: 'トークンを設定すると直接使用し、トークンが制限された場合のみプロキシに切り替えます。',
  },
  // 说明
  details: {
    'zh-CN': '说明',
    'zh-TW': '說明',
    en: 'Details',
    ko: '설명',
    ja: '説明',
  },
  // 下载安装
  download: {
    'zh-CN': '下载安装',
    'zh-TW': '下載安裝',
    en: 'Download',
    ko: '다운로드 및 설치',
    ja: 'ダウンロードとインストール',
  },
  // 未保存
  unsaved: {
    'zh-CN': '未保存',
    'zh-TW': '未儲存',
    en: 'Unsaved',
    ko: '저장 안 됨',
    ja: '未保存',
  },
  // 有未保存的修改，已取消
  unsavedChangesWereDiscarded: {
    'zh-CN': '有未保存的修改，已取消',
    'zh-TW': '有未儲存的修改，已取消',
    en: 'Unsaved changes were discarded',
    ko: '저장하지 않은 변경 사항이 있습니다',
    ja: '未保存の変更があります',
  },
  // 先刷新源文档，再并行刷新各仓库
  reloadSourceDocumentsThenRefreshRepositoriesInParallel: {
    'zh-CN': '先刷新源文档，再并行刷新各仓库',
    'zh-TW': '先重新整理來源文件，再並行整理各倉庫',
    en: 'Reload source documents, then refresh repositories in parallel',
    ko: '소스 문서를 먼저 새로 고친 뒤 저장소를 병렬로 새로 고침',
    ja: 'ソース文書を更新してから各リポジトリを並列更新',
  },
  // 仓库源刷新失败，请稍后重试
  sourceRefreshFailedTryAgainLater: {
    'zh-CN': '仓库源刷新失败，请稍后重试',
    'zh-TW': '倉庫源重新整理失敗，請稍後重試',
    en: 'Source refresh failed. Try again later.',
    ko: '소스 새로 고침 실패, 잠시 후 다시 시도하세요',
    ja: 'ソースの更新に失敗しました。後で再試行してください',
  },
  // 中国大陆地区
  mainlandChina: {
    'zh-CN': '中国大陆地区',
    'zh-TW': '中國大陸地區',
    en: 'Mainland China',
    ko: '중국 본토',
    ja: '中国本土',
  },
  // 国际地区
  international: {
    'zh-CN': '国际地区',
    'zh-TW': '國際地區',
    en: 'International',
    ko: '해외',
    ja: '海外',
  },
  // 许可证
  license: {
    'zh-CN': '许可证',
    'zh-TW': '授權條款',
    en: 'License',
    ko: '라이선스',
    ja: 'ライセンス',
  },
  // 最后更新
  updated: {
    'zh-CN': '最后更新',
    'zh-TW': '最後更新',
    en: 'Updated',
    ko: '최종 업데이트',
    ja: '最終更新',
  },
  // 标识
  identifier: {
    'zh-CN': '标识',
    'zh-TW': '識別碼',
    en: 'Identifier',
    ko: '식별자',
    ja: '識別子',
  },
  // 版本绑定
  versionBinding: {
    'zh-CN': '版本绑定',
    'zh-TW': '版本綁定',
    en: 'Version binding',
    ko: '버전 바인딩',
    ja: 'バージョン紐付け',
  },
  // 源码
  source: {
    'zh-CN': '源码',
    'zh-TW': '原始碼',
    en: 'Source',
    ko: '소스',
    ja: 'ソース',
  },
  // Release
  release: {
    'zh-CN': 'Release',
    'zh-TW': 'Release',
    en: 'Release',
    ko: 'Release',
    ja: 'Release',
  },
  // 该仓库没有备注
  thisRepositoryHasNoNote: {
    'zh-CN': '该仓库没有备注',
    'zh-TW': '該倉庫沒有備註',
    en: 'This repository has no note',
    ko: '이 저장소에는 메모가 없습니다',
    ja: 'このリポジトリにはメモがありません',
  },
  // 正在加载仓库说明
  loadingRepositoryReadme: {
    'zh-CN': '正在加载仓库说明',
    'zh-TW': '正在載入倉庫說明',
    en: 'Loading repository README',
    ko: '저장소 README 불러오는 중',
    ja: 'リポジトリの説明を読み込み中',
  },
  // 没有匹配的仓库
  noMatchingRepositories: {
    'zh-CN': '没有匹配的仓库',
    'zh-TW': '沒有符合的倉庫',
    en: 'No matching repositories',
    ko: '일치하는 저장소가 없습니다',
    ja: '一致するリポジトリがありません',
  },
  // 部分启用
  partiallyEnabled: {
    'zh-CN': '部分启用',
    'zh-TW': '部分啟用',
    en: 'Partially enabled',
    ko: '일부 활성',
    ja: '一部有効',
  },
  // 可更新
  update: {
    'zh-CN': '可更新',
    'zh-TW': '可更新',
    en: 'Update',
    ko: '업데이트 가능',
    ja: '更新あり',
  },
  // 刷新失败
  refreshFailed: {
    'zh-CN': '刷新失败',
    'zh-TW': '重新整理失敗',
    en: 'Refresh failed',
    ko: '새로 고침 실패',
    ja: '更新失敗',
  },
  // 已最新
  upToDate: {
    'zh-CN': '已最新',
    'zh-TW': '已最新',
    en: 'Up to date',
    ko: '최신 상태',
    ja: '最新です',
  },
  // 低版本
  olderVersion: {
    'zh-CN': '低版本',
    'zh-TW': '低版本',
    en: 'Older version',
    ko: '이전 버전',
    ja: '旧バージョン',
  },
  // 版本未知
  versionUnknown: {
    'zh-CN': '版本未知',
    'zh-TW': '版本未知',
    en: 'Version unknown',
    ko: '버전 미확인',
    ja: 'バージョン不明',
  },
  // 检测中
  checking: {
    'zh-CN': '检测中',
    'zh-TW': '檢測中',
    en: 'Checking',
    ko: '확인 중',
    ja: '確認中',
  },
  // 刷新中（绑定仓库 / 地图源的 Release 正在重抓）
  refreshing: {
    'zh-CN': '刷新中',
    'zh-TW': '重新整理中',
    en: 'Refreshing',
    ko: '새로 고침 중',
    ja: '更新中',
  },
  // 正在检测更新
  checkingForUpdates: {
    'zh-CN': '正在检测更新',
    'zh-TW': '正在檢查更新',
    en: 'Checking for updates',
    ko: '업데이트 확인 중',
    ja: '更新を確認中',
  },
  // 排序
  sort: {
    'zh-CN': '排序',
    'zh-TW': '排序',
    en: 'Sort',
    ko: '정렬',
    ja: '並び替え',
  },
  // 重新扫描本地 MOD
  rescanLocalMods: {
    'zh-CN': '重新扫描本地 MOD',
    'zh-TW': '重新掃描本機 MOD',
    en: 'Rescan local MODs',
    ko: '로컬 MOD 다시 검사',
    ja: 'ローカル MOD を再スキャン',
  },
  // 已重新扫描本地 MOD
  localModsRescanned: {
    'zh-CN': '已重新扫描本地 MOD',
    'zh-TW': '已重新掃描本機 MOD',
    en: 'Local MODs rescanned',
    ko: '로컬 MOD를 다시 검사했습니다',
    ja: 'ローカル MOD を再スキャンしました',
  },
  // 准备中
  preparing: {
    'zh-CN': '准备中',
    'zh-TW': '準備中',
    en: 'Preparing',
    ko: '준비 중',
    ja: '準備中',
  },
  // 准备中…
  preparing2: {
    'zh-CN': '准备中…',
    'zh-TW': '準備中…',
    en: 'Preparing…',
    ko: '준비 중…',
    ja: '準備中…',
  },
  // 正在连接下载源…
  connectingToDownloadSource: {
    'zh-CN': '正在连接下载源…',
    'zh-TW': '正在連線下載來源…',
    en: 'Connecting to download source…',
    ko: '다운로드 소스에 연결 중…',
    ja: 'ダウンロード元に接続中…',
  },
  // 正在切换更快的源…
  switchingToAFasterSource: {
    'zh-CN': '正在切换更快的源…',
    'zh-TW': '正在切換更快的來源…',
    en: 'Switching to a faster source…',
    ko: '더 빠른 소스로 전환 중…',
    ja: 'より速いソースに切り替え中…',
  },
  // 正在下载
  downloading: {
    'zh-CN': '正在下载',
    'zh-TW': '正在下載',
    en: 'Downloading',
    ko: '다운로드 중',
    ja: 'ダウンロード中',
  },
  // 已完成
  completed: {
    'zh-CN': '已完成',
    'zh-TW': '已完成',
    en: 'Completed',
    ko: '완료',
    ja: '完了',
  },
  // {count} 源竞速
  sourcesRacing: {
    'zh-CN': '{count} 源竞速',
    'zh-TW': '{count} 源競速',
    en: '{count} sources racing',
    ko: '{count}개 소스 경쟁',
    ja: '{count} ソース競争',
  },
  // 第 {current}/{total} 次尝试
  attempt: {
    'zh-CN': '第 {current}/{total} 次尝试',
    'zh-TW': '第 {current}/{total} 次嘗試',
    en: 'Attempt {current}/{total}',
    ko: '{current}/{total}번째 시도',
    ja: '{current}/{total} 回目の試行',
  },
  // 选择版本
  selectVersion: {
    'zh-CN': '选择版本',
    'zh-TW': '選擇版本',
    en: 'Select version',
    ko: '버전 선택',
    ja: 'バージョンを選択',
  },
  // 选择文件
  selectFile: {
    'zh-CN': '选择文件',
    'zh-TW': '選擇檔案',
    en: 'Select file',
    ko: '파일 선택',
    ja: 'ファイルを選択',
  },
  // 大小
  size: {
    'zh-CN': '大小',
    'zh-TW': '大小',
    en: 'Size',
    ko: '크기',
    ja: 'サイズ',
  },
  // 下载 {count} 次
  downloads: {
    'zh-CN': '下载 {count} 次',
    'zh-TW': '下載 {count} 次',
    en: '{count} downloads',
    ko: '{count}회 다운로드',
    ja: '{count} 回ダウンロード',
  },
  // 另有 {count} 个匹配文件
  moreMatchingFiles: {
    'zh-CN': '另有 {count} 个匹配文件',
    'zh-TW': '另有 {count} 個符合檔案',
    en: '{count} more matching files',
    ko: '{count}개 추가 일치 파일',
    ja: '他に {count} 件の一致ファイル',
  },
  // 该版本没有命中资产规则的文件
  noAssetInThisReleaseMatchesTheAssetRules: {
    'zh-CN': '该版本没有命中资产规则的文件',
    'zh-TW': '該版本沒有符合資產規則的檔案',
    en: 'No asset in this release matches the asset rules',
    ko: '이 버전에는 자산 규칙과 일치하는 파일이 없습니다',
    ja: 'このバージョンにアセット規則と一致するファイルがありません',
  },
  // 仓库文件模式直接下载该文件
  repositoryFileModeDownloadsThisFileDirectly: {
    'zh-CN': '仓库文件模式直接下载该文件',
    'zh-TW': '倉庫檔案模式直接下載該檔案',
    en: 'Repository-file mode downloads this file directly',
    ko: '저장소 파일 모드는 이 파일을 바로 받습니다',
    ja: 'リポジトリファイルモードはこのファイルを直接ダウンロードします',
  },
  // 当前渠道没有社区仓库
  noCommunityRepositoriesInThisChannel: {
    'zh-CN': '当前渠道没有社区仓库',
    'zh-TW': '目前渠道沒有社群倉庫',
    en: 'No community repositories in this channel',
    ko: '이 채널에는 커뮤니티 저장소가 없습니다',
    ja: 'このチャンネルにはコミュニティリポジトリがありません',
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
}
