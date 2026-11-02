import type { DomainMessages } from './types'

/**
 * settings 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const settings: DomainMessages = {
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
  // 解析地图 info 时同时处理几张地图；默认 4
  mapInfoConcurrencyHint: {
    'zh-CN': '解析地图 info 时同时处理几张地图；默认 4',
    'zh-TW': '解析地圖 info 時同時處理幾張地圖；預設 4',
    en: 'How many map `info` files are parsed at once; defaults to 4',
    ko: '지도 info를 동시에 몇 개까지 파싱할지; 기본값 4',
    ja: 'マップ info を同時にいくつ解析するか。既定 4',
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
