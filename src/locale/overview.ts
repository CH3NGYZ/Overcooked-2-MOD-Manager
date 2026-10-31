import type { DomainMessages } from './types'

/**
 * overview 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const overview: DomainMessages = {
  // 已安装 MOD
  installedMods: {
    'zh-CN': '已安装 MOD',
    'zh-TW': '已安裝 MOD',
    en: 'Installed MODs',
    ko: '설치된 MOD',
    ja: 'インストール済みMOD',
  },
  // 框架状态
  framework: {
    'zh-CN': '框架状态',
    'zh-TW': '框架狀態',
    en: 'Framework',
    ko: '프레임워크 상태',
    ja: 'フレームワーク状態',
  },
  // 游戏路径
  gamePath: {
    'zh-CN': '游戏路径',
    'zh-TW': '遊戲路徑',
    en: 'Game path',
    ko: '게임 경로',
    ja: 'ゲームパス',
  },
  // 待处理事项
  pending: {
    'zh-CN': '待处理事项',
    'zh-TW': '待處理事項',
    en: 'Pending',
    ko: '처리 대기',
    ja: '保留項目',
  },
  // 已启用 {count} 个
  enabled: {
    'zh-CN': '已启用 {count} 个',
    'zh-TW': '已啟用 {count} 個',
    en: '{count} enabled',
    ko: '{count}개 활성',
    ja: '{count} 個有効',
  },
  // 正常
  ready: {
    'zh-CN': '正常',
    'zh-TW': '正常',
    en: 'Ready',
    ko: '정상',
    ja: '正常',
  },
  // 未安装
  notInstalled: {
    'zh-CN': '未安装',
    'zh-TW': '未安裝',
    en: 'Not installed',
    ko: '설치 안 됨',
    ja: '未インストール',
  },
  // 已验证
  verified: {
    'zh-CN': '已验证',
    'zh-TW': '已驗證',
    en: 'Verified',
    ko: '검증됨',
    ja: '検証済み',
  },
  // 目录异常
  pathInvalid: {
    'zh-CN': '目录异常',
    'zh-TW': '目錄異常',
    en: 'Path invalid',
    ko: '경로 오류',
    ja: 'パス異常',
  },
  // 未设置
  notSet: {
    'zh-CN': '未设置',
    'zh-TW': '未設定',
    en: 'Not set',
    ko: '설정 안 됨',
    ja: '未設定',
  },
  // 无法探测游戏目录
  cannotDetectGameDirectory: {
    'zh-CN': '无法探测游戏目录',
    'zh-TW': '無法偵測遊戲目錄',
    en: 'Cannot detect game directory',
    ko: '게임 폴더를 감지할 수 없습니다',
    ja: 'ゲームフォルダーを検出できません',
  },
  // 暂无可更新内容
  nothingToUpdate: {
    'zh-CN': '暂无可更新内容',
    'zh-TW': '暫無可更新內容',
    en: 'Nothing to update',
    ko: '업데이트할 항목 없음',
    ja: '更新はありません',
  },
  // 有可更新的 MOD
  modUpdatesAvailable: {
    'zh-CN': '有可更新的 MOD',
    'zh-TW': '有可更新的 MOD',
    en: 'MOD updates available',
    ko: '업데이트 가능한 MOD 있음',
    ja: '更新可能なMODがあります',
  },
  // 去更新
  review: {
    'zh-CN': '去更新',
    'zh-TW': '去更新',
    en: 'Review',
    ko: '업데이트하기',
    ja: '更新へ',
  },
  // 去管理
  manage: {
    'zh-CN': '去管理',
    'zh-TW': '去管理',
    en: 'Manage',
    ko: '관리하기',
    ja: '管理へ',
  },
  // 去安装
  browse: {
    'zh-CN': '去安装',
    'zh-TW': '去安裝',
    en: 'Browse',
    ko: '설치하기',
    ja: 'インストールへ',
  },
  // 在线 MOD
  onlineMods: {
    'zh-CN': '在线 MOD',
    'zh-TW': '線上 MOD',
    en: 'Online MODs',
    ko: '온라인 MOD',
    ja: 'オンラインMOD',
  },
  // 本地 MOD
  localMods: {
    'zh-CN': '本地 MOD',
    'zh-TW': '本機 MOD',
    en: 'Local MODs',
    ko: '로컬 MOD',
    ja: 'ローカルMOD',
  },
  // 在线地图
  onlineMaps: {
    'zh-CN': '在线地图',
    'zh-TW': '線上地圖',
    en: 'Online maps',
    ko: '온라인 지도',
    ja: 'オンライン地図',
  },
  // 本地地图
  localMaps: {
    'zh-CN': '本地地图',
    'zh-TW': '本機地圖',
    en: 'Local maps',
    ko: '로컬 지도',
    ja: 'ローカル地図',
  },
  // ===== 列小标题：总览左右两条链路 =====
  // MOD 链路（框架 → 在线 → 本地）
  modsChain: {
    'zh-CN': 'MOD 链路',
    'zh-TW': 'MOD 鏈路',
    en: 'MOD chain',
    ko: 'MOD 체인',
    ja: 'MOD チェーン',
  },
  // 地图链路（框架 → 在线 → 本地）
  mapsChain: {
    'zh-CN': '地图链路',
    'zh-TW': '地圖鏈路',
    en: 'Map chain',
    ko: '지도 체인',
    ja: 'マップチェーン',
  },
  // 去刷新
  refresh: {
    'zh-CN': '去刷新',
    'zh-TW': '去重新整理',
    en: 'Refresh',
    ko: '새로 고침',
    ja: '更新へ',
  },
  // 共 {count} 个仓库 · {failed} 个刷新失败
  repositoriesFailedToRefresh: {
    'zh-CN': '共 {count} 个仓库 · {failed} 个刷新失败',
    'zh-TW': '共 {count} 個倉庫 · {failed} 個重新整理失敗',
    en: '{count} repositories · {failed} failed to refresh',
    ko: '저장소 {count}개 · {failed}개 새로 고침 실패',
    ja: 'リポジトリ {count} 件 · {failed} 件が更新失敗',
  },
  // 共 {count} 个仓库 · 全部正常
  repositoriesAllHealthy: {
    'zh-CN': '共 {count} 个仓库 · 全部正常',
    'zh-TW': '共 {count} 個倉庫 · 全部正常',
    en: '{count} repositories · all healthy',
    ko: '저장소 {count}개 · 모두 정상',
    ja: 'リポジトリ {count} 件 · すべて正常',
  },
  // 尚未加载任何仓库
  noRepositoriesLoadedYet: {
    'zh-CN': '尚未加载任何仓库',
    'zh-TW': '尚未載入任何倉庫',
    en: 'No repositories loaded yet',
    ko: '아직 저장소가 없습니다',
    ja: 'リポジトリがまだありません',
  },
  // 共 {count} 个，其中 {updates} 个可更新
  installedCanUpdate: {
    'zh-CN': '共 {count} 个，其中 {updates} 个可更新',
    'zh-TW': '共 {count} 個，其中 {updates} 個可更新',
    en: '{count} installed, {updates} can update',
    ko: '{count}개 중 {updates}개 업데이트 가능',
    ja: '{count} 件中 {updates} 件が更新可能',
  },
  // 共 {count} 个，全部为最新版
  installedAllUpToDate: {
    'zh-CN': '共 {count} 个，全部为最新版',
    'zh-TW': '共 {count} 個，全部為最新版',
    en: '{count} installed, all up to date',
    ko: '{count}개 모두 최신 버전',
    ja: '{count} 件すべて最新版',
  },
  // 共 {count} 张地图 · 全部正常
  mapsAllHealthy: {
    'zh-CN': '共 {count} 张地图 · 全部正常',
    'zh-TW': '共 {count} 張地圖 · 全部正常',
    en: '{count} maps · all healthy',
    ko: '지도 {count}개 · 모두 정상',
    ja: '地図 {count} 件 · すべて正常',
  },
  // 共 {count} 张地图 · {failed} 张刷新失败
  mapsFailedToRefresh: {
    'zh-CN': '共 {count} 张地图 · {failed} 张刷新失败',
    'zh-TW': '共 {count} 張地圖 · {failed} 張重新整理失敗',
    en: '{count} maps · {failed} failed to refresh',
    ko: '지도 {count}개 · {failed}개 새로 고침 실패',
    ja: '地図 {count} 件 · {failed} 件が更新失敗',
  },
  // 尚未加载任何地图
  noMapsLoadedYet: {
    'zh-CN': '尚未加载任何地图',
    'zh-TW': '尚未載入任何地圖',
    en: 'No maps loaded yet',
    ko: '아직 지도가 없습니다',
    ja: '地図がまだありません',
  },
  // 共 {count} 张地图，其中 {updates} 张可更新
  installedMapsCanUpdate: {
    'zh-CN': '共 {count} 张地图，其中 {updates} 张可更新',
    'zh-TW': '共 {count} 張地圖，其中 {updates} 張可更新',
    en: '{count} maps installed, {updates} can update',
    ko: '지도 {count}개 중 {updates}개 업데이트 가능',
    ja: '地図 {count} 件中 {updates} 件が更新可能',
  },
  // 共 {count} 张地图
  installedMaps: {
    'zh-CN': '共 {count} 张地图',
    'zh-TW': '共 {count} 張地圖',
    en: '{count} maps installed',
    ko: '지도 {count}개 설치됨',
    ja: '地図 {count} 件インストール済み',
  },
  // 还没有安装任何地图
  noMapsInstalled: {
    'zh-CN': '还没有安装任何地图',
    'zh-TW': '還沒有安裝任何地圖',
    en: 'No maps installed yet',
    ko: '아직 설치된 지도가 없습니다',
    ja: 'インストール済みの地図はありません',
  },
  // 全部 MOD
  allMods: {
    'zh-CN': '全部 MOD',
    'zh-TW': '全部 MOD',
    en: 'All MODs',
    ko: '모든 MOD',
    ja: 'すべてのMOD',
  },
  // 已启用 {enabled} / {total}
  ofEnabled: {
    'zh-CN': '已启用 {enabled} / {total}',
    'zh-TW': '已啟用 {enabled} / {total}',
    en: '{enabled} of {total} enabled',
    ko: '{total}개 중 {enabled}개 활성',
    ja: '{total} 件中 {enabled} 件が有効',
  },
  // 有 MOD 未启用
  someModsAreDisabled: {
    'zh-CN': '有 MOD 未启用',
    'zh-TW': '有 MOD 未啟用',
    en: 'Some MODs are disabled',
    ko: '비활성 MOD가 있습니다',
    ja: '無効な MOD があります',
  },
  // 全部启用
  enableAll: {
    'zh-CN': '全部启用',
    'zh-TW': '全部啟用',
    en: 'Enable all',
    ko: '모두 활성화',
    ja: 'すべて有効化',
  },
  // 全部禁用
  disableAll: {
    'zh-CN': '全部禁用',
    'zh-TW': '全部停用',
    en: 'Disable all',
    ko: '모두 비활성화',
    ja: 'すべて無効化',
  },
  // 禁用全部 MOD
  disableAllMods: {
    'zh-CN': '禁用全部 MOD',
    'zh-TW': '停用全部 MOD',
    en: 'Disable all MODs',
    ko: '모든 MOD 비활성화',
    ja: 'すべてのMODを無効化',
  },
  // 将禁用全部 {count} 个 MOD，游戏将不加载任何 MOD。可以随时重新启用。
  allModsWillBeDisabledTheGameWillLoadNoneOfThemYouCanReEnableThemAnytime: {
    'zh-CN': '将禁用全部 {count} 个 MOD，游戏将不加载任何 MOD。可以随时重新启用。',
    'zh-TW': '將停用全部 {count} 個 MOD，遊戲將不載入任何 MOD。可以隨時重新啟用。',
    en: 'All {count} MODs will be disabled; the game will load none of them. You can re-enable them anytime.',
    ko: 'MOD {count}개를 모두 비활성화합니다. 게임이 어떤 MOD도 불러오지 않습니다. 언제든 다시 활성화할 수 있습니다.',
    ja: 'MOD {count} 件をすべて無効化します。ゲームはどの MOD も読み込みません。いつでも再度有効化できます。',
  },
  // 安装后才能加载 MOD
  installItToLoadMods: {
    'zh-CN': '安装后才能加载 MOD',
    'zh-TW': '安裝後才能載入 MOD',
    en: 'Install it to load MODs',
    ko: '설치해야 MOD를 불러올 수 있습니다',
    ja: 'インストールしないとMODを読み込めません',
  },
  // 检测更新
  check: {
    'zh-CN': '检测更新',
    'zh-TW': '檢查更新',
    en: 'Check',
    ko: '확인',
    ja: '確認',
  },
  // 只刷新已安装 MOD 绑定的仓库，检查是否有新版本
  refreshOnlyTheRepositoriesYourInstalledModsAreBoundToAndCheckForNewVersions: {
    'zh-CN': '只刷新已安装 MOD 绑定的仓库，检查是否有新版本',
    'zh-TW': '只重新整理已安裝 MOD 綁定的倉庫，檢查是否有新版本',
    en: 'Refresh only the repositories your installed MODs are bound to, and check for new versions',
    ko: '설치된 MOD가 연결된 저장소만 새로 고침해 새 버전이 있는지 확인합니다',
    ja: 'インストール済み MOD が紐づくリポジトリだけを更新し、新しいバージョンを確認します',
  },
  // 没有已安装 MOD 绑定到可用的在线仓库
  noInstalledModIsBoundToAnAvailableOnlineRepository: {
    'zh-CN': '没有已安装 MOD 绑定到可用的在线仓库',
    'zh-TW': '沒有已安裝 MOD 綁定到可用的線上倉庫',
    en: 'No installed MOD is bound to an available online repository',
    ko: '사용 가능한 온라인 저장소에 연결된 설치된 MOD가 없습니다',
    ja: '利用可能なオンラインリポジトリに紐づくインストール済み MOD はありません',
  },
  // 通用设置
  general: {
    'zh-CN': '通用设置',
    'zh-TW': '一般設定',
    en: 'General',
    ko: '일반 설정',
    ja: '一般設定',
  },
  // 重装
  reinstall: {
    'zh-CN': '重装',
    'zh-TW': '重新安裝',
    en: 'Reinstall',
    ko: '재설치',
    ja: '再インストール',
  },
  // 启动游戏
  launchGame: {
    'zh-CN': '启动游戏',
    'zh-TW': '啟動遊戲',
    en: 'Launch game',
    ko: '게임 실행',
    ja: 'ゲームを起動',
  },
  // 停止游戏
  stopGame: {
    'zh-CN': '停止游戏',
    'zh-TW': '停止遊戲',
    en: 'Stop game',
    ko: '게임 중지',
    ja: 'ゲームを停止',
  },
  // 启动时检查程序更新
  checkForUpdatesOnStartup: {
    'zh-CN': '启动时检查程序更新',
    'zh-TW': '啟動時檢查程式更新',
    en: 'Check for updates on startup',
    ko: '시작 시 업데이트 확인',
    ja: '起動時に更新を確認',
  },
  // 启动管理器时检查新版本，可在设置中关闭
  checkForANewVersionWhenTheManagerStartsCanBeTurnedOffInSettings: {
    'zh-CN': '启动管理器时检查新版本，可在设置中关闭',
    'zh-TW': '啟動管理器時檢查新版本，可在設定中關閉',
    en: 'Check for a new version when the manager starts; can be turned off in settings',
    ko: '관리자 시작 시 새 버전을 확인합니다. 설정에서 끌 수 있습니다',
    ja: 'マネージャー起動時に新バージョンを確認します（設定で無効化できます）',
  },
  // 自动检测游戏路径
  autoDetectGamePath: {
    'zh-CN': '自动检测游戏路径',
    'zh-TW': '自動偵測遊戲路徑',
    en: 'Auto-detect game path',
    ko: '게임 경로 자동 감지',
    ja: 'ゲームパスを自動検出',
  },
  // 启动与刷新时自动探测 Steam 库
  probeSteamLibrariesOnLaunchAndRefresh: {
    'zh-CN': '启动与刷新时自动探测 Steam 库',
    'zh-TW': '啟動與重新整理時自動探測 Steam 庫',
    en: 'Probe Steam libraries on launch and refresh',
    ko: '시작·새로 고침 시 Steam 라이브러리를 탐색합니다',
    ja: '起動時と更新時にSteamライブラリを検出します',
  },
  // 关于
  about: {
    'zh-CN': '关于',
    'zh-TW': '關於',
    en: 'About',
    ko: '정보',
    ja: 'このアプリについて',
  },
  // 当前路径
  currentPath: {
    'zh-CN': '当前路径',
    'zh-TW': '目前路徑',
    en: 'Current path',
    ko: '현재 경로',
    ja: '現在のパス',
  },
  // 选择或输入目录路径
  chooseOrEnterAFolderPath: {
    'zh-CN': '选择或输入目录路径',
    'zh-TW': '選擇或輸入目錄路徑',
    en: 'Choose or enter a folder path',
    ko: '폴더 경로를 선택하거나 입력하세요',
    ja: 'フォルダーパスを選択または入力',
  },
  // 浏览…
  browse2: {
    'zh-CN': '浏览…',
    'zh-TW': '瀏覽…',
    en: 'Browse…',
    ko: '찾아보기…',
    ja: '参照…',
  },
  // 确认保存
  save: {
    'zh-CN': '确认保存',
    'zh-TW': '確認儲存',
    en: 'Save',
    ko: '저장',
    ja: '保存',
  },
  // 选择游戏目录
  chooseGameDirectory: {
    'zh-CN': '选择游戏目录',
    'zh-TW': '選擇遊戲目錄',
    en: 'Choose game directory',
    ko: '게임 폴더 선택',
    ja: 'ゲームフォルダーを選択',
  },
  // 目录需包含 Overcooked!2.exe；保存后自动验证。
  theFolderMustContainOvercooked2ExeItIsVerifiedOnSave: {
    'zh-CN': '目录需包含 Overcooked!2.exe；保存后自动验证。',
    'zh-TW': '目錄需包含 Overcooked!2.exe；儲存後自動驗證。',
    en: 'The folder must contain Overcooked!2.exe; it is verified on save.',
    ko: '폴더에 Overcooked!2.exe가 있어야 하며 저장 후 검증합니다.',
    ja: 'Overcooked!2.exe を含む必要があります。保存時に検証します。',
  },
}
