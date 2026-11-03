import type { DomainMessages } from './types'

/**
 * 内置教程：MOD 两页（`tutorial.mods.*`）。
 *
 * 覆盖 `steps.mods.ts` 里的 11 步：安装页 6 步（搜索 → 卡片 → 详情面板 →
 * 说明/下载分段 → 历史版本 → 资产与安装）、管理页 5 步（状态标签 → 批量开关 →
 * 检测更新 → 打开目录 → 第一张本地卡片）。
 *
 * 标题只写「这块地方叫什么」，正文讲「它是干什么的、点下去会发生什么」，
 * 不复述按钮上已经写着的字。安装页那四步的目标在抽屉里，用户不点开卡片时
 * 教程会退化成居中卡片，所以正文必须能脱离高亮框独立看懂。
 *
 * 域内 key 带 `mods.` 段（与 `maps.` / `overview.` / `tools.` 同一惯例）：
 * 这里的 `'mods.onlineSearch.title'` 经 `domain: 'tutorial'` 拼成
 * `tutorial.mods.onlineSearch.title`，`steps.mods.ts` 里写的正是完整 key。
 */
export const tutorialMods: DomainMessages = {
  'mods.onlineSearch.title': {
    'zh-CN': '搜索仓库',
    'zh-TW': '搜尋倉庫',
    en: 'Search repositories',
    ko: '저장소 검색',
    ja: 'リポジトリを検索',
  },
  'mods.onlineSearch.body': {
    'zh-CN': '按 MOD 名称、作者或仓库名过滤这张列表，输入时会自动展开所有分类。清空后回到完整列表；标题右侧的小标签是当前下载渠道。',
    'zh-TW': '依 MOD 名稱、作者或倉庫名篩選這份清單，輸入時會自動展開所有分類。清空後回到完整清單；標題右側的小標籤是目前下載渠道。',
    en: 'Filter this list by MOD name, author or repository; every group expands while you type. Clear the field to see everything again.',
    ko: 'MOD 이름, 작성자, 저장소 이름으로 이 목록을 필터링합니다. 입력하는 동안 모든 분류가 자동으로 펼쳐지고, 지우면 전체 목록으로 돌아갑니다.',
    ja: 'MOD 名・作者・リポジトリ名でこの一覧を絞り込みます。入力中はすべての分類が自動で展開され、消すと全件に戻ります。',
  },
  'mods.onlineCard.title': {
    'zh-CN': '仓库卡片',
    'zh-TW': '倉庫卡片',
    en: 'Repository card',
    ko: '저장소 카드',
    ja: 'リポジトリのカード',
  },
  'mods.onlineCard.body': {
    'zh-CN': '一张卡就是一个仓库：标题下是说明，右侧是本地安装状态、最新版本和下载量。点卡片任意位置，从右边拉出详情。',
    'zh-TW': '一張卡就是一個倉庫：標題下是說明，右側是本機安裝狀態、最新版本和下載量。點卡片任一位置，從右邊拉出詳情。',
    en: 'Each card is one repository: a note, local install state, newest version and download count. Click it to open the detail panel.',
    ko: '카드 하나가 저장소 하나입니다. 설명과 함께 로컬 설치 상태, 최신 버전, 다운로드 수가 보이며, 카드를 클릭하면 오른쪽에 상세 패널이 열립니다.',
    ja: '1 枚のカードが 1 つのリポジトリです。説明に加えて、ローカルのインストール状態・最新バージョン・ダウンロード数が並び、クリックすると右側に詳細パネルが開きます。',
  },
  'mods.onlineDetail.title': {
    'zh-CN': '详情面板',
    'zh-TW': '詳情面板',
    en: 'Detail panel',
    ko: '상세 패널',
    ja: '詳細パネル',
  },
  'mods.onlineDetail.body': {
    'zh-CN': '右侧滑出的面板：上面是说明、依赖与 README，切到「下载」页才能选版本、点安装。教程不会真的安装任何东西。',
    'zh-TW': '右側滑出的面板：上面是說明、依賴與 README，切到「下載」頁才能選版本、按安裝。教學不會真的安裝任何東西。',
    en: 'Slides in from the right: notes and dependencies up top; the Download tab has versions and the install button. Nothing is installed here.',
    ko: '오른쪽에서 열리는 패널입니다. 위쪽은 설명·의존성·README 이고, 다운로드 탭에서 버전을 고르고 설치합니다. 튜토리얼은 실제로 아무것도 설치하지 않습니다.',
    ja: '右から開くパネルです。上は説明・依存関係・README で、「ダウンロード」タブにバージョン選択とインストールがあります。ツアーは実際には何もインストールしません。',
  },
  'mods.detailTabs.title': {
    'zh-CN': '说明 / 下载两个页签',
    'zh-TW': '說明 / 下載兩個頁籤',
    en: 'Two tabs: Details and Download',
    ko: '설명 / 다운로드 두 탭',
    ja: '説明 / ダウンロードの 2 タブ',
  },
  'mods.detailTabs.body': {
    'zh-CN': '这个分段 bar 把面板分成两页：「说明」看备注与 README，「下载」选版本、看提交日志、点安装。两边说的是同一个 Release。',
    'zh-TW': '這個分段列把面板分成兩頁：「說明」看備註與 README，「下載」選版本、看提交日誌、按安裝。兩邊講的是同一個 Release。',
    en: 'Splits the panel in two: Details holds notes and the README, Download holds version choice, release notes and install — same Release.',
    ko: '이 세그먼트가 패널을 두 페이지로 나눕니다. 설명은 비고와 README, 다운로드는 버전 선택·변경 로그·설치 버튼이며, 양쪽 모두 같은 Release입니다.',
    ja: 'このセグメントでパネルが 2 ページに分かれます。「説明」はメモと README、「ダウンロード」はバージョン選択・変更履歴・インストールで、どちらも同じ Release の話です。',
  },
  'mods.detailVersion.title': {
    'zh-CN': '手动选历史版本',
    'zh-TW': '手動選歷史版本',
    en: 'Pick an older version',
    ko: '이전 버전 직접 선택',
    ja: '過去バージョンを選ぶ',
  },
  'mods.detailVersion.body': {
    'zh-CN': '「版本」下拉列出这个仓库的全部 Release，默认最新一版。想装旧版或测试版在这里选；下面的提交日志会跟着换。',
    'zh-TW': '「版本」下拉列出這個倉庫所有 Release，預設最新版。要裝舊版或測試版在這裡選；下面的提交日誌會跟著換。',
    en: 'The Version dropdown lists every Release, newest first. Pick an older or pre-release build and the log below follows your choice.',
    ko: '버전 드롭다운에는 이 저장소의 모든 Release가 나열되고 기본값은 최신 버전입니다. 예전 버전이나 테스트 버전을 고르면 아래 변경 로그도 함께 바뀝니다.',
    ja: '「バージョン」のドロップダウンにこのリポジトリの全 Release が並び、既定は最新版です。旧版やプレリリースを選ぶと、下の更新履歴も切り替わります。',
  },
  'mods.detailInstall.title': {
    'zh-CN': '选资产，然后安装',
    'zh-TW': '選資產，然後安裝',
    en: 'Choose the asset, then install',
    ko: '파일 선택 후 설치',
    ja: 'ファイルを選んでインストール',
  },
  'mods.detailInstall.body': {
    'zh-CN': '这一行左边挑要安装的文件，中间是大小与下载量，右边是安装按钮。按下去才下载解析，落盘前还会再确认一次。',
    'zh-TW': '這一行左邊挑要安裝的檔案，中間是大小與下載量，右邊是按鈕。按下去才下載解析，寫入前還會再確認一次。',
    en: 'Pick the file to install, check its size and download count, then press the button on the right. Nothing is written until you confirm again.',
    ko: '왼쪽에서 설치할 파일을 고르고, 가운데에서 크기와 다운로드 수를 확인한 뒤 오른쪽 버튼으로 시작합니다. 실제 설치 전에 한 번 더 확인합니다.',
    ja: '左でインストールするファイルを選び、中央でサイズとダウンロード数を確認し、右のボタンで開始します。書き込む前にもう一度確認します。',
  },
  'mods.localTags.title': {
    'zh-CN': '状态标签过滤',
    'zh-TW': '狀態標籤過濾',
    en: 'Filter by status tags',
    ko: '상태 태그 필터',
    ja: '状態タグで絞り込み',
  },
  'mods.localTags.body': {
    'zh-CN': '页头这几个带数字的标签是整页统计：点一下滚到并高亮符合的卡片。数量为 0 的状态不显示。',
    'zh-TW': '頁頭這幾個帶數字的標籤是整頁統計：點一下會捲到並高亮符合的卡片。數量為 0 的狀態不顯示。',
    en: 'These counted tags summarise the whole page: click one to jump to and highlight the matching cards. States with zero items stay hidden.',
    ko: '숫자가 붙은 태그는 페이지 전체 통계입니다. 클릭하면 해당 카드로 이동해 강조하고, 0개인 상태는 표시하지 않습니다.',
    ja: '数字付きのタグはページ全体の集計です。クリックすると該当カードまで移動して強調表示します。0 件の状態は出ません。',
  },
  'mods.localBulk.title': {
    'zh-CN': '全部启用 / 全部禁用',
    'zh-TW': '全部啟用 / 全部停用',
    en: 'Enable all / disable all',
    ko: '모두 켜기 / 모두 끄기',
    ja: 'すべて有効 / すべて無効',
  },
  'mods.localBulk.body': {
    'zh-CN': '二选一分段器：一次开或关掉整页 MOD。禁用方向会先弹确认；只有一部分开着时两段都不选中。',
    'zh-TW': '二選一分段器：一次開或關掉整頁 MOD。停用方向會先跳確認；只有一部分開著時兩段都不選中。',
    en: 'Enables or disables every MOD on the page at once. Disabling asks first; when only some are on, neither side is selected.',
    ko: '페이지의 모든 MOD를 한 번에 켜거나 끄는 2택 세그먼트입니다. 끌 때는 확인을 먼저 받고, 일부만 켜져 있으면 어느 쪽도 선택되지 않습니다.',
    ja: 'ページ内の MOD をまとめて有効/無効にする 2 択です。無効化は確認が出て、一部だけ有効なときはどちらも選択されません。',
  },
  'mods.localActions.title': {
    'zh-CN': '检测更新',
    'zh-TW': '檢測更新',
    en: 'Check for updates',
    ko: '업데이트 확인',
    ja: '更新を確認',
  },
  'mods.localActions.body': {
    'zh-CN': '只检查已安装 MOD 绑定的那些仓库有没有新版本，不重扫整份源文档。结果会以标签形式常驻在页头。',
    'zh-TW': '只檢查已安裝 MOD 綁定的那些倉庫有沒有新版本，不重掃整份源文件。結果會以標籤形式常駐在頁頭。',
    en: 'Only checks the repositories your installed MODs are bound to — no full source rescan. The result stays in the header.',
    ko: '설치된 MOD가 연결된 저장소만 새 버전이 있는지 확인합니다. 전체 소스 문서를 다시 읽지 않으며, 결과는 이후 헤더에 남습니다.',
    ja: 'インストール済み MOD が紐づくリポジトリだけ、新しいバージョンがあるか確認します。ソース全体の再読み込みはしません。結果はヘッダーに残ります。',
  },
  'mods.localOpenDir.title': {
    'zh-CN': '打开 MOD 目录',
    'zh-TW': '開啟 MOD 目錄',
    en: 'Open the MOD folder',
    ko: 'MOD 폴더 열기',
    ja: 'MOD フォルダーを開く',
  },
  'mods.localOpenDir.body': {
    'zh-CN': '在文件管理器里打开 BepInEx 的插件目录。想手动放 DLL、核对文件或看日志，从这里进去最快。',
    'zh-TW': '在檔案總管裡開啟 BepInEx 的外掛目錄。想手動放 DLL、核對檔案或看日誌，從這裡進去最快。',
    en: 'Opens the BepInEx plugins folder in your file manager — the quickest way to drop in a DLL by hand or check what is on disk.',
    ko: 'BepInEx 플러그인 폴더를 파일 탐색기로 엽니다. DLL을 직접 넣거나 디스크 상태를 확인할 때 가장 빠릅니다.',
    ja: 'BepInEx のプラグインフォルダをファイルマネージャーで開きます。DLL を手で置いたり中身を確かめたいときに一番早い入口です。',
  },
  'mods.localCard.title': {
    'zh-CN': '本地 MOD 卡',
    'zh-TW': '本機 MOD 卡片',
    en: 'Local MOD card',
    ko: '로컬 MOD 카드',
    ja: 'ローカル MOD のカード',
  },
  'mods.localCard.body': {
    'zh-CN': '一行就是一个 MOD：左边开关，中间标题带版本号，右边是状态标签和删除、配置等按钮，点卡片展开详情。（本地还没装 MOD 时，教程会补两张带「示例」字样的卡片。）',
    'zh-TW': '一行就是一個 MOD：左邊開關，中間標題帶版本號，右邊是狀態標籤和刪除、設定等按鈕，點卡片展開詳情。（本機還沒裝 MOD 時，教學會補兩張帶「範例」字樣的卡片。）',
    en: 'One row per MOD: switch on the left, title and version in the middle, tags and actions on the right. Click to expand details. (If nothing is installed yet, the tutorial shows two sample cards marked “Sample”.)',
    ko: '한 줄이 MOD 하나입니다. 왼쪽은 스위치, 가운데는 버전이 붙은 제목, 오른쪽은 상태 태그와 삭제·설정 버튼이고, 카드를 클릭하면 상세가 펼쳐집니다. (아직 설치된 MOD가 없으면 튜토리얼이 “예시”라고 표시된 카드 두 개를 대신 보여 줍니다.)',
    ja: '1 行が 1 つの MOD です。左がスイッチ、中央がバージョン付きのタイトル、右が状態タグと削除・設定ボタン。カードをクリックすると詳細が開きます。(まだ MOD を入れていない場合、チュートリアルでは「サンプル」と付いたカードを 2 枚表示します。)',
  },
}
