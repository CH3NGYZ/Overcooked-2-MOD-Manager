import type { DomainMessages } from '../types.ts'

/** MOD/地图来源、预览、冲突检查、安装与移除确认。同一 key 的五语保持集中。 */
export const toolsUiInstall: DomainMessages = {
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
  // 地图包（拖入/选出的是含 info 的 ZIP）：安装详情头部的类型标签
  mapPackage: {
    'zh-CN': '地图包',
    'zh-TW': '地圖包',
    en: 'Map package',
    ko: '지도 패키지',
    ja: 'マップパッケージ',
  },
  // 地图包头部摘要：{dlls} 个 DLL（含随包插件）· {sets} 个地图集
  mapPackageSummary: {
    'zh-CN': '{dlls} 个 DLL · {sets} 个地图集',
    'zh-TW': '{dlls} 個 DLL · {sets} 個地圖集',
    en: '{dlls} DLLs · {sets} map sets',
    ko: 'DLL {dlls}개 · 지도 세트 {sets}개',
    ja: 'DLL {dlls} 件 · マップセット {sets} 件',
  },
  // 把 {types} 拖到这里，或点击选择文件（{types} 形如「.dll / .zip」）
  dropFilesHereOrClickToChoose: {
    'zh-CN': '把 {types} 拖到这里，或点击选择文件',
    'zh-TW': '把 {types} 拖到這裡，或點擊選擇檔案',
    en: 'Drop a {types} file here, or click to choose one',
    ko: '{types} 파일을 여기로 끌어오거나 클릭해 선택하세요',
    ja: '{types} をここにドロップするか、クリックして選択します',
  },
  // 这是地图包（把地图包拖到 MOD 侧时的弹窗标题）
  mapPackageDetected: {
    'zh-CN': '这是地图包',
    'zh-TW': '這是地圖包',
    en: 'This is a map package',
    ko: '지도 패키지입니다',
    ja: 'これはマップパッケージです',
  },
  // 这个 ZIP 里含地图资源（info 文件）……请到地图安装页导入
  mapPackageDetectedHint: {
    'zh-CN': '这个 ZIP 里包含地图资源（info 文件），不能当作 MOD 安装。请到地图安装页导入；作者随包的修复插件会一起装进地图目录，并随地图一起启停。',
    'zh-TW': '這個 ZIP 裡包含地圖資源（info 檔案），不能當作 MOD 安裝。請到地圖安裝頁匯入；作者附帶的修復外掛會一起裝進地圖目錄，並隨地圖一起啟停。',
    en: 'This ZIP contains map resources (info files), so it cannot be installed as a mod. Import it on the map install page — the bug-fix plugins bundled by the author are installed into the map folder and enabled/disabled together with the map.',
    ko: '이 ZIP에는 지도 리소스(info 파일)가 들어 있어 MOD로 설치할 수 없습니다. 지도 설치 페이지에서 가져오세요. 제작자가 함께 넣은 수정 플러그인도 지도 폴더에 함께 설치되고 지도와 함께 켜고 꺼집니다.',
    ja: 'この ZIP にはマップリソース（info ファイル）が含まれているため、MOD としてインストールできません。マップのインストール画面から取り込んでください。作者が同梱した修正プラグインもマップフォルダーに一緒にインストールされ、マップと一緒に有効／無効になります。',
  },
  // 这是 MOD 包（把 MOD 包拖到地图侧时的弹窗标题）
  modPackageDetected: {
    'zh-CN': '这是 MOD 包',
    'zh-TW': '這是 MOD 包',
    en: 'This is a mod package',
    ko: 'MOD 패키지입니다',
    ja: 'これは MOD パッケージです',
  },
  // 这个 ZIP 里没有地图资源（info），只有 DLL/插件……请到 MOD 安装页安装
  modPackageDetectedHint: {
    'zh-CN': '这个 ZIP 里没有地图资源（info 文件），只有 DLL / 插件，不能当作地图安装。请到 MOD 安装页安装。',
    'zh-TW': '這個 ZIP 裡沒有地圖資源（info 檔案），只有 DLL / 外掛，不能當作地圖安裝。請到 MOD 安裝頁安裝。',
    en: 'This ZIP has no map resources (info files) — only DLLs/plugins — so it cannot be installed as a map. Install it on the mod install page.',
    ko: '이 ZIP에는 지도 리소스(info 파일)가 없고 DLL/플러그인만 있어 지도로 설치할 수 없습니다. MOD 설치 페이지에서 설치하세요.',
    ja: 'この ZIP にはマップリソース（info ファイル）がなく、DLL／プラグインだけなのでマップとしてインストールできません。MOD のインストール画面からインストールしてください。',
  },
  // 既不是地图包（没有 info）也不是 MOD 包（没有 DLL），无法判断类型
  unknownPackageHint: {
    'zh-CN': '这个文件既不是地图包（没有 info 文件）也不是 MOD 包（没有 DLL），无法判断类型。',
    'zh-TW': '這個檔案既不是地圖包（沒有 info 檔案）也不是 MOD 包（沒有 DLL），無法判斷類型。',
    en: 'This file is neither a map package (no info file) nor a mod package (no DLL), so the type cannot be determined.',
    ko: '이 파일은 지도 패키지(info 파일 없음)도 MOD 패키지(DLL 없음)도 아니어서 종류를 판단할 수 없습니다.',
    ja: 'このファイルはマップパッケージ（info ファイルなし）でも MOD パッケージ（DLL なし）でもないため、種類を判定できません。',
  },
  // 松开即可安装 {types}（整窗拖放提示条；{types} 形如「.dll / .zip」）
  releaseToInstall: {
    'zh-CN': '松开即可安装 {types}',
    'zh-TW': '放開即可安裝 {types}',
    en: 'Release to install {types}',
    ko: '놓으면 {types} 설치가 시작됩니다',
    ja: '{types} を離すとインストールします',
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
  // 卸载并安装（有冲突时的确认按钮：先卸载旧版本再安装）
  uninstallAndInstall: {
    'zh-CN': '卸载并安装',
    'zh-TW': '解除安裝並安裝',
    en: 'Uninstall and install',
    ko: '제거 후 설치',
    ja: 'アンインストールしてインストール',
  },
  // 以下已安装内容与本次安装的 ID / 地图身份相同，需要先卸载再安装：
  installConflictHint: {
    'zh-CN': '以下已安装内容与本次安装的 ID / 地图身份相同，需要先卸载再安装：',
    'zh-TW': '以下已安裝內容與本次安裝的 ID / 地圖身分相同，需要先解除安裝再安裝：',
    en: 'The following installed items match this install by ID or map identity and must be uninstalled first:',
    ko: '다음 항목이 이번에 설치할 내용과 ID 또는 지도 식별자가 같아 먼저 제거해야 합니다:',
    ja: '以下は今回インストールする内容と ID / マップ識別子が同じため、先にアンインストールする必要があります：',
  },
  // 安装「{name}」前，需要先卸载以下已安装内容：
  beforeInstallingTargetMustBeUninstalledFirst: {
    'zh-CN': '安装「{name}」前，需要先卸载以下已安装内容：',
    'zh-TW': '安裝「{name}」前，需要先解除安裝以下已安裝內容：',
    en: 'Before installing "{name}", the following installed items must be uninstalled:',
    ko: '"{name}" 설치 전에 다음 설치된 항목을 먼저 제거해야 합니다:',
    ja: '「{name}」をインストールする前に、以下のインストール済み項目をアンインストールする必要があります：',
  },
  // 同版本（冲突行的行尾标签）
  installConflictSameVersion: {
    'zh-CN': '同版本',
    'zh-TW': '同版本',
    en: 'Same version',
    ko: '같은 버전',
    ja: '同じバージョン',
  },
  // 卸载「{name}」失败：{reason}，请手动卸载后重试
  installConflictFailed: {
    'zh-CN': '卸载「{name}」失败：{reason}，请手动卸载后重试',
    'zh-TW': '解除安裝「{name}」失敗：{reason}，請手動解除安裝後再試',
    en: 'Failed to uninstall "{name}": {reason}. Uninstall it manually and try again.',
    ko: '"{name}" 제거에 실패했습니다: {reason}. 직접 제거한 뒤 다시 시도하세요.',
    ja: '「{name}」のアンインストールに失敗しました: {reason}。手動でアンインストールしてから再試行してください。',
  },
  // 这个压缩包里没有 DLL 文件
  thisArchiveContainsNoDllFiles: {
    'zh-CN': '这个压缩包里没有 DLL 文件',
    'zh-TW': '這個壓縮檔裡沒有 DLL 檔案',
    en: 'This archive contains no DLL files',
    ko: '이 압축 파일에는 DLL이 없습니다',
    ja: 'このアーカイブに DLL は含まれていません',
  },
  // 跨域弹窗的肯定按钮：点了就跳到对面的安装页并接着走安装流程
  goToInstall: {
    'zh-CN': '去安装',
    'zh-TW': '去安裝',
    en: 'Go install',
    ko: '설치하러 가기',
    ja: 'インストールへ',
  },
  // 没有 DLL 时说明「为什么装不了、该去哪」——安装按钮同时被禁用，只能取消
  nothingInstallableHere: {
    'zh-CN': '没有可安装的内容，无法继续。如果这是地图包，请到地图安装页导入。',
    'zh-TW': '沒有可安裝的內容，無法繼續。如果這是地圖包，請到地圖安裝頁匯入。',
    en: 'There is nothing to install here. If this is a map package, import it on the map install page.',
    ko: '설치할 내용이 없습니다. 지도 패키지라면 지도 설치 페이지에서 가져오세요.',
    ja: 'インストールできる内容がありません。マップパッケージの場合はマップインストールページからインポートしてください。',
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
}
