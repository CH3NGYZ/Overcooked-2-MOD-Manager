import type { DomainMessages } from './types'

/** Sidebar destinations. Content instructions live in each page's own table. */
export const tutorialNavigation: DomainMessages = {
  'chrome.modsOnline.title': {
    'zh-CN': '侧栏：MOD 安装',
    'zh-TW': '側欄：MOD 安裝',
    en: 'Sidebar: MOD install',
    ko: '사이드바: MOD 설치',
    ja: 'サイドバー: MOD インストール',
  },
  'chrome.modsOnline.body': {
    'zh-CN': '接下来进入 MOD 安装：这里按仓库列出能下载的 MOD，选一个看详情就能装。点侧栏这一格即可进入。',
    'zh-TW': '接著進入 MOD 安裝：這裡依倉庫列出可下載的 MOD，選一個看詳情就能安裝。點側欄這一格即可進入。',
    en: 'Next is MOD install: it lists downloadable MODs by repository — open one to see details and install. Click this sidebar entry to go there.',
    ko: '다음은 MOD 설치입니다. 저장소별로 내려받을 MOD를 보여 주며, 하나를 골라 상세를 보고 설치합니다. 사이드바에서 이 항목을 클릭하세요.',
    ja: '次は MOD インストールです。リポジトリごとに入手できる MOD が並び、選ぶと詳細を見て導入できます。サイドバーのこの項目をクリックします。',
  },
  'chrome.modsLocal.title': {
    'zh-CN': '侧栏：MOD 管理',
    'zh-TW': '側欄：MOD 管理',
    en: 'Sidebar: MOD manager',
    ko: '사이드바: MOD 관리',
    ja: 'サイドバー: MOD 管理',
  },
  'chrome.modsLocal.body': {
    'zh-CN': '装好的 MOD 都在「MOD 管理」：开关、删除、依赖和配置都在这一页，不再联网。同样点侧栏这一格进入。',
    'zh-TW': '裝好的 MOD 都在「MOD 管理」：開關、刪除、依賴和設定都在這一頁，不再連網。同樣點側欄這一格進入。',
    en: 'Installed MODs live in MOD manager: toggles, deletion, dependencies and config, all offline. Use this sidebar entry again.',
    ko: '설치한 MOD는 모두 MOD 관리에 있습니다. 켜기/끄기, 삭제, 의존성, 설정이 한 페이지에 있고 네트워크는 쓰지 않습니다. 역시 사이드바에서 들어갑니다.',
    ja: '入れた MOD はすべて「MOD 管理」にあります。オン/オフ、削除、依存関係、設定がこの 1 ページで、通信も不要です。ここもサイドバーから開きます。',
  },
  'chrome.mapsOnline.title': {
    'zh-CN': '侧栏：地图安装',
    'zh-TW': '側欄：地圖安裝',
    en: 'Sidebar: Map install',
    ko: '사이드바: 지도 설치',
    ja: 'サイドバー: マップ導入',
  },
  'chrome.mapsOnline.body': {
    'zh-CN': '地图分成两页。「地图安装」是在线下载：从地图源拉整张关卡集，装完就能在游戏里选到。',
    'zh-TW': '地圖分成兩頁。「地圖安裝」是在線下載：從地圖源取得整張關卡集，裝完就能在遊戲裡選到。',
    en: 'Maps split into two pages. Map install downloads online: it pulls whole level sets from map sources, ready to pick in-game once installed.',
    ko: '지도는 두 페이지로 나뉩니다. 지도 설치는 온라인 다운로드로, 지도 소스에서 레벨 세트를 받아 설치하면 게임에서 고를 수 있습니다.',
    ja: 'マップは 2 ページに分かれます。「マップ導入」はオンライン入手で、マップソースからレベルセットを丸ごと取得し、導入後すぐゲーム内で選べます。',
  },
  'chrome.levels.title': {
    'zh-CN': '侧栏：地图管理',
    'zh-TW': '側欄：地圖管理',
    en: 'Sidebar: Map manager',
    ko: '사이드바: 지도 관리',
    ja: 'サイドバー: マップ管理',
  },
  'chrome.levels.body': {
    'zh-CN': '地图管理负责本地地图集的启停、删除与随包插件。接下来进入这一页；与 MOD 管理相同的卡片操作只做简要说明。',
    'zh-TW': '地圖管理負責本機地圖集的啟停、刪除與隨包外掛。接著進入這一頁；與 MOD 管理相同的卡片操作只做簡要說明。',
    en: 'Map manager handles local level sets: enabling, removal and bundled plugins. Next we will open it, with a brief overview of the card controls shared with MOD manager.',
    ko: '지도 관리는 로컬 레벨 세트의 활성 전환, 삭제와 동봉 플러그인을 다룹니다. 다음에 이 페이지를 열고 MOD 관리와 같은 카드 조작을 간단히 설명합니다.',
    ja: 'マップ管理ではローカルのセットの有効化・削除・同梱プラグインを扱います。次にこのページを開き、MOD 管理と共通のカード操作を簡単に説明します。',
  },
  'chrome.overview.title': {
    'zh-CN': '侧栏：总览', 'zh-TW': '側欄：總覽', en: 'Sidebar: Overview', ko: '사이드바: 개요', ja: 'サイドバー: 概要',
  },
  'chrome.overview.body': {
    'zh-CN': '总览汇总框架、MOD 和地图的本地状态与更新提醒。接下来先看这一页，了解安装和管理的入口。',
    'zh-TW': '總覽彙整框架、MOD 與地圖的本機狀態及更新提醒。接著先看這一頁，瞭解安裝與管理的入口。',
    en: 'Overview brings together local framework, MOD and map status, plus update reminders. We will start here to find the install and management shortcuts.',
    ko: '개요에는 프레임워크, MOD, 지도의 로컬 상태와 업데이트 알림이 모입니다. 먼저 이 페이지에서 설치와 관리 진입점을 살펴봅니다.',
    ja: '概要にはフレームワーク・MOD・マップのローカル状態と更新情報がまとまります。まずこのページで導入と管理の入口を確認します。',
  },
  'chrome.rename.title': {
    'zh-CN': '侧栏：改多彩名', 'zh-TW': '側欄：改多彩名', en: 'Sidebar: Colorful name', ko: '사이드바: 컬러 이름', ja: 'サイドバー: カラフル名',
  },
  'chrome.rename.body': {
    'zh-CN': '这里制作带颜色和不同字号的游戏名字，能预览、复制代码，也能推送给运行中的街机 MOD。下一步进入名字编辑器。',
    'zh-TW': '這裡製作帶顏色與不同字級的遊戲名字，可預覽、複製代碼，也能推送給執行中的街機 MOD。下一步進入名字編輯器。',
    en: 'Create a game name with colors and different text sizes. Preview it, copy its code, or send it to a running Arcade Mod. Next we will open the name editor.',
    ko: '색과 글자 크기가 다른 게임 이름을 만들 수 있습니다. 미리 보거나 코드를 복사하고 실행 중인 아케이드 MOD로 보낼 수도 있습니다. 다음은 이름 편집기입니다.',
    ja: '色や文字サイズを変えたゲーム名を作れます。プレビューやコードのコピー、実行中のアーケード MOD への送信もできます。次に名前の編集画面を開きます。',
  },
  'chrome.diychef.title': {
    'zh-CN': '侧栏：厨房暖暖', 'zh-TW': '側欄：廚房暖暖', en: 'Sidebar: Chef wardrobe', ko: '사이드바: 셰프 꾸미기', ja: 'サイドバー: シェフの着せ替え',
  },
  'chrome.diychef.body': {
    'zh-CN': '厨房暖暖管理 DIYChef 的厨师顺序、启用状态和帽子，也能设置默认开屏厨师。接下来用示例厨师演示，教程不会改动你的配置。',
    'zh-TW': '廚房暖暖管理 DIYChef 的廚師順序、啟用狀態與帽子，也能設定預設開場廚師。接著以範例廚師示範，教學不會更動你的設定。',
    en: 'The chef wardrobe manages DIYChef order, enabled chefs, hats and your opening chef. Next we will use sample chefs to demonstrate it without changing your configuration.',
    ko: '셰프 꾸미기에서는 DIYChef의 순서, 활성 상태, 모자와 시작 셰프를 정합니다. 다음 단계는 예시 셰프로 안내하며 실제 설정은 바꾸지 않습니다.',
    ja: 'シェフの着せ替えでは DIYChef の順番・有効状態・帽子・起動画面のシェフを設定できます。次はサンプルのシェフで説明し、実際の設定は変更しません。',
  },
  'chrome.tools.title': {
    'zh-CN': '侧栏：工具', 'zh-TW': '側欄：工具', en: 'Sidebar: Tools', ko: '사이드바: 도구', ja: 'サイドバー: ツール',
  },
  'chrome.tools.body': {
    'zh-CN': '工具页集中放日志、回收站和缓存等维护功能，也能重新播放教程。先认准底部这个入口，下一步再看各组用途。',
    'zh-TW': '工具頁集中放日誌、資源回收筒與快取等維護功能，也能重新播放教學。先認清底部這個入口，下一步再看各組用途。',
    en: 'Tools contains maintenance features such as logs, the recycle bin and caches, plus tutorial replay. This entry stays at the bottom; next we will look at its groups.',
    ko: '도구에는 로그, 휴지통, 캐시 등 유지 관리 기능과 튜토리얼 다시 재생이 있습니다. 하단의 이 진입점을 기억한 뒤 다음 단계에서 각 그룹을 살펴봅니다.',
    ja: 'ツールにはログ・ごみ箱・キャッシュなどの保守機能とチュートリアルの再生があります。この入口は下部に固定されています。次に各グループを見ていきます。',
  },
  'chrome.settings.title': {
    'zh-CN': '侧栏：设置', 'zh-TW': '側欄：設定', en: 'Sidebar: Settings', ko: '사이드바: 설정', ja: 'サイドバー: 設定',
  },
  'chrome.settings.body': {
    'zh-CN': '设置页管理游戏目录、框架、更新和并发选项，决定管理器如何找到游戏并工作。下一步逐项介绍，教程不会替你修改设置。',
    'zh-TW': '設定頁管理遊戲目錄、框架、更新與並行選項，決定管理器如何找到遊戲並運作。下一步逐項介紹，教學不會替你修改設定。',
    en: 'Settings controls the game folder, frameworks, updates and concurrency, defining how the manager finds and works with your game. Next we will explain the options without changing them.',
    ko: '설정은 게임 폴더, 프레임워크, 업데이트와 동시 처리 옵션을 관리하여 관리자가 게임을 찾고 동작하는 방식을 정합니다. 다음 단계에서 실제 설정을 바꾸지 않고 각 항목을 설명합니다.',
    ja: '設定ではゲームフォルダー・フレームワーク・更新・並列処理を管理し、マネージャーの動作を決めます。次は設定を変更せずに各項目を説明します。',
  },
}
