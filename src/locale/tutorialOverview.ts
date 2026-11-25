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
  'overview.columnMods.title': { 'zh-CN': '左列：MOD 链路', 'zh-TW': '左欄：MOD 鏈路', en: 'Left column: MOD chain', ko: '왼쪽 열: MOD 체인', ja: '左列: MOD チェーン' },
  'overview.columnMods.body': { 'zh-CN': '从上到下就是装 MOD 的顺序：先框架，再在线下载，最后管本地磁盘。点任意一张卡都能跳到对应页面。', 'zh-TW': '由上到下就是安裝 MOD 的順序：先框架，再到線上取得，最後管本機磁碟。點任一張卡都能跳到對應頁面。', en: 'Top to bottom is the order you install MODs: framework, then downloads from online repositories, then what is on disk. Click any card to open its page.', ko: '위에서부터 MOD를 설치하는 순서입니다. 프레임워크, 온라인 다운로드, 로컬 디스크 관리 순이며 카드를 누르면 해당 페이지로 이동합니다.', ja: '上から順に MOD を導入する流れです。フレームワーク、オンライン取得、ローカル管理の順で、カードを押すと各ページへ移動します。' },
  // BepInEx 框架
  'overview.bepinex.title': { 'zh-CN': 'BepInEx 框架', 'zh-TW': 'BepInEx 框架', en: 'BepInEx framework', ko: 'BepInEx 프레임워크', ja: 'BepInEx フレームワーク' },
  'overview.bepinex.body': { 'zh-CN': '所有 MOD 都靠它加载。数值是装没装、开没开，下一行是版本号；右侧开关直接启停，点卡片去设置页的框架抽屉安装或备份。', 'zh-TW': '所有 MOD 都靠它載入。數值是裝沒裝、開沒開，下一行是版本號；右側開關直接啟停，點卡片到設定頁的框架抽屜安裝或備份。', en: 'Every MOD is loaded through it. The value shows whether it is installed and enabled, the line below is the version; the switch toggles it, and clicking the card opens the framework drawer in Settings to install or back up.', ko: '모든 MOD는 이 프레임워크로 불러옵니다. 값은 설치·활성 상태이고 아래 줄은 버전입니다. 오른쪽 스위치로 켜고 끄며, 카드를 누르면 설정의 프레임워크 서랍에서 설치나 백업을 할 수 있습니다.', ja: 'すべての MOD はこれ経由で読み込まれます。値はインストール済みか有効か、下の行はバージョンです。右のスイッチで切り替え、カードを押すと設定のフレームワーク引き出しで導入やバックアップができます。' },
  // 在线 MOD
  'overview.onlineMods.title': { 'zh-CN': '在线 MOD', 'zh-TW': '線上 MOD', en: 'Online MODs', ko: '온라인 MOD', ja: 'オンラインMOD' },
  'overview.onlineMods.body': { 'zh-CN': '大数字是当前地区可见的在线 MOD 数。点卡片进下载页；有刷新失败时，「去刷新」会定位到第一个失败的条目。', 'zh-TW': '大數字是目前地區可見的線上 MOD 數。點卡片進下載頁；有重新整理失敗時，「去重新整理」會定位到第一個失敗的項目。', en: 'The large number counts online MODs visible in your region. Click the card to open the download page. If a refresh failed, Refresh jumps to the first affected entry.', ko: '큰 숫자는 현재 지역에서 볼 수 있는 온라인 MOD 수입니다. 카드를 누르면 다운로드 페이지가 열립니다. 새로 고침에 실패한 항목이 있으면 새로 고침 버튼으로 첫 항목으로 이동합니다.', ja: '大きな数字は現在の地域で表示されるオンライン MOD の件数です。カードを押すとダウンロードページが開き、更新に失敗した項目があれば「更新へ」で最初の項目に移動します。' },
  // 本地 MOD
  'overview.localMods.title': { 'zh-CN': '本地 MOD', 'zh-TW': '本機 MOD', en: 'Local MODs', ko: '로컬 MOD', ja: 'ローカルMOD' },
  'overview.localMods.body': { 'zh-CN': '大数字是已启用的 MOD 数，小字是已安装总数（含停用）。有更新时会单独标出数量。点卡片进管理页，有更新时会定位到第一个可更新的 MOD。', 'zh-TW': '大數字是已啟用的 MOD 數，小字是已安裝總數（含停用）。有更新時會另外標出數量。點卡片進管理頁，有更新時會定位到第一個可更新的 MOD。', en: 'The large number counts enabled MODs. The line below counts all installed MODs, including disabled ones. Updates are counted separately. Click the card to open the management page and jump to the first available update, if any.', ko: '큰 숫자는 활성화된 MOD 수이고, 아래 줄은 비활성 항목을 포함한 전체 설치 수입니다. 업데이트 수는 따로 표시됩니다. 카드를 누르면 관리 페이지가 열리고, 업데이트가 있으면 첫 항목으로 이동합니다.', ja: '大きな数字は有効な MOD の件数、下の行は無効なものも含めたインストール件数です。更新可能な件数は別に表示されます。カードを押すと管理ページが開き、更新があれば最初の項目に移動します。' },

  // ===== 右列：地图链路 =====
  // 右列：地图链路
  'overview.columnMaps.title': { 'zh-CN': '右列：地图链路', 'zh-TW': '右欄：地圖鏈路', en: 'Right column: map chain', ko: '오른쪽 열: 지도 체인', ja: '右列: マップチェーン' },
  'overview.columnMaps.body': { 'zh-CN': '与左列一一对应：运行时、在线地图、本地地图。自定义地图也要先装运行时才会被加载；安装、开关、更新与 MOD 同一套逻辑。', 'zh-TW': '與左欄一一對應：執行環境、線上地圖、本機地圖。自訂地圖也要先裝執行環境才會被載入；安裝、開關、更新與 MOD 是同一套邏輯。', en: 'The mirror image of the left column: runtime, online maps, local maps. Custom maps also need the runtime before the game loads them; installing, switching and updating work exactly like MODs.', ko: '왼쪽 열과 대응됩니다. 런타임, 온라인 지도, 로컬 지도 순입니다. 사용자 지도도 런타임을 먼저 설치해야 로드되며, 설치·켜기·업데이트는 MOD와 같은 방식입니다.', ja: '左列と対になる並びです。ランタイム、オンライン地図、ローカル地図。カスタムマップもランタイムを入れて初めて読み込まれ、導入・オンオフ・更新は MOD と同じ仕組みです。' },
  // DIYLevel 运行时
  'overview.diyLevel.title': { 'zh-CN': 'DIYLevel 运行时', 'zh-TW': 'DIYLevel 執行環境', en: 'DIYLevel runtime', ko: 'DIYLevel 런타임', ja: 'DIYLevel ランタイム' },
  'overview.diyLevel.body': { 'zh-CN': '自定义地图需要 DIYLevel 才能加载。卡片显示安装和启用状态，已安装时下方显示版本号。点卡片可在设置里安装、备份或恢复。', 'zh-TW': '自訂地圖需要 DIYLevel 才能載入。卡片顯示安裝和啟用狀態，已安裝時下方顯示版本號。點卡片可在設定裡安裝、備份或還原。', en: 'Custom maps require DIYLevel. The card shows whether it is installed and enabled, with the version below once installed. Click the card to install, back up or restore it in Settings.', ko: '사용자 지도를 불러오려면 DIYLevel이 필요합니다. 카드는 설치 및 활성 상태를 표시하며, 설치되어 있으면 아래 줄에 버전이 나옵니다. 카드를 누르면 설정에서 설치·백업·복원할 수 있습니다.', ja: 'カスタムマップの読み込みには DIYLevel が必要です。カードにはインストールと有効化の状態が表示され、インストール済みなら下にバージョンが表示されます。カードから設定を開き、導入・バックアップ・復元ができます。' },
  // 在线地图
  'overview.onlineMaps.title': { 'zh-CN': '在线地图', 'zh-TW': '線上地圖', en: 'Online maps', ko: '온라인 지도', ja: 'オンライン地図' },
  'overview.onlineMaps.body': { 'zh-CN': '同一张地图即使有多个下载来源，也只算一张。副标题提示目录是否已确认、有无刷新失败。点卡片进下载页；有失败时定位到第一个出错的条目。', 'zh-TW': '同一張地圖即使有多個下載來源，也只算一張。副標題提示目錄是否已確認、有無重新整理失敗。點卡片進下載頁；有失敗時定位到第一個出錯的項目。', en: 'A map counts once even if it has several download sources. The subtitle shows catalog or refresh status. Click the card to open the download page and jump to the first entry that failed to refresh, if any.', ko: '다운로드 소스가 여러 개여도 같은 지도는 한 번만 셉니다. 아래 줄에는 카탈로그 또는 새로 고침 상태가 표시됩니다. 카드를 누르면 다운로드 페이지가 열리고, 실패한 항목이 있으면 첫 항목으로 이동합니다.', ja: '同じ地図はダウンロード元が複数あっても 1 件と数えます。下の行にはカタログや更新の状態が表示されます。カードを押すとダウンロードページが開き、更新に失敗した項目があれば最初の項目に移動します。' },
  // 本地地图
  'overview.localMaps.title': { 'zh-CN': '本地地图', 'zh-TW': '本機地圖', en: 'Local maps', ko: '로컬 지도', ja: 'ローカル地図' },
  'overview.localMaps.body': { 'zh-CN': '大数字是已启用的地图数，小字是已安装总数（含停用）。还没有读取结果时显示「—」，不是 0。点卡片进管理页，有更新时会定位到第一张。', 'zh-TW': '大數字是已啟用的地圖數，小字是已安裝總數（含停用）。還沒有讀取結果時顯示「—」，不是 0。點卡片進管理頁，有更新時會定位到第一張。', en: 'The large number counts enabled maps. The line below counts all installed maps, including disabled ones. Until the count is known, the card shows “—”, not 0. Click to open the management page and jump to the first available update, if any.', ko: '큰 숫자는 활성화된 지도 수이고, 아래 줄은 비활성 지도를 포함한 전체 설치 수입니다. 읽기 결과가 나오기 전에는 0이 아닌 "—"가 표시됩니다. 카드를 누르면 관리 페이지가 열리고, 업데이트가 있으면 첫 지도로 이동합니다.', ja: '大きな数字は有効な地図の件数、下の行は無効なものも含めたインストール件数です。件数が分かるまでは 0 ではなく「—」が表示されます。カードを押すと管理ページが開き、更新があれば最初の項目に移動します。' },
}
