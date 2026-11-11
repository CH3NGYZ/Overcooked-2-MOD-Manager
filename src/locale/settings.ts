import type { DomainMessages } from './types'

/**
 * settings 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const settings: DomainMessages = {
  dataSources: { 'zh-CN': '数据来源', 'zh-TW': '資料來源', en: 'Data sources', ko: '데이터 소스', ja: 'データソース' },
  dataProviderGithub: { 'zh-CN': 'GitHub（国际）', 'zh-TW': 'GitHub（國際）', en: 'GitHub (International)', ko: 'GitHub (국제)', ja: 'GitHub（国際）' },
  dataProviderGitee: { 'zh-CN': 'Gitee（中国大陆）', 'zh-TW': 'Gitee（中國大陸）', en: 'Gitee (Mainland China)', ko: 'Gitee (중국 본토)', ja: 'Gitee（中国本土）' },
  dataProviderBuiltin: { 'zh-CN': '内置', 'zh-TW': '內建', en: 'Bundled', ko: '내장', ja: '内蔵' },
  dataProviderHint: {
    'zh-CN': '统一选择 MOD 源、地图源、代理和网络计划。每次启动按电脑地区初始化，本次运行可手动更改。',
    'zh-TW': '統一選擇 MOD 來源、地圖來源、代理與網路計畫。每次啟動依電腦地區初始化，本次執行可手動變更。',
    en: 'Choose the source for MODs, maps, proxies and network plans. Initialized from your computer region at each launch; you can change it for this session.',
    ko: 'MOD, 지도, 프록시 및 네트워크 계획의 소스를 함께 선택합니다. 시작할 때 컴퓨터 지역에 따라 초기화되며 이번 실행 중에는 직접 바꿀 수 있습니다.',
    ja: 'MOD・マップ・プロキシ・ネットワークプランの取得元をまとめて選択します。起動ごとにコンピューターの地域で初期化され、実行中は手動で変更できます。',
  },
  dataProviderChanged: {
    'zh-CN': '已切换数据来源，刷新 MOD / 地图仓库后应用',
    'zh-TW': '已切換資料來源，重新整理 MOD / 地圖倉庫後套用',
    en: 'Data source changed. Refresh the MOD / map repositories to apply.',
    ko: '데이터 소스를 변경했습니다. MOD / 지도 저장소를 새로 고치면 적용됩니다.',
    ja: 'データソースを変更しました。MOD / マップリポジトリを更新すると反映されます。',
  },
  dataProviderNetworkFallback: {
    'zh-CN': '已切换数据来源；网络配置暂时使用内置数据，刷新 MOD / 地图仓库后应用源变更',
    'zh-TW': '已切換資料來源；網路設定暫時使用內建資料，重新整理 MOD / 地圖倉庫後套用來源變更',
    en: 'Data source changed; using bundled network settings for now. Refresh the MOD / map repositories to apply the source change.',
    ko: '데이터 소스를 변경했습니다. 네트워크 설정은 일시적으로 내장 데이터를 사용합니다. MOD / 지도 저장소를 새로 고쳐 소스 변경을 적용하세요.',
    ja: 'データソースを変更しました。ネットワーク設定は一時的に内蔵データを使用します。MOD / マップリポジトリを更新すると取得元の変更が反映されます。',
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
