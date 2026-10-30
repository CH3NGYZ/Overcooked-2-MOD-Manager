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
  // ===== 界面文案 =====
  title: {
    'zh-CN': '地图管理',
    'zh-TW': '地圖管理',
    en: 'Maps',
    ko: '지도 관리',
    ja: 'マップ管理',
  },
  noLevelSets: {
    'zh-CN': '还没有安装任何地图。可以把 ZIP 拖进来，或点击右上角「导入 ZIP」。',
    'zh-TW': '還沒有安裝任何地圖。可以把 ZIP 拖進來，或點擊右上角「匯入 ZIP」。',
    en: 'No maps installed yet. Drop a ZIP here, or use "Import ZIP" above.',
    ko: '설치된 지도가 없습니다. ZIP을 끌어다 놓거나 위의 "ZIP 가져오기"를 사용하세요.',
    ja: 'マップがまだありません。ZIP をドロップするか、右上の「ZIP をインポート」を使ってください。',
  },
  importZip: {
    'zh-CN': '导入 ZIP',
    'zh-TW': '匯入 ZIP',
    en: 'Import ZIP',
    ko: 'ZIP 가져오기',
    ja: 'ZIP をインポート',
  },
  importTitle: {
    'zh-CN': '导入地图',
    'zh-TW': '匯入地圖',
    en: 'Import maps',
    ko: '지도 가져오기',
    ja: 'マップをインポート',
  },
  importConfirm: {
    'zh-CN': '将安装 {count} 个地图。',
    'zh-TW': '將安裝 {count} 個地圖。',
    en: 'This will install {count} map(s).',
    ko: '지도 {count}개를 설치합니다.',
    ja: 'マップを {count} 個インストールします。',
  },
  importConfirmReplace: {
    'zh-CN': '以下地图已存在，安装会先把旧版本移入回收站：{names}',
    'zh-TW': '以下地圖已存在，安裝會先把舊版本移入資源回收筒：{names}',
    en: 'These maps already exist; the old versions will be moved to the recycle bin first: {names}',
    ko: '다음 지도가 이미 있습니다. 이전 버전은 먼저 휴지통으로 이동합니다: {names}',
    ja: '以下のマップは既に存在します。インストール時に旧バージョンをゴミ箱へ移動します: {names}',
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
    'zh-CN': '启用时地图放在 levels，停用时移到 disabled（游戏运行中暂不支持切换）',
    'zh-TW': '啟用時地圖放在 levels，停用時移到 disabled（遊戲執行中暫不支援切換）',
    en: 'Enabled maps live in levels; disabled ones move to disabled (not available while the game is running)',
    ko: '활성화하면 levels, 비활성화하면 disabled로 이동합니다(게임 실행 중에는 지원되지 않음)',
    ja: '有効時は levels、無効時は disabled に移動します（ゲーム実行中は切り替えできません）',
  },
}
