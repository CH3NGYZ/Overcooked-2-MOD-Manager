import type { DomainMessages } from './types'

/**
 * 内置教程：总览页（`tutorial.overview.*`）。
 *
 * 八步对应首页的两列卡片：左列 MOD 链路（整列 → 框架 → 在线 → 本地），
 * 右列地图链路（整列 → 运行时 → 在线 → 本地）。每张卡都只讲三件事：
 * 这张卡是什么、数值从哪来、点它会发生什么 —— 用户扫一眼首页时真正
 * 想知道的就是「这块要不要我动手」。
 *
 * 文案里不写具体版本号、仓库名或地图名（数据会变），也不复述标签上
 * 已经印着的字；「去刷新」「去更新」这类按钮文案与页面上的实际按钮一致，
 * 用户照着找得到。
 *
 * key 是「域内相对 key」：`overview.<短名>.title`，登记后即
 * `t('tutorial.overview.<短名>.title')`。
 */
export const tutorialOverview: DomainMessages = {
  // ===== 左列：MOD 链路 =====
  // 左列：MOD 链路
  'overview.columnMods.title': {
    'zh-CN': '左列：MOD 链路',
    'zh-TW': '左欄：MOD 鏈路',
    en: 'Left column: MOD chain',
    ko: '왼쪽 열: MOD 체인',
    ja: '左列: MOD チェーン',
  },
  'overview.columnMods.body': {
    'zh-CN': '从上到下就是装 MOD 的顺序：先框架，再在线下载，最后管本地磁盘。点任意一张卡都能跳到对应页面。',
    'zh-TW': '由上到下就是安裝 MOD 的順序：先框架，再到線上取得，最後管本機磁碟。點任一張卡都能跳到對應頁面。',
    en: 'Top to bottom is the order you install MODs: framework, then downloads from online repositories, then what is on disk. Click any card to open its page.',
    ko: '위에서부터 MOD를 설치하는 순서입니다. 프레임워크, 온라인 다운로드, 로컬 디스크 관리 순이며 카드를 누르면 해당 페이지로 이동합니다.',
    ja: '上から順に MOD を導入する流れです。フレームワーク、オンライン取得、ローカル管理の順で、カードを押すと各ページへ移動します。',
  },
  // BepInEx 框架
  'overview.bepinex.title': {
    'zh-CN': 'BepInEx 框架',
    'zh-TW': 'BepInEx 框架',
    en: 'BepInEx framework',
    ko: 'BepInEx 프레임워크',
    ja: 'BepInEx フレームワーク',
  },
  'overview.bepinex.body': {
    'zh-CN': '所有 MOD 都靠它加载。数值是装没装、开没开，下一行是版本号；右侧开关直接启停，点卡片去设置页的框架抽屉安装或备份。',
    'zh-TW': '所有 MOD 都靠它載入。數值是裝沒裝、開沒開，下一行是版本號；右側開關直接啟停，點卡片到設定頁的框架抽屜安裝或備份。',
    en: 'Every MOD is loaded through it. The value shows whether it is installed and enabled, the line below is the version; the switch toggles it, and clicking the card opens the framework drawer in Settings to install or back up.',
    ko: '모든 MOD는 이 프레임워크로 불러옵니다. 값은 설치·활성 상태이고 아래 줄은 버전입니다. 오른쪽 스위치로 켜고 끄며, 카드를 누르면 설정의 프레임워크 서랍에서 설치나 백업을 할 수 있습니다.',
    ja: 'すべての MOD はこれ経由で読み込まれます。値はインストール済みか有効か、下の行はバージョンです。右のスイッチで切り替え、カードを押すと設定のフレームワーク引き出しで導入やバックアップができます。',
  },
  // 在线 MOD
  'overview.onlineMods.title': {
    'zh-CN': '在线 MOD',
    'zh-TW': '線上 MOD',
    en: 'Online MODs',
    ko: '온라인 MOD',
    ja: 'オンラインMOD',
  },
  'overview.onlineMods.body': {
    'zh-CN': '数值是当前地区可见的仓库数，副标题说明有没有刷新失败。点卡片进下载页；有失败时按钮变「去刷新」，直接跳到出错的那个。',
    'zh-TW': '數值是目前地區看得到幾個倉庫，副標題說明有沒有重新整理失敗。點卡片進下載頁；有倉庫失敗時按鈕變成「去重新整理」，直接跳到出錯的那個。',
    en: 'The value counts repositories visible in your region, and the subtitle says whether any refresh failed. Click the card to open the download page; if some failed, the button jumps straight to the first broken repository.',
    ko: '값은 현재 지역에서 보이는 저장소 수이고, 부제는 새로 고침 실패 여부를 알려 줍니다. 카드를 누르면 다운로드 페이지로 가며, 실패가 있으면 버튼이 첫 실패 저장소로 바로 이동합니다.',
    ja: '値は現在の地域で見えるリポジトリ数、副題は更新に失敗したかどうかです。カードを押すとダウンロードページへ移動し、失敗があればボタンで最初の失敗リポジトリへ直接飛べます。',
  },
  // 本地 MOD
  'overview.localMods.title': {
    'zh-CN': '本地 MOD',
    'zh-TW': '本機 MOD',
    en: 'Local MODs',
    ko: '로컬 MOD',
    ja: 'ローカルMOD',
  },
  'overview.localMods.body': {
    'zh-CN': '数值是已装的 MOD 数（不含随包依赖），旁边是可更新数。点卡片进管理页，有更新时直接定位到第一个可更新的 MOD。',
    'zh-TW': '數值是磁碟上已安裝的 MOD 數（不含隨包依賴），旁邊是可更新的數量。點卡片進管理頁，有更新時直接定位到第一個可更新的 MOD。',
    en: 'The value counts MODs installed on disk (bundled dependencies excluded), with the updatable count beside it. Click the card to open the manage page, which jumps to the first updatable MOD when there is one.',
    ko: '값은 디스크에 설치된 MOD 수(동봉 의존성 제외)이고 옆에 업데이트 가능 수가 붙습니다. 카드를 누르면 관리 페이지로 가며, 업데이트가 있으면 첫 항목으로 바로 이동합니다.',
    ja: '値はディスク上の MOD 数(同梱依存を除く)で、隣が更新可能数です。カードを押すと管理ページへ移動し、更新があれば最初の更新可能な MOD に直接移動します。',
  },

  // ===== 右列：地图链路 =====
  // 右列：地图链路
  'overview.columnMaps.title': {
    'zh-CN': '右列：地图链路',
    'zh-TW': '右欄：地圖鏈路',
    en: 'Right column: map chain',
    ko: '오른쪽 열: 지도 체인',
    ja: '右列: マップチェーン',
  },
  'overview.columnMaps.body': {
    'zh-CN': '与左列一一对应：运行时、在线地图、本地地图。自定义地图也要先装运行时才会被加载；安装、开关、更新与 MOD 同一套逻辑。',
    'zh-TW': '與左欄一一對應：執行環境、線上地圖、本機地圖。自訂地圖也要先裝執行環境才會被載入；安裝、開關、更新與 MOD 是同一套邏輯。',
    en: 'The mirror image of the left column: runtime, online maps, local maps. Custom maps also need the runtime before the game loads them; installing, switching and updating work exactly like MODs.',
    ko: '왼쪽 열과 대응됩니다. 런타임, 온라인 지도, 로컬 지도 순입니다. 사용자 지도도 런타임을 먼저 설치해야 로드되며, 설치·켜기·업데이트는 MOD와 같은 방식입니다.',
    ja: '左列と対になる並びです。ランタイム、オンライン地図、ローカル地図。カスタムマップもランタイムを入れて初めて読み込まれ、導入・オンオフ・更新は MOD と同じ仕組みです。',
  },
  // DIYLevel 运行时
  'overview.diyLevel.title': {
    'zh-CN': 'DIYLevel 运行时',
    'zh-TW': 'DIYLevel 執行環境',
    en: 'DIYLevel runtime',
    ko: 'DIYLevel 런타임',
    ja: 'DIYLevel ランタイム',
  },
  'overview.diyLevel.body': {
    'zh-CN': '自定义地图进游戏的前提。数值是启用状态，下一行是版本号与地图集数量；点卡片去设置页的抽屉安装、备份或恢复。',
    'zh-TW': '自訂地圖進遊戲的前提。數值是啟用狀態，下一行是版本號與地圖集數量；點卡片到設定頁的抽屜安裝、備份或還原。',
    en: 'What custom maps need to show up in game. The value is the enabled state, the line below shows the version and how many map packs are installed; clicking the card opens the drawer in Settings to install, back up or restore it.',
    ko: '사용자 지도가 게임에 나오기 위한 전제입니다. 값은 활성 상태, 아래 줄은 버전과 지도 팩 수입니다. 카드를 누르면 설정의 서랍에서 설치·백업·복원할 수 있습니다.',
    ja: 'カスタムマップをゲームで使うための前提です。値は有効状態、下の行はバージョンとマップパック数です。カードを押すと設定の引き出しから導入・バックアップ・復元ができます。',
  },
  // 在线地图
  'overview.onlineMaps.title': {
    'zh-CN': '在线地图',
    'zh-TW': '線上地圖',
    en: 'Online maps',
    ko: '온라인 지도',
    ja: 'オンライン地図',
  },
  'overview.onlineMaps.body': {
    'zh-CN': '数值是地图源里能下载的地图条目数，副标题写着有几条刷新失败。点卡片进下载页；有失败时直接定位到第一张刷新失败的地图。',
    'zh-TW': '數值是地圖源裡能下載的地圖條目數，副標題寫著幾條重新整理失敗。點卡片進下載頁；有失敗時直接定位到第一張重新整理失敗的地圖。',
    en: 'The value counts downloadable entries in the map sources, and the subtitle reports how many failed to refresh. Click the card to open the download page; if any failed, it jumps to the first one.',
    ko: '값은 지도 소스에서 받을 수 있는 지도 항목 수이고, 부제에 새로 고침 실패 수가 나옵니다. 카드를 누르면 다운로드 페이지로 가며, 실패가 있으면 첫 실패 지도로 이동합니다.',
    ja: '値はマップソースから取得できる地図の件数、副題は更新に失敗した件数です。カードを押すとダウンロードページへ移動し、失敗があれば最初の失敗した地図へ飛びます。',
  },
  // 本地地图
  'overview.localMaps.title': {
    'zh-CN': '本地地图',
    'zh-TW': '本機地圖',
    en: 'Local maps',
    ko: '로컬 지도',
    ja: 'ローカル地図',
  },
  'overview.localMaps.body': {
    'zh-CN': '数值是本地关卡集数量，一个文件夹算一张，读盘中显示「—」；旁边是可更新的数量。点卡片进管理页，有更新时定位到第一张。',
    'zh-TW': '數值是本機關卡集數量，一個資料夾算一張，讀取中顯示「—」；旁邊是可更新的數量。點卡片進管理頁，有更新時定位到第一張。',
    en: 'The value counts local level sets, one folder per map, and shows “—” while reading; the updatable count is beside it. Click the card to open the manage page, which jumps to the first updatable map.',
    ko: '값은 로컬 레벨 세트 수(폴더 하나가 지도 하나)이고 읽는 동안에는 "—"로 표시됩니다. 옆은 업데이트 가능 수입니다. 카드를 누르면 관리 페이지로 가서 첫 항목으로 이동합니다.',
    ja: '値はローカルのレベルセット数(フォルダ 1 つが地図 1 枚)で、読み込み中は「—」と表示されます。隣が更新可能数です。カードを押すと管理ページへ移動し、更新があれば先頭へ移動します。',
  },
}
