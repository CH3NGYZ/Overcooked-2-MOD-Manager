import type { DomainMessages } from './types'

/**
 * 内置教程：骨架与标题栏（`tutorial.intro.*` / `tutorial.chrome.*` / 卡片按钮）。
 *
 * 其余几段（总览 / MOD / 工具与设置 + 二维码）在
 * `tutorialOverview.ts`、`tutorialMods.ts`、`tutorialTools.ts`、
 * `tutorialSettings.ts`，都挂在同一个 `tutorial` 域下。分文件是因为每一段由
 * 一个页面区域负责，改一屏文案只碰一个文件。
 *
 * 文案写法约定：标题是一句「这块地方叫什么」，正文讲「它是干什么的、
 * 点下去会发生什么」。不复述按钮上已经写着的字。
 */
export const tutorialChrome: DomainMessages = {
  'intro.title': {
    'zh-CN': '欢迎使用 OC2 MOD 管理器',
    'zh-TW': '歡迎使用 OC2 MOD 管理器',
    en: 'Welcome to the OC2 MOD Manager',
    ko: 'OC2 MOD 관리자에 오신 것을 환영합니다',
    ja: 'OC2 MOD マネージャーへようこそ',
  },
  'intro.body': {
    'zh-CN': '这是内置教程，会按界面顺序指出标题栏、侧栏、总览、MOD、地图、工具和设置里的关键区域。随时可以跳过；想再看一遍，去「工具 → 高级」里点「重新播放教程」。',
    'zh-TW': '這是內建教學，會依介面順序指出標題列、側邊欄、總覽、MOD、地圖、工具與設定裡的關鍵區域。隨時可以跳過；想再看一次，到「工具 → 進階」點「重新播放教學」。',
    en: 'This built-in tour walks through the key areas in order: title bar, sidebar, overview, MODs, maps, tools and settings. You can skip it at any time; to watch it again, open Tools → Advanced and choose “Replay tutorial”.',
    ko: '이 내장 투어는 제목 표시줄, 사이드바, 개요, MOD, 지도, 도구, 설정의 주요 영역을 순서대로 안내합니다. 언제든지 건너뛸 수 있으며, 다시 보려면 도구 → 고급에서 “튜토리얼 다시 재생”을 선택하세요.',
    ja: 'これは内蔵ツアーです。タイトルバー、サイドバー、概要、MOD、マップ、ツール、設定の要点を画面の順に案内します。いつでもスキップできます。もう一度見るには「ツール → 詳細設定」の「チュートリアルを再度再生」を選んでください。',
  },
  'chrome.sidebar.title': {
    'zh-CN': '侧栏：按用途分组',
    'zh-TW': '側邊欄：依用途分組',
    en: 'Sidebar: grouped by purpose',
    ko: '사이드바: 용도별 그룹',
    ja: 'サイドバー: 用途ごとのグループ',
  },
  'chrome.sidebar.body': {
    'zh-CN': '总览、MOD、地图各成一组，分割线以上是内容页面，往下是多彩名；「工具」和「设置」固定在底部。MOD 与地图都分成「下载」和「管理」两页：下载页要联网拉 Release，管理页只看本地磁盘。',
    'zh-TW': '總覽、MOD、地圖各成一組，分隔線以上是內容頁面，往下是多彩名；「工具」與「設定」固定在底部。MOD 與地圖都分成「下載」和「管理」兩頁：下載頁要連網抓 Release，管理頁只看本機磁碟。',
    en: 'Overview, MODs and maps form their own groups; below the divider comes the colorful-name tool, with Tools and Settings pinned to the bottom. Both MODs and maps split into a Download page (fetches Releases online) and a Manage page (local disk only).',
    ko: '개요, MOD, 지도가 각각 한 그룹이고 구분선 아래는 컬러 이름 도구이며, 도구와 설정은 하단에 고정됩니다. MOD와 지도는 모두 온라인에서 Release를 가져오는 다운로드 페이지와 로컬 디스크만 보는 관리 페이지로 나뉩니다.',
    ja: '概要・MOD・マップがそれぞれ 1 グループで、区切り線の下はカラフル名ツール、ツールと設定は下部に固定されています。MOD もマップも「ダウンロード」(Release を取得) と「管理」(ローカルのみ) の 2 ページに分かれています。',
  },
  'chrome.game.title': {
    'zh-CN': '启动 / 停止游戏',
    'zh-TW': '啟動 / 停止遊戲',
    en: 'Start / stop the game',
    ko: '게임 시작 / 중지',
    ja: 'ゲームの起動 / 停止',
  },
  'chrome.game.body': {
    'zh-CN': '点这颗圆形按钮直接启动游戏；游戏运行时它会变成停止样式，再点一次会让游戏正常退出。装好或关掉 MOD 之后，改动要等游戏下一次启动才生效。',
    'zh-TW': '點這顆圓形按鈕直接啟動遊戲；遊戲執行時它會變成停止樣式，再點一次會讓遊戲正常結束。安裝或關閉 MOD 之後，改動要等遊戲下次啟動才生效。',
    en: 'This round button launches the game directly; while the game is running it turns into a stop button, and clicking it shuts the game down cleanly. Installing or disabling MODs only takes effect the next time the game starts.',
    ko: '이 원형 버튼으로 게임을 바로 시작할 수 있습니다. 게임이 실행 중이면 중지 버튼으로 바뀌고, 다시 누르면 게임이 정상 종료됩니다. MOD를 설치하거나 끈 변경 사항은 다음 게임 시작 때 적용됩니다.',
    ja: 'この丸いボタンでゲームを直接起動できます。実行中は停止ボタンに変わり、もう一度押すとゲームが正常終了します。MOD のインストールや無効化は、次回ゲーム起動時に反映されます。',
  },
  'chrome.zoom.title': {
    'zh-CN': '界面缩放',
    'zh-TW': '介面縮放',
    en: 'Interface zoom',
    ko: '인터페이스 확대/축소',
    ja: '画面のズーム',
  },
  'chrome.zoom.body': {
    'zh-CN': '100% 到 200% 五档，缩放的是整个管理器的界面。窗口小、字看不清时调大，想一屏看到更多卡片时调小。它只影响管理器，不影响游戏本身。',
    'zh-TW': '100% 到 200% 五檔，縮放的是整個管理器的介面。視窗小、字看不清時調大，想一屏看到更多卡片時調小。它只影響管理器，不影響遊戲本身。',
    en: 'Five steps from 100% to 200%, scaling the whole manager interface. Raise it when the window is small or text is hard to read, lower it to fit more cards on screen. It only affects the manager, not the game.',
    ko: '100%에서 200%까지 다섯 단계로 관리자 인터페이스 전체를 확대/축소합니다. 창이 작거나 글씨가 잘 안 보이면 키우고, 한 화면에 더 많은 카드를 보고 싶으면 줄이세요. 게임 자체에는 영향을 주지 않습니다.',
    ja: '100%〜200% の 5 段階でマネージャーの UI 全体を拡大縮小します。ウィンドウが小さいときや文字が読みにくいときは大きく、より多くのカードを表示したいときは小さくします。ゲーム本体には影響しません。',
  },
  'chrome.language.title': {
    'zh-CN': '界面语言',
    'zh-TW': '介面語言',
    en: 'Interface language',
    ko: '인터페이스 언어',
    ja: '表示言語',
  },
  'chrome.language.body': {
    'zh-CN': '简体中文、繁體中文、English、한국어、日本語 五选一。仓库与地图自带的多种语言名称、说明也会跟着一起切，不用重启。',
    'zh-TW': '簡體中文、繁體中文、English、한국어、日本語 五選一。倉庫與地圖內建的多語名稱、說明也會跟著切換，不需重啟。',
    en: 'Choose from Simplified Chinese, Traditional Chinese, English, Korean and Japanese. Multi-language names and notes shipped by repositories and maps switch along with it — no restart needed.',
    ko: '중국어 간체, 중국어 번체, 영어, 한국어, 일본어 중에서 고릅니다. 저장소와 지도에 포함된 다국어 이름과 설명도 함께 바뀌며 재시작은 필요 없습니다.',
    ja: '簡体字中国語・繁体字中国語・英語・韓国語・日本語から選べます。リポジトリやマップが持つ多言語の名前・説明も一緒に切り替わり、再起動は不要です。',
  },
  'chrome.themeMode.title': {
    'zh-CN': '明暗模式',
    'zh-TW': '明暗模式',
    en: 'Light / dark mode',
    ko: '라이트 / 다크 모드',
    ja: 'ライト / ダークモード',
  },
  'chrome.themeMode.body': {
    'zh-CN': '在深色、浅色、跟随系统之间轮换，按钮上的图标就是当前模式。选「跟随系统」时会随 Windows / macOS 的外观设置自动切换。',
    'zh-TW': '在深色、淺色、跟隨系統之間輪換，按鈕上的圖示就是目前模式。選「跟隨系統」時會隨 Windows / macOS 的外觀設定自動切換。',
    en: 'Cycles through dark, light and follow-system; the icon shows the current mode. “Follow system” tracks the Windows / macOS appearance setting automatically.',
    ko: '다크, 라이트, 시스템 설정 따르기 사이를 순환하며 버튼 아이콘이 현재 모드를 보여 줍니다. 시스템 설정을 따를 때는 Windows / macOS 모양 설정에 따라 자동으로 바뀝니다.',
    ja: 'ダーク・ライト・システムに従う を順に切り替えます。ボタンのアイコンが現在のモードです。「システムに従う」では Windows / macOS の外観設定に自動追従します。',
  },
  'chrome.accent.title': {
    'zh-CN': '主题色',
    'zh-TW': '主題色',
    en: 'Accent color',
    ko: '강조 색상',
    ja: 'アクセントカラー',
  },
  'chrome.accent.body': {
    'zh-CN': '左键打开色板挑一个强调色，右键直接跳到下一个。按钮、开关、标签、图标块的配色都跟着它走，和明暗模式互不影响。',
    'zh-TW': '左鍵開啟色盤挑一個強調色，右鍵直接跳到下一個。按鈕、開關、標籤、圖示區塊的配色都跟著它走，和明暗模式互不影響。',
    en: 'Left-click opens the palette; right-click jumps to the next color. Buttons, switches, tags and icon tiles all follow it, independently of the light / dark mode.',
    ko: '왼쪽 클릭으로 색상 팔레트를 열고, 오른쪽 클릭으로 다음 색상으로 넘어갑니다. 버튼, 스위치, 태그, 아이콘 타일 색상이 함께 바뀌며 라이트/다크 모드와는 별개입니다.',
    ja: '左クリックでパレットを開き、右クリックで次の色へ進みます。ボタン・スイッチ・タグ・アイコンタイルの配色が連動し、ライト/ダークモードとは独立しています。',
  },
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
    'zh-CN': '「地图管理」管本地已有的地图：开关、删除、看随包插件。版式和 MOD 管理一样，这里不再赘述。',
    'zh-TW': '「地圖管理」管本機既有的地圖：開關、刪除、查看隨包外掛。版面和 MOD 管理一樣，這裡不再贅述。',
    en: 'Map manager handles maps already on disk: toggles, deletion, bundled plugins. The layout matches MOD manager, so we skip the details.',
    ko: '지도 관리는 이미 있는 지도를 다룹니다. 켜기/끄기, 삭제, 함께 들어온 플러그인을 볼 수 있고, 구성은 MOD 관리와 같아 자세히 설명하지 않습니다.',
    ja: '「マップ管理」は手元にあるマップを扱います。オン/オフ、削除、同梱プラグインの確認ができ、レイアウトは MOD 管理と同じなので詳しくは触れません。',
  },
  next: {
    'zh-CN': '下一步',
    'zh-TW': '下一步',
    en: 'Next',
    ko: '다음',
    ja: '次へ',
  },
  prev: {
    'zh-CN': '上一步',
    'zh-TW': '上一步',
    en: 'Back',
    ko: '이전',
    ja: '戻る',
  },
  skip: {
    'zh-CN': '跳过教程',
    'zh-TW': '跳過教學',
    en: 'Skip tutorial',
    ko: '튜토리얼 건너뛰기',
    ja: 'チュートリアルをスキップ',
  },
  done: {
    'zh-CN': '完成',
    'zh-TW': '完成',
    en: 'Done',
    ko: '완료',
    ja: '完了',
  },
  hint: {
    'zh-CN': '点击任意位置继续 · 滚轮或 ← ↑ ↓ → 方向键切换',
    'zh-TW': '點擊任意位置繼續 · 滾輪或 ← ↑ ↓ → 方向鍵切換',
    en: 'Click anywhere to continue · wheel or arrow keys to move',
    ko: '아무 곳이나 클릭해 계속 · 휠 또는 방향키로 이동',
    ja: 'どこかをクリックで続行 · ホイールまたは矢印キーで移動',
  },
  replay: {
    'zh-CN': '重新播放教程',
    'zh-TW': '重新播放教學',
    en: 'Replay tutorial',
    ko: '튜토리얼 다시 재생',
    ja: 'チュートリアルを再度再生',
  },
  replayDescription: {
    'zh-CN': '从标题栏开始，按界面顺序把整份引导再走一遍。',
    'zh-TW': '從標題列開始，依介面順序把整份引導再走一遍。',
    en: 'Walk through the whole guide again in interface order, starting from the title bar.',
    ko: '제목 표시줄부터 시작해 인터페이스 순서대로 전체 안내를 다시 살펴봅니다.',
    ja: 'タイトルバーから始め、画面の順序どおりにガイド全体をもう一度たどります。',
  },
  /*
   * 教程示例数据（`src/tutorial/demo.ts`）。
   *
   * 新用户磁盘上没有 MOD，「MOD 管理」页连一张卡都不渲染，教程那一步就
   * 无从框选；教程期间前端补两张带「示例」字样的卡片。名字必须一眼看出
   * 不是自己的东西，所以都带「示例 / Sample」。
   */
  'demo.modA': {
    'zh-CN': '示例 MOD · 自动上菜助手',
    'zh-TW': '範例 MOD · 自動上菜助手',
    en: 'Sample mod · Auto-serve helper',
    ko: '예시 MOD · 자동 서빙 도우미',
    ja: 'サンプル MOD・自動配膳ヘルパー',
  },
  'demo.modB': {
    'zh-CN': '示例 MOD · 连击计时器',
    'zh-TW': '範例 MOD · 連擊計時器',
    en: 'Sample mod · Combo timer',
    ko: '예시 MOD · 콤보 타이머',
    ja: 'サンプル MOD・コンボタイマー',
  },
  'demo.author': {
    'zh-CN': '教程示例',
    'zh-TW': '教學範例',
    en: 'Tutorial sample',
    ko: '튜토리얼 예시',
    ja: 'チュートリアルのサンプル',
  },
}
