import type { DomainMessages } from './types'

/**
 * 内置教程：设置页（`tutorial.settings.*`）。
 *
 * 设置页是一排排「改一次、长期生效」的开关与路径，教程这一步只解释**这一项改什么、
 * 什么时候需要动**，不替用户点任何按钮 —— 尤其是「卸载框架」「提权重启」这类会被
 * 教程的副作用禁令挡掉的危险动作（见 `.tmp/tutorial-contract.md` 第 7 节）。所以正文
 * 里提到的按钮（选择路径）只做指路，真正操作留给用户。
 *
 * 语言表里写的是**域内 key**：`settings.gamePath.title` 经 `locale/index.ts` 的
 * `{ domain: 'tutorial', messages: tutorialSettings }` 拼成
 * `tutorial.settings.gamePath.title`。段名与步骤 id 对齐（`settings-game-path` →
 * `settings.gamePath.*`）。
 */
export const tutorialSettings: DomainMessages = {
  'settings.gameRuntime.title': { 'zh-CN': '游戏版本', 'zh-TW': '遊戲版本', en: 'Game build', ko: '게임 버전', ja: 'ゲームの版' },
  'settings.gameRuntime.body': { 'zh-CN': 'macOS 上先选跑哪一种：原生版配 macOS 框架，CrossOver 走 Windows 容器。切换后立即生效，不用重装。', 'zh-TW': 'macOS 上先選跑哪一種：原生版搭配 macOS 框架，CrossOver 走 Windows 容器。切換後立即生效，不用重裝。', en: 'On macOS, pick the build first: native uses the macOS framework, CrossOver runs the Windows build in a container. Switching applies immediately — no reinstall.', ko: 'macOS에서는 먼저 실행 방식을 고릅니다. 네이티브는 macOS 프레임워크를 쓰고, CrossOver는 Windows 빌드를 컨테이너에서 실행합니다. 전환은 즉시 적용되며 다시 설치할 필요가 없습니다.', ja: 'macOS では先に実行方法を選びます。ネイティブは macOS 版フレームワーク、CrossOver は Windows 版をコンテナで動かします。切り替えはすぐ反映され、再インストールは不要です。' },
  'settings.gamePath.title': { 'zh-CN': '游戏目录', 'zh-TW': '遊戲目錄', en: 'Game folder', ko: '게임 폴더', ja: 'ゲームフォルダー' },
  'settings.gamePath.body': { 'zh-CN': '管理器靠这一行找到游戏和 BepInEx。换盘或重装后路径失效，安装、启动都会失败：点「选择路径」重新指定。', 'zh-TW': '管理器靠這一列找到遊戲與 BepInEx。換磁碟或重裝後路徑失效，安裝、啟動都會失敗：點「選擇路徑」重新指定。', en: 'This row is how the manager finds the game and BepInEx. Moving or reinstalling the game leaves the path stale and installs or launches fail: use “Choose path” to point it again.', ko: '이 줄로 게임과 BepInEx를 찾습니다. 드라이브를 옮기거나 다시 설치하면 경로가 어긋나 설치와 실행이 실패하니 “경로 선택”으로 다시 지정하세요.', ja: 'この行がゲームと BepInEx の場所です。ドライブ変更や再インストールでパスが無効になるとインストールも起動も失敗します。「パスを選択」で指定し直してください。' },
  'settings.switches.title': { 'zh-CN': '通用设置', 'zh-TW': '一般設定', en: 'General settings', ko: '일반 설정', ja: '一般設定' },
  'settings.switches.body': { 'zh-CN': '这里决定删除的文件是否先放进回收站。建议保持开启，误删后还能恢复；关闭后，删除的文件无法恢复。', 'zh-TW': '這裡決定刪除的檔案是否先放進資源回收筒。建議保持開啟，誤刪後還能復原；關閉後，刪除的檔案無法復原。', en: 'Keep the recycle bin on if you want to restore accidentally deleted files. With it off, deleted files cannot be recovered.', ko: '실수로 삭제한 파일을 복원하려면 휴지통을 켜 두세요. 끄면 삭제한 파일을 복원할 수 없습니다.', ja: '誤って消したファイルを復元できるよう、ゴミ箱は有効のままがおすすめです。無効にすると削除したファイルは復元できません。' },
  'settings.dataSources.title': { 'zh-CN': '数据来源', 'zh-TW': '資料來源', en: 'Data sources', ko: '데이터 소스', ja: 'データソース' },
  'settings.dataSources.body': { 'zh-CN': '地区会保存，管理器每次启动时按地区自动选择 MOD 和地图的目录来源。连不上时可手动换源，或选择内置目录；手选只在本次运行有效。', 'zh-TW': '地區會儲存，管理器每次啟動時依地區自動選擇 MOD 和地圖的目錄來源。連不上時可手動換來源，或選擇內建目錄；手選僅在本次執行有效。', en: 'Your region is saved and used to pick a MOD and map catalog at startup. If a source is unreachable, switch sources or use the bundled catalog for this session.', ko: '지역을 저장하고 시작할 때 해당 지역에 맞는 MOD와 지도 목록을 자동으로 선택합니다. 연결이 안 되면 이번 실행에 한해 다른 소스나 내장 목록을 선택할 수 있습니다.', ja: '地域を保存し、起動時に地域に合った MOD とマップの一覧を選びます。接続できない場合は、今回の起動中だけ別の取得元や内蔵の一覧を選べます。' },
  'settings.download.title': { 'zh-CN': '下载设置', 'zh-TW': '下載設定', en: 'Download settings', ko: '다운로드 설정', ja: 'ダウンロード設定' },
  'settings.download.body': { 'zh-CN': '管理器会记住本机近期下载最快且通过校验的线路，必要时尝试备用线路。API 超时管查询请求，首字节超时管等待第一段下载数据。', 'zh-TW': '管理器會記住本機近期下載最快且通過驗證的線路，必要時嘗試備用線路。API 逾時適用於查詢請求，首位元組逾時限制等待第一段下載資料的時間。', en: 'The app remembers the fastest verified route on this device and tries backups when needed. API timeout limits lookups; first-byte timeout limits the wait for download data.', ko: '이 기기에서 최근 검증을 통과한 가장 빠른 경로를 기억하고 필요하면 예비 경로를 시도합니다. API 제한 시간은 조회에, 첫 바이트 제한 시간은 다운로드 데이터 수신 시작까지 적용됩니다.', ja: 'この端末で最近検証に通った最速の経路を記憶し、必要に応じて予備経路を試します。API タイムアウトは照会、初回データのタイムアウトはダウンロードの受信開始までに適用します。' },
  'settings.performance.title': { 'zh-CN': '性能设置', 'zh-TW': '效能設定', en: 'Performance settings', ko: '성능 설정', ja: 'パフォーマンス設定' },
  'settings.performance.body': { 'zh-CN': '分别设置 MOD 读取、地图信息读取、地图资源转换和暖暖缩略图生成的并行数量。默认用全部核心；如果电脑卡顿，可以调低对应项。', 'zh-TW': '分別設定 MOD 讀取、地圖資訊讀取、地圖資源轉換和暖暖縮圖產生的並行數量。預設使用全部核心；如果電腦卡頓，可以調低對應項目。', en: 'Set parallelism for MOD scanning, map scanning, map asset conversion and outfit thumbnails. All cores are used by default; lower the relevant setting if your computer stutters.', ko: 'MOD 정보 읽기, 지도 정보 읽기, 지도 리소스 변환, 꾸미기 썸네일 생성의 동시 작업 수를 따로 정합니다. 기본값은 모든 코어이며, 버벅이면 해당 항목을 낮추세요.', ja: 'MOD とマップ情報の読み込み、マップ素材の変換、着せ替えサムネイル生成の並列数を個別に設定します。既定はすべてのコアです。動作が重いときは該当する項目を下げてください。' },
  'settings.bepinex.title': { 'zh-CN': 'BepInEx 框架', 'zh-TW': 'BepInEx 框架', en: 'BepInEx framework', ko: 'BepInEx 프레임워크', ja: 'BepInEx フレームワーク' },
  'settings.bepinex.body': { 'zh-CN': '多数 MOD 的运行前提。这一行显示版本与启用状态，安装、备份、卸载都收在右侧抽屉里。', 'zh-TW': '多數 MOD 的執行前提。這一列顯示版本與啟用狀態，安裝、備份、移除都收在右側抽屜裡。', en: 'Most MODs need it to run. This row shows version and state; install, backup and removal live in the right drawer.', ko: '대부분의 MOD가 실행되려면 필요합니다. 이 줄은 버전과 사용 상태를 보여 주고, 설치·백업·제거는 오른쪽 서랍에 있습니다.', ja: '多くの MOD の動作に必要です。この行はバージョンと有効状態を示し、導入・バックアップ・削除は右のドロワーにまとまっています。' },
  'settings.diylevel.title': { 'zh-CN': 'DIYLevel 框架', 'zh-TW': 'DIYLevel 框架', en: 'DIYLevel framework', ko: 'DIYLevel 프레임워크', ja: 'DIYLevel フレームワーク' },
  'settings.diylevel.body': { 'zh-CN': '自制的关卡集靠它加载。它本身是个 MOD，所以管理方式和装 MOD 相同，点右侧按钮打开抽屉。', 'zh-TW': '自製的關卡集靠它載入。它本身是個 MOD，所以管理方式和安裝 MOD 相同，點右側按鈕開啟抽屜。', en: 'Custom level sets load through it. It is itself a MOD, so managing it works like installing one — use the button on the right.', ko: '직접 만든 레벨 세트를 불러오는 데 씁니다. 그 자체가 MOD라서 관리 방식이 MOD 설치와 같으며, 오른쪽 버튼으로 서랍을 엽니다.', ja: '自作のレベルセットを読み込むためのものです。それ自体が MOD なので扱いは MOD の導入と同じで、右のボタンからドロワーを開きます。' },
}
