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
  // 游戏与 BepInEx 框架所在目录；DIYLevel 自定义地图框架路径后续也在此管理
  folderContainingTheGameAndBepinexDiylevelMapFrameworkPathsWillLiveHereToo: {
    'zh-CN': '游戏与 BepInEx 框架所在目录；DIYLevel 自定义地图框架路径后续也在此管理',
    'zh-TW': '遊戲與 BepInEx 框架所在目錄；DIYLevel 自訂地圖框架路徑後續也在此管理',
    en: 'Folder containing the game and BepInEx; DIYLevel map framework paths will live here too',
    ko: '게임과 BepInEx가 있는 폴더. DIYLevel 지도 프레임워크 경로도 여기서 관리합니다',
    ja: 'ゲームとBepInExのあるフォルダー。DIYLevel マップフレームワークのパスもここで管理します',
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
}
