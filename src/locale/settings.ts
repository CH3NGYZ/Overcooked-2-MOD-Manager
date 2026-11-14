import type { DomainMessages } from './types'

/**
 * settings 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const settings: DomainMessages = {
  dataSources: { 'zh-CN': '数据来源', 'zh-TW': '資料來源', en: 'Data sources', ko: '데이터 소스', ja: 'データソース' },
  dataProviderGitcode: { 'zh-CN': 'GitCode（中国大陆 - 推荐）', 'zh-TW': 'GitCode（中國大陸 - 推薦）', en: 'GitCode (Mainland China - Recommended)', ko: 'GitCode (중국 본토 - 권장)', ja: 'GitCode（中国本土 - 推奨）' },
  dataProviderGitee: { 'zh-CN': 'Gitee (中国大陆 - 备用)', 'zh-TW': 'Gitee（中國大陸 - 備用）', en: 'Gitee (Mainland China - Backup)', ko: 'Gitee (중국 본토 - 예비)', ja: 'Gitee（中国本土 - 予備）' },
  dataProviderGithub: { 'zh-CN': 'Github (国际)', 'zh-TW': 'Github（國際）', en: 'Github (International)', ko: 'Github (국제)', ja: 'Github（国際）' },
  dataProviderCloudflare: { 'zh-CN': 'Cloudflare (全球)', 'zh-TW': 'Cloudflare（全球）', en: 'Cloudflare (Global)', ko: 'Cloudflare (전 세계)', ja: 'Cloudflare（グローバル）' },
  dataProviderBuiltin: { 'zh-CN': '内置数据源 (保底)', 'zh-TW': '內建資料來源（保底）', en: 'Bundled source (Fallback)', ko: '내장 데이터 소스 (최종 대안)', ja: '内蔵データソース（最終手段）' },
  dataProviderBuiltinShort: { 'zh-CN': '内置源', 'zh-TW': '內建源', en: 'Bundled', ko: '내장 소스', ja: '内蔵ソース' },
  dataProviderHint: {
    'zh-CN': '本次启动有效。中国大陆：GitCode → Gitee → Cloudflare → 内置；国际地区：GitHub → Cloudflare → 内置。Gitee 需在独立原生窗口中完成验证并点击下载。官方文档从同一源码 ZIP 读取，此处显示实际来源。',
    'zh-TW': '僅在本次啟動有效。中國大陸：GitCode → Gitee → Cloudflare → 內建；國際地區：GitHub → Cloudflare → 內建。Gitee 需在獨立原生視窗中完成驗證並點擊下載。官方文件從同一原始碼 ZIP 讀取，此處顯示實際來源。',
    en: 'Applies to this session. Mainland China: GitCode → Gitee → Cloudflare → Bundled; international: GitHub → Cloudflare → Bundled. For Gitee, complete verification and click download in a separate native window. Official documents come from one source ZIP; the active source is shown here.',
    ko: '이번 실행에만 적용됩니다. 중국 본토: GitCode → Gitee → Cloudflare → 내장, 그 외 지역: GitHub → Cloudflare → 내장. Gitee는 별도 네이티브 창에서 인증을 완료한 뒤 다운로드를 클릭해야 합니다. 공식 문서는 동일한 소스 ZIP에서 읽으며 여기에 실제 사용 중인 소스를 표시합니다.',
    ja: '今回の起動中のみ有効です。中国本土：GitCode → Gitee → Cloudflare → 内蔵、その他の地域：GitHub → Cloudflare → 内蔵。Gitee は独立したネイティブウィンドウで認証を完了し、ダウンロードをクリックしてください。公式ドキュメントは同じソース ZIP から読み込み、ここに実際の取得元を表示します。',
  },
  dataProviderChanged: {
    'zh-CN': '已切换数据来源并刷新 MOD / 地图源',
    'zh-TW': '已切換資料來源並重新整理 MOD / 地圖來源',
    en: 'Data source changed and MOD / map sources refreshed.',
    ko: '데이터 소스를 변경하고 MOD / 지도 소스를 새로 고쳤습니다.',
    ja: 'データソースを切り替え、MOD / マップソースを更新しました。',
  },
  dataProviderNetworkFallback: {
    'zh-CN': '未能完成数据来源刷新，请查看当前实际来源并重试刷新 MOD / 地图仓库',
    'zh-TW': '未能完成資料來源重新整理，請查看目前實際來源並重試重新整理 MOD / 地圖倉庫',
    en: 'Could not finish refreshing the data source. Check the active source and retry refreshing MOD / map repositories.',
    ko: '데이터 소스 새로 고침을 완료하지 못했습니다. 현재 사용 중인 소스를 확인하고 MOD / 지도 저장소를 다시 새로 고치세요.',
    ja: 'データソースの更新を完了できませんでした。実際の取得元を確認し、MOD / マップリポジトリの更新を再試行してください。',
  },
  // 路径管理
  paths: {
    'zh-CN': '路径管理',
    'zh-TW': '路徑管理',
    en: 'Paths',
    ko: '경로 관리',
    ja: 'パス管理',
  },
  // 游戏目录
  gameDirectory: {
    'zh-CN': '游戏目录',
    'zh-TW': '遊戲目錄',
    en: 'Game directory',
    ko: '게임 폴더',
    ja: 'ゲームフォルダー',
  },
  // 恢复默认路径
  resetPath: {
    'zh-CN': '恢复默认路径',
    'zh-TW': '還原預設路徑',
    en: 'Reset path',
    ko: '기본 경로로 복원',
    ja: '既定パスに戻す',
  },
  // 打开路径
  openFolder: {
    'zh-CN': '打开路径',
    'zh-TW': '開啟路徑',
    en: 'Open folder',
    ko: '경로 열기',
    ja: 'パスを開く',
  },
  // 选择路径
  choosePath: {
    'zh-CN': '选择路径',
    'zh-TW': '選擇路徑',
    en: 'Choose path',
    ko: '경로 선택',
    ja: 'パスを選択',
  },
  // 程序更新
  // 没有开关键，右侧状态标签
  alwaysOn: {
    'zh-CN': '始终开启',
    'zh-TW': '永遠開啟',
    en: 'Always on',
    ko: '항상 켜짐',
    ja: '常に有効',
  },
  appUpdate: {
    'zh-CN': '程序更新',
    'zh-TW': '程式更新',
    en: 'App update',
    ko: '앱 업데이트',
    ja: 'アプリの更新',
  },
  // 性能
  performance: {
    'zh-CN': '性能',
    'zh-TW': '效能',
    en: 'Performance',
    ko: '성능',
    ja: 'パフォーマンス',
  },
  // MOD 元数据并行读取
  modMetadataConcurrency: {
    'zh-CN': 'MOD 元数据并行读取',
    'zh-TW': 'MOD 中繼資料並行讀取',
    en: 'Parallel MOD metadata reads',
    ko: 'MOD 메타데이터 병렬 읽기',
    ja: 'MOD メタデータ並列読み込み',
  },
  // 读取 MOD 的 DLL 元数据时同时处理几个文件；默认使用全部核心
  modMetadataConcurrencyHint: {
    'zh-CN': '读取 MOD 的 DLL 元数据时同时处理几个文件；默认使用全部核心',
    'zh-TW': '讀取 MOD 的 DLL 中繼資料時同時處理幾個檔案；預設使用全部核心',
    en: 'How many DLLs are inspected at once; defaults to all CPU cores',
    ko: 'MOD의 DLL 메타데이터를 동시에 몇 개까지 읽을지; 기본값은 모든 CPU 코어',
    ja: 'MOD の DLL メタデータを同時にいくつ読むか。既定は全 CPU コア',
  },
  // 地图信息并行读取
  mapInfoConcurrency: {
    'zh-CN': '地图信息并行读取',
    'zh-TW': '地圖資訊並行讀取',
    en: 'Parallel map info reads',
    ko: '지도 정보 병렬 읽기',
    ja: 'マップ情報の並列読み込み',
  },
  // 解析地图 info 时同时处理几张地图；默认全部核心
  mapInfoConcurrencyHint: {
    'zh-CN': '解析地图 info 时同时处理几张地图；默认全部核心',
    'zh-TW': '解析地圖 info 時同時處理幾張地圖；預設全部核心',
    en: 'How many map `info` files are parsed at once; defaults to all CPU cores',
    ko: '지도 info를 동시에 몇 개까지 파싱할지; 기본값은 모든 CPU 코어',
    ja: 'マップ info を同時にいくつ解析するか。既定は全 CPU コア',
  },
  mapCompressionConcurrency: {
    'zh-CN': '地图资源转换并行数', 'zh-TW': '地圖資源轉換並行數',
    en: 'Parallel map bundle conversions', ko: '지도 리소스 병렬 변환', ja: 'マップリソースの並列変換数',
  },
  mapCompressionConcurrencyHint: {
    'zh-CN': '后台同时将几个 LZMA 资源转换为 LZ4HC3；默认全部核心，下次转换生效',
    'zh-TW': '背景同時將幾個 LZMA 資源轉換為 LZ4HC3；預設全部核心，下次轉換生效',
    en: 'LZMA bundles converted to LZ4HC3 at once; defaults to all cores, applies to the next batch',
    ko: '동시에 LZ4HC3로 변환할 LZMA 리소스 수; 기본값은 모든 코어이며 다음 작업부터 적용',
    ja: '同時に LZ4HC3 に変換する LZMA リソース数。既定は全コア、次の処理から適用',
  },
  diyChefPreviewConcurrency: {
    'zh-CN': '暖暖缩略图渲染并行数', 'zh-TW': '暖暖縮圖渲染並行數',
    en: 'Parallel chef thumbnail rendering', ko: '셰프 미리보기 병렬 렌더링', ja: 'シェフサムネイルの並列生成数',
  },
  diyChefPreviewConcurrencyHint: {
    'zh-CN': '同时准备几个厨师的模型和贴图；默认全部核心，已缓存的缩略图直接复用',
    'zh-TW': '同時準備幾個廚師的模型和貼圖；預設全部核心，已快取的縮圖直接重用',
    en: 'How many chef models and textures are prepared at once; defaults to all cores and reuses cached thumbnails',
    ko: '동시에 준비할 셰프 모델과 텍스처 수입니다. 기본값은 모든 코어이며 캐시된 미리보기를 재사용합니다',
    ja: '同時にモデルとテクスチャを準備するシェフ数。既定は全コアで、保存済みサムネイルを再利用します',
  },
  // 全部核心
  allCores: {
    'zh-CN': '全部核心',
    'zh-TW': '全部核心',
    en: 'All cores',
    ko: '모든 코어',
    ja: 'すべてのコア',
  },
}
