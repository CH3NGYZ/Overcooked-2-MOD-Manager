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
  'settings.gameRuntime.title': {
    'zh-CN': '游戏版本',
    'zh-TW': '遊戲版本',
    en: 'Game build',
    ko: '게임 버전',
    ja: 'ゲームの版',
  },
  'settings.gameRuntime.body': {
    'zh-CN': 'macOS 上先选跑哪一种：原生版配 macOS 框架，CrossOver 走 Windows 容器。切换后立即生效，不用重装。',
    'zh-TW': 'macOS 上先選跑哪一種：原生版搭配 macOS 框架，CrossOver 走 Windows 容器。切換後立即生效，不用重裝。',
    en: 'On macOS, pick the build first: native uses the macOS framework, CrossOver runs the Windows build in a container. Switching applies immediately — no reinstall.',
    ko: 'macOS에서는 먼저 실행 방식을 고릅니다. 네이티브는 macOS 프레임워크를 쓰고, CrossOver는 Windows 빌드를 컨테이너에서 실행합니다. 전환은 즉시 적용되며 다시 설치할 필요가 없습니다.',
    ja: 'macOS では先に実行方法を選びます。ネイティブは macOS 版フレームワーク、CrossOver は Windows 版をコンテナで動かします。切り替えはすぐ反映され、再インストールは不要です。',
  },
  'settings.gamePath.title': {
    'zh-CN': '游戏目录',
    'zh-TW': '遊戲目錄',
    en: 'Game folder',
    ko: '게임 폴더',
    ja: 'ゲームフォルダー',
  },
  'settings.gamePath.body': {
    'zh-CN':
      '管理器靠这一行找到游戏和 BepInEx。换盘或重装后路径失效，安装、启动都会失败：点「选择路径」重新指定。',
    'zh-TW':
      '管理器靠這一列找到遊戲與 BepInEx。換磁碟或重裝後路徑失效，安裝、啟動都會失敗：點「選擇路徑」重新指定。',
    en: 'This row is how the manager finds the game and BepInEx. Moving or reinstalling the game leaves the path stale and installs or launches fail: use “Choose path” to point it again.',
    ko: '이 줄로 게임과 BepInEx를 찾습니다. 드라이브를 옮기거나 다시 설치하면 경로가 어긋나 설치와 실행이 실패하니 “경로 선택”으로 다시 지정하세요.',
    ja: 'この行がゲームと BepInEx の場所です。ドライブ変更や再インストールでパスが無効になるとインストールも起動も失敗します。「パスを選択」で指定し直してください。',
  },
  'settings.switches.title': {
    'zh-CN': '通用开关',
    'zh-TW': '一般開關',
    en: 'General switches',
    ko: '일반 스위치',
    ja: '一般スイッチ',
  },
  'settings.switches.body': {
    'zh-CN':
      '长期生效的开关都在这里：删除是否先进回收站、离线时只用内置源。关掉回收站后删除不可恢复。',
    'zh-TW':
      '長期生效的開關都在這裡：刪除是否先進資源回收筒、離線時只用內建來源。關掉回收筒後刪除無法復原。',
    en: 'These switches stick: send deleted files to the recycle bin, use only bundled sources offline. With the recycle bin off, deletes are permanent.',
    ko: '한 번 정하면 계속 적용됩니다. 삭제 시 휴지통 사용, 오프라인에서 내장 소스만 사용. 휴지통을 끄면 삭제는 되돌릴 수 없습니다.',
    ja: '一度決めたら効き続けます。削除時にゴミ箱へ入れるか、オフラインで内蔵ソースのみ使うか。ゴミ箱を切ると削除は取り消せません。',
  },
  'settings.performance.title': {
    'zh-CN': '并发读取',
    'zh-TW': '並行讀取',
    en: 'Parallel reads',
    ko: '병렬 읽기',
    ja: '並列読み込み',
  },
  'settings.performance.body': {
    'zh-CN':
      '两个下拉只决定同时读几个文件：MOD 读 DLL、地图读 info。结果一样，卡就调小，想更快就保持「全部核心」。',
    'zh-TW':
      '兩個下拉只決定同時讀幾個檔案：MOD 讀 DLL、地圖讀 info。結果一樣，電腦卡就調小，想更快就保持「全部核心」。',
    en: 'The two dropdowns only set how many files are read at once: DLLs for MODs, info for maps. Results are identical — lower them if the machine stutters, keep “All cores” for faster scans.',
    ko: '두 드롭다운은 동시에 몇 개를 읽을지만 정합니다. MOD는 DLL, 지도는 info. 결과는 같으니 버벅이면 줄이고, 빠르게 하려면 “모든 코어”를 유지하세요.',
    ja: '2 つのドロップダウンは同時に読む数だけを決めます（MOD は DLL、マップは info）。結果は同じ。重いときは減らし、速くしたいなら「すべてのコア」のままに。',
  },
  'settings.bepinex.title': {
    'zh-CN': 'BepInEx 框架',
    'zh-TW': 'BepInEx 框架',
    en: 'BepInEx framework',
    ko: 'BepInEx 프레임워크',
    ja: 'BepInEx フレームワーク',
  },
  'settings.bepinex.body': {
    'zh-CN': '多数 MOD 的运行前提。这一行显示版本与启用状态，安装、备份、卸载都收在右侧抽屉里。',
    'zh-TW': '多數 MOD 的執行前提。這一列顯示版本與啟用狀態，安裝、備份、移除都收在右側抽屜裡。',
    en: 'Most MODs need it to run. This row shows version and state; install, backup and removal live in the right drawer.',
    ko: '대부분의 MOD가 실행되려면 필요합니다. 이 줄은 버전과 사용 상태를 보여 주고, 설치·백업·제거는 오른쪽 서랍에 있습니다.',
    ja: '多くの MOD の動作に必要です。この行はバージョンと有効状態を示し、導入・バックアップ・削除は右のドロワーにまとまっています。',
  },
  'settings.diylevel.title': {
    'zh-CN': 'DIYLevel 框架',
    'zh-TW': 'DIYLevel 框架',
    en: 'DIYLevel framework',
    ko: 'DIYLevel 프레임워크',
    ja: 'DIYLevel フレームワーク',
  },
  'settings.diylevel.body': {
    'zh-CN': '自制的关卡集靠它加载。它本身是个 MOD，所以管理方式和装 MOD 相同，点右侧按钮打开抽屉。',
    'zh-TW': '自製的關卡集靠它載入。它本身是個 MOD，所以管理方式和安裝 MOD 相同，點右側按鈕開啟抽屜。',
    en: 'Custom level sets load through it. It is itself a MOD, so managing it works like installing one — use the button on the right.',
    ko: '직접 만든 레벨 세트를 불러오는 데 씁니다. 그 자체가 MOD라서 관리 방식이 MOD 설치와 같으며, 오른쪽 버튼으로 서랍을 엽니다.',
    ja: '自作のレベルセットを読み込むためのものです。それ自体が MOD なので扱いは MOD の導入と同じで、右のボタンからドロワーを開きます。',
  },
  'settings.update.title': {
    'zh-CN': '自动检查更新',
    'zh-TW': '自動檢查更新',
    en: 'Update on startup',
    ko: '자동 업데이트 확인',
    ja: '更新の自動確認',
  },
  'settings.update.body': {
    'zh-CN':
      '只决定启动时要不要联网查新版本；关掉后仍可在最下方「关于」里手动检测。查到新版弹窗询问，不会静默安装。',
    'zh-TW':
      '只決定啟動時要不要連網檢查新版本；關掉後仍可在最下方「關於」手動檢查。查到新版本會彈窗詢問，不會靜默安裝。',
    en: 'Only controls whether a new version is looked up at startup; checking manually still works under About at the bottom. A found update asks first — never installed silently.',
    ko: '시작할 때 새 버전을 확인할지만 정합니다. 꺼도 맨 아래 “정보”에서 직접 확인할 수 있습니다. 새 버전은 물어보고 설치하며, 자동 설치하지 않습니다.',
    ja: '起動時に新しいバージョンを確認するかだけを決めます。オフでも最下部の「このアプリについて」から手動で確認できます。見つかっても確認してからで、勝手には入れません。',
  },
}
