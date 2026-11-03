import type { DomainMessages } from './types'

/**
 * DIYLevel 关卡集（玩家口中的「地图」）的后端 tag。
 *
 * 与 `mods` 域的分工：这里是**关卡集目录**层面的操作（扫描、移动、ZIP 导入），
 * 不涉及 BepInEx 插件的加载与绑定。两者在界面上是两张不同的页面，所以文案
 * 也分开维护 —— 共用措辞走 `common`。
 *
 * 命名沿用后端 `levelSet.*`：第一段是域，其后是失败动作。
 */
export const levelSet: DomainMessages = {
  infoMissing: {
    'zh-CN': '关卡集缺少 info 文件，无法读取名称与版本',
    'zh-TW': '關卡集缺少 info 檔案，無法讀取名稱與版本',
    en: 'The level set has no info file, so its name and version cannot be read',
    ko: '레벨 세트에 info 파일이 없어 이름과 버전을 읽을 수 없습니다',
    ja: 'レベルセットに info ファイルがないため、名前とバージョンを読み取れません',
  },
  invalidName: {
    'zh-CN': '关卡集名称无效，无法作为目录名',
    'zh-TW': '關卡集名稱無效，無法作為目錄名',
    en: 'The level set name cannot be used as a directory name',
    ko: '레벨 세트 이름을 디렉터리 이름으로 사용할 수 없습니다',
    ja: 'レベルセット名をディレクトリ名として使用できません',
  },
  notFound: {
    'zh-CN': '关卡集不在预期位置，请刷新后重试',
    'zh-TW': '關卡集不在預期位置，請重新整理後重試',
    en: 'The level set is not where it was expected. Refresh and try again.',
    ko: '레벨 세트가 예상 위치에 없습니다. 새로 고친 후 다시 시도하세요.',
    ja: 'レベルセットが想定の場所にありません。更新してから再試行してください。',
  },
  targetExists: {
    'zh-CN': '已存在同名关卡集「{name}」，请先处理它',
    'zh-TW': '已存在同名關卡集「{name}」，請先處理它',
    en: 'A level set named "{name}" already exists. Resolve it first.',
    ko: '같은 이름의 레벨 세트 "{name}"이(가) 이미 있습니다. 먼저 처리하세요.',
    ja: '同じ名前のレベルセット「{name}」が既にあります。先に処理してください。',
  },
  moveFailed: {
    'zh-CN': '移动关卡集失败：{reason}',
    'zh-TW': '移動關卡集失敗：{reason}',
    en: 'Failed to move the level set: {reason}',
    ko: '레벨 세트 이동 실패: {reason}',
    ja: 'レベルセットの移動に失敗しました: {reason}',
  },
  writeFailed: {
    'zh-CN': '写入关卡集文件失败：{reason}',
    'zh-TW': '寫入關卡集檔案失敗：{reason}',
    en: 'Failed to write the level set files: {reason}',
    ko: '레벨 세트 파일 쓰기 실패: {reason}',
    ja: 'レベルセットのファイル書き込みに失敗しました: {reason}',
  },
  nothingSelected: {
    'zh-CN': '没有选择任何关卡集',
    'zh-TW': '沒有選擇任何關卡集',
    en: 'No level set was selected',
    ko: '선택한 레벨 세트가 없습니다',
    ja: 'レベルセットが選択されていません',
  },
  gameRunning: {
    'zh-CN': '游戏正在运行，暂时无法启用或停用地图，请先关闭游戏',
    'zh-TW': '遊戲正在執行，暫時無法啟用或停用地圖，請先關閉遊戲',
    en: 'The game is running, so maps cannot be enabled or disabled right now. Close the game first.',
    ko: '게임이 실행 중이라 지도를 활성화/비활성화할 수 없습니다. 먼저 게임을 종료하세요.',
    ja: 'ゲームが実行中のため、マップの有効化・無効化はできません。先にゲームを終了してください。',
  },
  zipWithoutInfo: {
    'zh-CN': '这个 ZIP 里没有找到任何关卡集（缺少 info 文件）',
    'zh-TW': '這個 ZIP 裡沒有找到任何關卡集（缺少 info 檔案）',
    en: 'No level set was found in this ZIP (no info file)',
    ko: '이 ZIP에서 레벨 세트를 찾지 못했습니다(info 파일 없음)',
    ja: 'この ZIP にレベルセットが見つかりません（info ファイルがありません）',
  },
  zipDuplicate: {
    'zh-CN': 'ZIP 里有两个关卡集都叫「{name}」，无法区分',
    'zh-TW': 'ZIP 裡有兩個關卡集都叫「{name}」，無法區分',
    en: 'The ZIP contains two level sets named "{name}", so they cannot be told apart',
    ko: 'ZIP에 이름이 "{name}"인 레벨 세트가 둘 있어 구분할 수 없습니다',
    ja: 'ZIP 内に「{name}」という名前のレベルセットが 2 つあり区別できません',
  },
  zipInfoInvalid: {
    'zh-CN': 'ZIP 里的关卡集信息无法读取：{reason}',
    'zh-TW': 'ZIP 裡的關卡集資訊無法讀取：{reason}',
    en: 'The level set info inside the ZIP cannot be read: {reason}',
    ko: 'ZIP 안의 레벨 세트 정보를 읽을 수 없습니다: {reason}',
    ja: 'ZIP 内のレベルセット情報を読み取れません: {reason}',
  },
  zipEntryMissing: {
    'zh-CN': 'ZIP 里缺少文件「{entry}」，可能已被修改',
    'zh-TW': 'ZIP 裡缺少檔案「{entry}」，可能已被修改',
    en: 'The ZIP is missing "{entry}"; it may have been modified',
    ko: 'ZIP에 "{entry}" 파일이 없습니다. 파일이 변경되었을 수 있습니다.',
    ja: 'ZIP に「{entry}」がありません。変更された可能性があります。',
  },
  zipMismatch: {
    'zh-CN': '这个 ZIP 的关卡集与来源声明不一致（声明 {expected}，实际 {actual}）',
    'zh-TW': '這個 ZIP 的關卡集與來源宣告不一致（宣告 {expected}，實際 {actual}）',
    en: 'The level set in this ZIP does not match the source declaration (declared {expected}, found {actual})',
    ko: '이 ZIP의 레벨 세트가 소스 선언과 일치하지 않습니다(선언 {expected}, 실제 {actual})',
    ja: 'この ZIP のレベルセットがソースの宣言と一致しません（宣言 {expected}、実際 {actual}）',
  },
  zipMultipleForRepository: {
    'zh-CN': '这个 ZIP 里有 {count} 个关卡集，一个地图仓库只能对应一个，请改用本地导入逐个安装',
    'zh-TW': '這個 ZIP 裡有 {count} 個關卡集，一個地圖倉庫只能對應一個，請改用本機匯入逐一安裝',
    en: 'This ZIP contains {count} level sets, but one map repository maps to exactly one. Use local import to install them individually.',
    ko: '이 ZIP에는 레벨 세트가 {count}개 있지만 지도 저장소 하나는 하나에만 대응합니다. 로컬 가져오기로 하나씩 설치하세요.',
    ja: 'この ZIP にはレベルセットが {count} 個ありますが、地図リポジトリ 1 つは 1 つにのみ対応します。ローカルインポートで個別にインストールしてください。',
  },
  mapMissing: {
    'zh-CN': '找不到这个地图，可能已从地图源下线，请刷新地图源',
    'zh-TW': '找不到這個地圖，可能已從地圖源下線，請重新整理地圖源',
    en: 'This map could not be found; it may have been removed from its source. Refresh the map sources.',
    ko: '이 지도를 찾을 수 없습니다. 지도 소스에서 제거되었을 수 있으니 지도 소스를 새로 고치세요.',
    ja: 'このマップが見つかりません。地図ソースから削除された可能性があります。地図ソースを更新してください。',
  },
  releaseMissing: {
    'zh-CN': '这个地图的 Release 已不在缓存里，请先刷新',
    'zh-TW': '這個地圖的 Release 已不在快取裡，請先重新整理',
    en: 'That release is no longer in the cache. Refresh first.',
    ko: '해당 Release가 캐시에 없습니다. 먼저 새로 고치세요.',
    ja: 'その Release はキャッシュにありません。先に更新してください。',
  },
  // ===== 地图源文档校验 =====
  duplicateEntry: {
    'zh-CN': '地图源里有两张地图都指向「{identity}」，请检查 level_set_name 与 author',
    'zh-TW': '地圖源裡有兩張地圖都指向「{identity}」，請檢查 level_set_name 與 author',
    en: 'Two maps in this source share the identity "{identity}". Check level_set_name and author.',
    ko: '지도 소스에 동일한 식별자 "{identity}"를 가진 지도가 둘 있습니다. level_set_name과 author를 확인하세요.',
    ja: 'この地図ソースに同じ識別子「{identity}」のマップが 2 つあります。level_set_name と author を確認してください。',
  },
  missingLevelSetName: {
    'zh-CN': '地图「{map}」缺少 level_set_name，无法与本地关卡集绑定',
    'zh-TW': '地圖「{map}」缺少 level_set_name，無法與本機關卡集綁定',
    en: 'Map "{map}" has no level_set_name, so it cannot be bound to a local level set',
    ko: '지도 "{map}"에 level_set_name이 없어 로컬 레벨 세트와 연결할 수 없습니다',
    ja: 'マップ「{map}」に level_set_name がないため、ローカルのレベルセットと紐付けできません',
  },
  missingCoordinates: {
    'zh-CN': '地图「{map}」缺少项目用户或仓库名',
    'zh-TW': '地圖「{map}」缺少專案使用者或倉庫名稱',
    en: 'Map "{map}" is missing the owner or repository name',
    ko: '지도 "{map}"에 프로젝트 사용자 또는 저장소 이름이 없습니다',
    ja: 'マップ「{map}」にプロジェクトユーザーまたはリポジトリ名がありません',
  },
  missingAssetRules: {
    'zh-CN': '地图「{map}」没有资产规则，无法判断该下载哪个文件',
    'zh-TW': '地圖「{map}」沒有資產規則，無法判斷該下載哪個檔案',
    en: 'Map "{map}" has no asset rules, so the file to download cannot be determined',
    ko: '지도 "{map}"에 자산 규칙이 없어 어떤 파일을 받을지 결정할 수 없습니다',
    ja: 'マップ「{map}」にアセットルールがないため、どのファイルを取得するか判断できません',
  },
  // ===== 界面文案 =====
  title: {
    'zh-CN': '地图管理',
    'zh-TW': '地圖管理',
    en: 'Maps',
    ko: '지도 관리',
    ja: 'マップ管理',
  },
  noLevelSets: {
    'zh-CN': '还没有安装任何地图。到「地图安装」页安装线上地图，或在那里用「安装地图」导入本地 ZIP。',
    'zh-TW': '還沒有安裝任何地圖。到「地圖安裝」頁安裝線上地圖，或在那裡用「安裝地圖」匯入本機 ZIP。',
    en: 'No maps installed yet. Install maps from the Map Install page, or use "Install map" there to import a local ZIP.',
    ko: '설치된 지도가 없습니다. 지도 설치 페이지에서 지도를 설치하거나, 거기서 "지도 설치"로 로컬 ZIP을 가져오세요.',
    ja: 'マップがまだありません。「マップインストール」でマップをインストールするか、そこの「マップをインストール」でローカル ZIP をインポートしてください。',
  },
  // 读取进度：已读取 3/7（转圈条下方）
  scanningProgress: {
    'zh-CN': '已读取 {done}/{total}',
    'zh-TW': '已讀取 {done}/{total}',
    en: 'Read {done}/{total}',
    ko: '{done}/{total} 읽음',
    ja: '読み込み済み {done}/{total}',
  },
  // 正在读取：地图名（转圈条下方）
  // 正在读取地图源（整页加载遮罩文案）
  readingMapSources: {
    'zh-CN': '正在读取地图源…',
    'zh-TW': '正在讀取地圖來源…',
    en: 'Reading map sources…',
    ko: '지도 소스를 읽는 중…',
    ja: 'マップソースを読み込み中…',
  },
  /*
   * 地图没有说明时的兜底文案。
   *
   * 以前这里显示仓库名，一个仓库多张地图时每张卡都长一个样；直接说「暂无
   * 说明」至少不会误导用户以为那就是这张图的介绍。
   */
  mapNoNote: {
    'zh-CN': '此地图暂无说明',
    'zh-TW': '此地圖暫無說明',
    en: 'No description for this map yet',
    ko: '이 지도에는 아직 설명이 없습니다',
    ja: 'このマップにはまだ説明がありません',
  },
  scanningFolder: {
    'zh-CN': '正在读取：{name}',
    'zh-TW': '正在讀取：{name}',
    en: 'Reading: {name}',
    ko: '읽는 중: {name}',
    ja: '読み込み中: {name}',
  },
  // 正在统计地图文件夹（还没拿到总数时）
  scanningPreparing: {
    'zh-CN': '正在统计地图文件夹…',
    'zh-TW': '正在統計地圖資料夾…',
    en: 'Counting map folders…',
    ko: '지도 폴더를 세는 중…',
    ja: 'マップフォルダを数えています…',
  },
  // 识别来源 ZIP：正在读哪个 info、读到第几份（转圈条下方）
  zipReading: {
    'zh-CN': '正在读取 {name}（{done}/{total}）',
    'zh-TW': '正在讀取 {name}（{done}/{total}）',
    en: 'Reading {name} ({done}/{total})',
    ko: '{name} 읽는 중 ({done}/{total})',
    ja: '{name} を読み込み中（{done}/{total}）',
  },
  // 导入来源 ZIP：正在写第几张（确认后写盘阶段）
  zipInstalling: {
    'zh-CN': '正在写入 {name}（{done}/{total}）',
    'zh-TW': '正在寫入 {name}（{done}/{total}）',
    en: 'Writing {name} ({done}/{total})',
    ko: '{name} 쓰는 중 ({done}/{total})',
    ja: '{name} を書き込み中（{done}/{total}）',
  },
  imported: {
    'zh-CN': '已导入 {count} 个地图',
    'zh-TW': '已匯入 {count} 個地圖',
    en: 'Imported {count} map(s)',
    ko: '지도 {count}개를 가져왔습니다',
    ja: 'マップを {count} 個インポートしました',
  },
  levelCount: {
    'zh-CN': '{count} 个关卡',
    'zh-TW': '{count} 個關卡',
    en: '{count} levels',
    ko: '레벨 {count}개',
    ja: '{count} レベル',
  },
  toggleHint: {
    'zh-CN': '启用时地图放在 levels，停用时整个文件夹（含随包插件）移到 cache/modmanager_disabled_maps（游戏运行中暂不支持切换）',
    'zh-TW': '啟用時地圖放在 levels，停用時整個資料夾（含隨包外掛）移到 cache/modmanager_disabled_maps（遊戲執行中暫不支援切換）',
    en: 'Enabled maps live in levels; disabling moves the whole folder (bundled plugins included) to cache/modmanager_disabled_maps (not available while the game is running)',
    ko: '활성화하면 levels, 비활성화하면 폴더 전체(함께 포함된 플러그인 포함)가 cache/modmanager_disabled_maps로 이동합니다(게임 실행 중에는 지원되지 않음)',
    ja: '有効時は levels、無効時はフォルダ全体（同梱プラグインを含む）を cache/modmanager_disabled_maps へ移動します（ゲーム実行中は切り替えできません）',
  },
  rescanLocal: {
    'zh-CN': '重新扫描本地地图',
    'zh-TW': '重新掃描本機地圖',
    en: 'Rescan local maps',
    ko: '로컬 지도 다시 검사',
    ja: 'ローカルマップを再スキャン',
  },
  checkUpdatesHint: {
    'zh-CN': '只刷新本地已装地图绑定的仓库，检查有没有新版本',
    'zh-TW': '只重新整理本機已裝地圖綁定的倉庫，檢查有沒有新版本',
    en: 'Refresh only the repositories your installed maps are bound to and check for new versions',
    ko: '설치된 지도가 연결된 저장소만 새로 고쳐 새 버전이 있는지 확인합니다',
    ja: 'インストール済みマップが紐づくリポジトリだけを更新し、新しいバージョンを確認します',
  },
  noBoundMaps: {
    'zh-CN': '本地地图都没有绑定地图源，无法检测更新',
    'zh-TW': '本機地圖都沒有綁定地圖源，無法檢查更新',
    en: 'None of your local maps is bound to a map source, so updates cannot be checked',
    ko: '로컬 지도가 지도 소스에 연결되어 있지 않아 업데이트를 확인할 수 없습니다',
    ja: 'ローカルマップがマップソースに紐づいていないため、更新を確認できません',
  },
  disableAllTitle: {
    'zh-CN': '禁用全部地图',
    'zh-TW': '停用全部地圖',
    en: 'Disable all maps',
    ko: '모든 지도 비활성화',
    ja: 'すべてのマップを無効化',
  },
  disableAllConfirm: {
    'zh-CN': '将禁用全部 {count} 张地图，游戏内不再加载它们；可以随时重新启用。',
    'zh-TW': '將停用全部 {count} 張地圖，遊戲內不再載入它們；可以隨時重新啟用。',
    en: 'All {count} map(s) will be disabled and will no longer load in game; you can re-enable them anytime.',
    ko: '지도 {count}개를 모두 비활성화하면 게임에서 더 이상 로드되지 않습니다. 언제든 다시 활성화할 수 있습니다.',
    ja: 'マップ {count} 個をすべて無効化するとゲーム内で読み込まれなくなります。いつでも再度有効化できます。',
  },
  // ===== 地图安装页 =====
  // 地图安装（导航与页标题）：与 MOD 安装页同一套命名，落地动作是安装而不是下载
  download: {
    'zh-CN': '地图安装',
    'zh-TW': '地圖安裝',
    en: 'Map Install',
    ko: '지도 설치',
    ja: 'マップインストール',
  },
  mapCount: {
    'zh-CN': '张地图',
    'zh-TW': '張地圖',
    en: 'maps',
    ko: '개 지도',
    ja: '件のマップ',
  },
  noMapSources: {
    'zh-CN': '还没有可用的地图源。可以在设置里检查地图源，或用右上角「安装地图」导入本地 ZIP。',
    'zh-TW': '還沒有可用的地圖源。可以在設定裡檢查地圖源，或用右上角「安裝地圖」匯入本機 ZIP。',
    en: 'No map sources are available yet. Check the map sources in Settings, or use "Install map" above to import a local ZIP.',
    ko: '사용 가능한 지도 소스가 없습니다. 설정에서 지도 소스를 확인하거나 위의 "지도 설치"로 로컬 ZIP을 가져오세요.',
    ja: '利用できるマップソースがありません。設定でマップソースを確認するか、右上の「マップをインストール」でローカル ZIP をインポートしてください。',
  },
  noMatchMaps: {
    'zh-CN': '没有匹配的地图，换个关键词试试。',
    'zh-TW': '沒有符合的地圖，換個關鍵字試試。',
    en: 'No maps match your search. Try a different keyword.',
    ko: '검색과 일치하는 지도가 없습니다. 다른 키워드를 입력해 보세요.',
    ja: '検索に一致するマップがありません。別のキーワードをお試しください。',
  },
  uncategorizedMapSource: {
    'zh-CN': '未归类地图源',
    'zh-TW': '未分類地圖源',
    en: 'Uncategorized map source',
    ko: '분류되지 않은 지도 소스',
    ja: '未分類のマップソース',
  },
  refreshAllHint: {
    'zh-CN': '刷新全部地图的 Release（忽略 5 分钟缓存）',
    'zh-TW': '重新整理全部地圖的 Release（忽略 5 分鐘快取）',
    en: 'Refresh releases for every map, ignoring the 5-minute cache',
    ko: '모든 지도의 Release를 새로 고칩니다(5분 캐시 무시)',
    ja: 'すべてのマップの Release を更新します（5 分キャッシュを無視）',
  },
  installMap: {
    'zh-CN': '安装地图',
    'zh-TW': '安裝地圖',
    en: 'Install map',
    ko: '지도 설치',
    ja: 'マップをインストール',
  },
  installDone: {
    'zh-CN': '已安装地图「{name}」，到「地图管理」可以启用或停用',
    'zh-TW': '已安裝地圖「{name}」，到「地圖管理」可以啟用或停用',
    en: 'Installed "{name}". Enable or disable it from Maps.',
    ko: '지도 "{name}"을(를) 설치했습니다. 지도 관리에서 활성화하거나 비활성화하세요.',
    ja: 'マップ「{name}」をインストールしました。マップ管理で有効/無効を切り替えられます。',
  },
  installPath: {
    'zh-CN': '安装到',
    'zh-TW': '安裝到',
    en: 'Installed to',
    ko: '설치 위치',
    ja: 'インストール先',
  },
  organize: {
    'zh-CN': '整理地图目录',
    'zh-TW': '整理地圖目錄',
    en: 'Organize map folders',
    ko: '지도 폴더 정리',
    ja: 'マップフォルダを整理',
  },
  organizeHint: {
    'zh-CN': '把 info 埋在子文件夹里、游戏读不到的目录搬成规范结构',
    'zh-TW': '把 info 埋在子資料夾裡、遊戲讀不到的目錄搬成規範結構',
    en: 'Move folders whose info file is buried in a subfolder (unreadable in game) into the standard layout',
    ko: 'info 파일이 하위 폴더에 묻혀 게임에서 읽히지 않는 폴더를 표준 구조로 옮깁니다',
    ja: 'info ファイルがサブフォルダに埋もれてゲームから読めないフォルダを標準構成に移します',
  },
  organizeNone: {
    'zh-CN': '没有需要整理的地图目录',
    'zh-TW': '沒有需要整理的地圖目錄',
    en: 'No map folders need organizing',
    ko: '정리할 지도 폴더가 없습니다',
    ja: '整理が必要なマップフォルダはありません',
  },
  organizeConfirm: {
    'zh-CN': '检测到 {count} 个目录的 info 埋在子文件夹里，游戏读不到。整理会把里面的关卡文件搬进规范目录，清空的子文件夹移入回收站。是否继续？',
    'zh-TW': '偵測到 {count} 個目錄的 info 埋在子資料夾裡，遊戲讀不到。整理會把裡面的關卡檔案搬進規範目錄，清空的子資料夾移入回收站。是否繼續？',
    en: '{count} folder(s) keep their info file in a subfolder, so the game cannot read them. Organizing moves the level files into the standard layout and sends the emptied subfolders to the recycle bin. Continue?',
    ko: '{count}개 폴더의 info 파일이 하위 폴더에 묻혀 있어 게임에서 읽을 수 없습니다. 정리하면 안의 레벨 파일을 표준 폴더로 옮기고 비워진 하위 폴더는 휴지통으로 보냅니다. 계속할까요?',
    ja: '{count} 個のフォルダで info ファイルがサブフォルダに埋もれており、ゲームから読めません。整理すると中のレベルファイルを標準フォルダへ移し、空になったサブフォルダはゴミ箱へ移動します。続行しますか？',
  },
  organizeDone: {
    'zh-CN': '已整理 {count} 个目录',
    'zh-TW': '已整理 {count} 個目錄',
    en: 'Organized {count} folder(s)',
    ko: '{count}개 폴더를 정리했습니다',
    ja: '{count} 個のフォルダを整理しました',
  },
  organizeFailed: {
    'zh-CN': '{count} 个目录整理失败：{detail}',
    'zh-TW': '{count} 個目錄整理失敗：{detail}',
    en: 'Failed to organize {count} folder(s): {detail}',
    ko: '{count}개 폴더 정리에 실패했습니다: {detail}',
    ja: '{count} 個のフォルダを整理できませんでした: {detail}',
  },
  installedVersion: {
    'zh-CN': '已装 {version}',
    'zh-TW': '已裝 {version}',
    en: 'installed {version}',
    ko: '설치됨 {version}',
    ja: 'インストール済み {version}',
  },
  frameworksNotMet: {
    'zh-CN': '这些前置还不满足，装上后地图仍无法加载：{list}',
    'zh-TW': '這些前置還不滿足，裝上後地圖仍無法載入：{list}',
    en: 'These requirements are not met yet, so the map will not load: {list}',
    ko: '다음 요구 사항이 충족되지 않아 지도가 로드되지 않습니다: {list}',
    ja: '次の前提が満たされていないため、マップは読み込まれません: {list}',
  },
  // 卡片上的「缺前置」标签（与 MOD 卡片的「缺依赖」对应）
  missingFramework: {
    'zh-CN': '缺前置',
    'zh-TW': '缺前置',
    en: 'Missing requirement',
    ko: '필수 구성 요소 없음',
    ja: '前提不足',
  },
  /*
   * 地图卡片的「已拿到版本」状态。
   *
   * 刻意不说「最新版 / 可更新」：本地关卡集的版本要解析 info 才知道，
   * 那是管理页的职责。这里只知道「线上有这些版本」。
   */
  releaseReady: {
    'zh-CN': '线上有版本',
    'zh-TW': '線上版本可用',
    en: 'Available online',
    ko: '온라인 버전 있음',
    ja: 'オンラインに版あり',
  },
  refreshMapSources: {
    'zh-CN': '刷新地图源',
    'zh-TW': '重新整理地圖源',
    en: 'Refresh map sources',
    ko: '지도 소스 새로 고침',
    ja: 'マップソースを更新',
  },
  useBundledMapSourcesOnly: {
    'zh-CN': '仅使用内置地图源',
    'zh-TW': '僅使用內建地圖源',
    en: 'Use bundled map sources only',
    ko: '내장 지도 소스만 사용',
    ja: '内蔵マップソースのみ使用',
  },
  useBundledMapSourcesOnlyHint: {
    'zh-CN': '地图索引改读管理器自带的快照，不访问 Gitee / GitHub 的 raw 入口。适合官方入口连不上或需要完全离线的场景。',
    'zh-TW': '地圖索引改讀管理器內建的快照，不存取 Gitee / GitHub 的 raw 入口。適合官方入口連不上或需要完全離線的情況。',
    en: 'Read the map index from the snapshot shipped with the manager instead of the Gitee/GitHub raw entry. Useful when that entry is unreachable or you need to stay fully offline.',
    ko: '지도 인덱스를 Gitee / GitHub raw 대신 관리자에 내장된 스냅숏에서 읽습니다. 공식 진입점에 연결할 수 없거나 완전히 오프라인일 때 유용합니다.',
    ja: '地図インデックスを Gitee / GitHub の raw ではなく、マネージャー内蔵のスナップショットから読みます。公式エントリに接続できない場合や完全オフラインで使う場合に便利です。',
  },
  // 地图管理页：绑定与版本对照
  unboundHint: {
    'zh-CN': '没有地图源声明这一组关卡集，所以无法检查更新（本地导入或源已下线）',
    'zh-TW': '沒有地圖源宣告這一組關卡集，因此無法檢查更新（本機匯入或源已下線）',
    en: 'No map source declares this level set, so updates cannot be checked (locally imported, or the source is gone)',
    ko: '이 레벨 세트를 선언한 지도 소스가 없어 업데이트를 확인할 수 없습니다(로컬 가져오기 또는 소스 제거됨)',
    ja: 'このレベルセットを宣言したマップソースがないため、更新を確認できません（ローカルインポート、またはソースが削除済み）',
  },
  updateAvailableHint: {
    'zh-CN': '地图源有更新的版本：{version}',
    'zh-TW': '地圖源有更新的版本：{version}',
    en: 'A newer version is available in the map source: {version}',
    ko: '지도 소스에 더 새로운 버전이 있습니다: {version}',
    ja: 'マップソースに新しいバージョンがあります: {version}',
  },
  // ===== 地图管理页：卡片状态 =====
  parseFailed: {
    'zh-CN': '解析失败',
    'zh-TW': '解析失敗',
    en: 'Parse failed',
    ko: '해석 실패',
    ja: '解析失敗',
  },
  higherVersion: {
    'zh-CN': '更高版本',
    'zh-TW': '更高版本',
    en: 'Newer version',
    ko: '더 높은 버전',
    ja: 'より新しいバージョン',
  },
  // ===== 地图管理页：展开详情 =====
  nameZh: {
    'zh-CN': '中文名',
    'zh-TW': '中文名稱',
    en: 'Chinese name',
    ko: '중국어 이름',
    ja: '中国語名',
  },
  nameEn: {
    'zh-CN': '英文名',
    'zh-TW': '英文名稱',
    en: 'English name',
    ko: '영어 이름',
    ja: '英語名',
  },
  authorZh: {
    'zh-CN': '作者中文名',
    'zh-TW': '作者中文名稱',
    en: 'Chinese author name',
    ko: '제작자 중국어 이름',
    ja: '作者の中国語名',
  },
  authorEn: {
    'zh-CN': '作者英文名',
    'zh-TW': '作者英文名稱',
    en: 'English author name',
    ko: '제작자 영어 이름',
    ja: '作者の英語名',
  },
  path: {
    'zh-CN': '路径',
    'zh-TW': '路徑',
    en: 'Path',
    ko: '경로',
    ja: 'パス',
  },
  version: {
    'zh-CN': '版本',
    'zh-TW': '版本',
    en: 'Version',
    ko: '버전',
    ja: 'バージョン',
  },
  levelCountLabel: {
    'zh-CN': '关卡数',
    'zh-TW': '關卡數',
    en: 'Levels',
    ko: '레벨 수',
    ja: 'レベル数',
  },
  // 安装详情：这个地图集在 levels/ 下的文件夹名（导入后就是这个目录）
  installFolder: {
    'zh-CN': '安装目录',
    'zh-TW': '安裝目錄',
    en: 'Install folder',
    ko: '설치 폴더',
    ja: 'インストール先',
  },
  // 安装详情：这个地图集一共有多少个文件（含随包插件）
  fileCountLabel: {
    'zh-CN': '文件数',
    'zh-TW': '檔案數',
    en: 'Files',
    ko: '파일 수',
    ja: 'ファイル数',
  },
  parseError: {
    'zh-CN': '解析错误',
    'zh-TW': '解析錯誤',
    en: 'Parse error',
    ko: '해석 오류',
    ja: '解析エラー',
  },
  statusHint: {
    'zh-CN': '状态说明',
    'zh-TW': '狀態說明',
    en: 'Status',
    ko: '상태 설명',
    ja: '状態の説明',
  },
  // 详情里「线上绑定 / 状态」这一小节的标题，与上面四行元数据分开
  bindStatusSection: {
    'zh-CN': '线上绑定与状态',
    'zh-TW': '線上綁定與狀態',
    en: 'Online binding & status',
    ko: '온라인 바인딩 및 상태',
    ja: 'オンライン紐付けと状態',
  },
  // ===== 地图详情抽屉 =====
  summary: {
    'zh-CN': '简介',
    'zh-TW': '簡介',
    en: 'Summary',
    ko: '소개',
    ja: '概要',
  },
  bindingIdentity: {
    'zh-CN': '绑定身份',
    'zh-TW': '綁定身分',
    en: 'Binding',
    ko: '연결 정보',
    ja: '紐付け',
  },
  sourcePage: {
    'zh-CN': '来源页',
    'zh-TW': '來源頁',
    en: 'Source',
    ko: '소스 페이지',
    ja: 'ソース',
  },
  releasePage: {
    'zh-CN': 'Release 页面',
    'zh-TW': 'Release 頁面',
    en: 'Releases',
    ko: 'Release 페이지',
    ja: 'Release ページ',
  },
  assets: {
    'zh-CN': '资产',
    'zh-TW': '資產',
    en: 'Assets',
    ko: '자산',
    ja: 'アセット',
  },
  // ===== 错误 tag（translateBackendMessage）=====
  deleteFailed: {
    'zh-CN': '删除地图失败：{reason}',
    'zh-TW': '刪除地圖失敗：{reason}',
    en: 'Failed to delete the map: {reason}',
    ko: '지도 삭제 실패: {reason}',
    ja: 'マップの削除に失敗しました: {reason}',
  },
  /*
   * 清理临时地图时目标不在管理器缓存目录里。
   * 正常流程不会走到这里（暂存文件一定是后端自己下的），出现即说明前端传错了路径，
   * 所以文案只说清「能清理的范围」，不诱导用户去重试。
   */
  outsideCache: {
    'zh-CN': '只能清理管理器缓存目录里的临时地图文件',
    'zh-TW': '只能清理管理器快取目錄裡的暫存地圖檔案',
    en: 'Only temporary map files inside the manager cache folder can be cleaned up',
    ko: '관리자 캐시 폴더 안의 임시 지도 파일만 정리할 수 있습니다',
    ja: 'マネージャーのキャッシュフォルダー内の一時マップファイルのみ削除できます',
  },
  releaseFiles: {
    'zh-CN': '该 Release 的文件',
    'zh-TW': '該 Release 的檔案',
    en: 'Files in this release',
    ko: '이 Release의 파일',
    ja: 'この Release のファイル',
  },
  viewInMapManager: {
    'zh-CN': '在「地图管理」中查看',
    'zh-TW': '在「地圖管理」中檢視',
    en: 'View in Maps',
    ko: '지도 관리에서 보기',
    ja: '「マップ管理」で表示',
  },
  // 地图管理页 · 卡片详情里的「绑定的地图」
  boundMap: {
    'zh-CN': '绑定的地图',
    'zh-TW': '綁定的地圖',
    en: 'Bound map',
    ko: '연결된 지도',
    ja: '紐付けマップ',
  },
  deleteTitle: {
    'zh-CN': '删除地图',
    'zh-TW': '刪除地圖',
    en: 'Delete map',
    ko: '지도 삭제',
    ja: 'マップを削除',
  },
  viewInMapDownload: {
    'zh-CN': '在「地图安装」中查看',
    'zh-TW': '在「地圖安裝」中檢視',
    en: 'View in map install',
    ko: '지도 설치에서 보기',
    ja: 'マップインストールで表示',
  },
  // 地图卡片「有依赖版本冲突」时的一键动作：跳到 MOD 管理页处理装错版本的依赖
  resolveConflict: {
    'zh-CN': '处理冲突',
    'zh-TW': '處理衝突',
    en: 'Resolve conflict',
    ko: '충돌 해결',
    ja: '競合を解決',
  },
  // 下载页卡片 · 刷新失败或版本不可比时的结论（对齐 MOD 的「无法检测更新」）
  cannotCheckUpdate: {
    'zh-CN': '无法检测更新',
    'zh-TW': '無法偵測更新',
    en: 'Cannot check for updates',
    ko: '업데이트를 확인할 수 없음',
    ja: '更新を確認できません',
  },
  // 单张随包插件卡片右侧的角色标签：说明「这不是独立 MOD，是这张图自带的」
  bundledPlugin: {
    'zh-CN': '随包插件',
    'zh-TW': '隨包插件',
    en: 'Bundled',
    ko: '동봉',
    ja: '同梱',
  },
  // 插件文件被单独改成 `.dll.disabled` 时的标记（正常情况跟着地图整目录启停）
  pluginDisabled: {
    'zh-CN': '未启用',
    'zh-TW': '未啟用',
    en: 'Disabled',
    ko: '사용 안 함',
    ja: '無効',
  },
  // 随包插件的角色：后端 `DllRole::Mod`（另一半是「依赖」）
  pluginRoleMod: {
    'zh-CN': '普通插件',
    'zh-TW': '一般外掛',
    en: 'Regular plugin',
    ko: '일반 플러그인',
    ja: '通常プラグイン',
  },
}
