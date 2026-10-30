import type { DomainMessages } from './types'

/**
 * 游戏目录、运行状态和 Steam 启动。
 *
 * 对应界面：总览页的厨房卡片、设置页的目录操作、以及所有「需要先关游戏」
 * 的拦截提示。
 *
 * 这些文案大多来自后端的 `game.*` tag：判定逻辑（进程是否存在、目录是否
 * 被用户改过）在 Rust，前端只负责翻译，不重复判断。
 */
export const game: DomainMessages = {
  notFound: {
    'zh-CN': '尚未找到厨房目录',
    'zh-TW': '尚未找到廚房目錄',
    en: 'Game directory not found yet',
    ko: '아직 주방 디렉터리를 찾지 못했습니다',
    ja: 'キッチンディレクトリが見つかりません',
  },
  directoryMissing: {
    'zh-CN': '尚未找到游戏目录',
    'zh-TW': '尚未找到遊戲目錄',
    en: 'Game directory not found yet',
    ko: '아직 게임 디렉터리를 찾지 못했습니다',
    ja: 'ゲームディレクトリが見つかりません',
  },
  directoryChanged: {
    'zh-CN': '厨房目录已变化，请重新添加模组',
    'zh-TW': '廚房目錄已變更，請重新新增模組',
    en: 'The game directory changed. Add your mods again.',
    ko: '주방 디렉터리가 변경되었습니다. 모드를 다시 추가하세요.',
    ja: 'キッチンディレクトリが変更されました。MOD を追加し直してください。',
  },
  directoryChangedReopenConfig: {
    'zh-CN': '厨房目录已变化，请重新打开配置',
    'zh-TW': '廚房目錄已變更，請重新開啟設定',
    en: 'The game directory changed. Reopen the settings.',
    ko: '주방 디렉터리가 변경되었습니다. 설정을 다시 여세요.',
    ja: 'キッチンディレクトリが変更されました。設定を開き直してください。',
  },
  invalidDirectory: {
    'zh-CN': '所选厨房目录无效',
    'zh-TW': '所選廚房目錄無效',
    en: 'The selected game directory is invalid',
    ko: '선택한 주방 디렉터리가 올바르지 않습니다',
    ja: '選択したキッチンディレクトリが無効です',
  },
  exeNotFound: {
    'zh-CN': '所选目录中没有 Overcooked2.exe',
    'zh-TW': '所選目錄中沒有 Overcooked2.exe',
    en: 'Overcooked2.exe was not found in the selected directory',
    ko: '선택한 디렉터리에 Overcooked2.exe가 없습니다',
    ja: '選択したディレクトリに Overcooked2.exe がありません',
  },
  appNotFound: {
    'zh-CN': '所选目录中没有 Overcooked2.app',
    'zh-TW': '所選目錄中沒有 Overcooked2.app',
    en: 'Overcooked2.app was not found in the selected directory',
    ko: '선택한 디렉터리에 Overcooked2.app이 없습니다',
    ja: '選択したディレクトリに Overcooked2.app がありません',
  },
  executableMissing: {
    'zh-CN': '当前模式未找到厨房可执行文件',
    'zh-TW': '目前模式找不到廚房執行檔',
    en: 'No game executable found for the current mode',
    ko: '현재 모드에서 주방 실행 파일을 찾지 못했습니다',
    ja: '現在のモードではキッチンの実行ファイルが見つかりません',
  },
  relativePathRequired: {
    'zh-CN': '路径必须是游戏目录内的相对路径',
    'zh-TW': '路徑必須是遊戲目錄內的相對路徑',
    en: 'The path must be relative to the game directory',
    ko: '경로는 게임 디렉터리 기준 상대 경로여야 합니다',
    ja: 'パスはゲームディレクトリ内の相対パスである必要があります',
  },
  running: {
    'zh-CN': '《胡闹厨房2》正在运行，请先关闭后重试',
    'zh-TW': '《胡鬧廚房2》正在執行，請先關閉後重試',
    en: 'Overcooked! 2 is running. Close it and try again.',
    ko: '《Overcooked! 2》가 실행 중입니다. 종료 후 다시 시도하세요.',
    ja: '『Overcooked! 2』が実行中です。終了してから再試行してください。',
  },
  notRunning: {
    'zh-CN': '厨房当前未在运行',
    'zh-TW': '廚房目前未在執行',
    en: 'The game is not running',
    ko: '주방이 현재 실행 중이 아닙니다',
    ja: 'キッチンは現在実行されていません',
  },
  stillRunning: {
    'zh-CN': '游戏进程仍在运行，请手动关闭',
    'zh-TW': '遊戲程序仍在執行，請手動關閉',
    en: 'The game process is still running. Close it manually.',
    ko: '게임 프로세스가 아직 실행 중입니다. 직접 종료하세요.',
    ja: 'ゲームプロセスがまだ実行中です。手動で終了してください。',
  },
  stopped: {
    'zh-CN': '已停止厨房',
    'zh-TW': '已停止廚房',
    en: 'Game stopped',
    ko: '주방을 중지했습니다',
    ja: 'キッチンを停止しました',
  },
  steamLaunchRequested: {
    'zh-CN': '已请求 Steam 启动厨房，请在 Steam 中确认启动',
    'zh-TW': '已請求 Steam 啟動廚房，請在 Steam 中確認啟動',
    en: 'Steam launch requested. Confirm the launch in Steam.',
    ko: 'Steam 실행을 요청했습니다. Steam에서 실행을 확인하세요.',
    ja: 'Steam に起動を要求しました。Steam で起動を確認してください。',
  },
  settingsChanged: {
    'zh-CN': '厨房设置已变化，请重试',
    'zh-TW': '廚房設定已變更，請重試',
    en: 'Game settings changed. Try again.',
    ko: '주방 설정이 변경되었습니다. 다시 시도하세요.',
    ja: 'キッチンの設定が変更されました。再試行してください。',
  },
  settingsNotInitialized: {
    'zh-CN': '游戏设置尚未初始化',
    'zh-TW': '遊戲設定尚未初始化',
    en: 'Game settings are not initialized yet',
    ko: '게임 설정이 아직 초기화되지 않았습니다',
    ja: 'ゲーム設定がまだ初期化されていません',
  },
  stateNotInitialized: {
    'zh-CN': '游戏状态尚未初始化',
    'zh-TW': '遊戲狀態尚未初始化',
    en: 'Game state is not initialized yet',
    ko: '게임 상태가 아직 초기화되지 않았습니다',
    ja: 'ゲームの状態がまだ初期化されていません',
  },
  unsupportedPlatform: {
    'zh-CN': '当前平台不支持启动厨房',
    'zh-TW': '目前平台不支援啟動廚房',
    en: 'Launching the game is not supported on this platform',
    ko: '현재 플랫폼에서는 주방 실행을 지원하지 않습니다',
    ja: 'このプラットフォームではキッチンの起動に対応していません',
  },
  windowsOnly: {
    'zh-CN': 'Windows 仅支持 Windows 版厨房',
    'zh-TW': 'Windows 僅支援 Windows 版廚房',
    en: 'Windows only supports the Windows build of the game',
    ko: 'Windows에서는 Windows 버전 주방만 지원합니다',
    ja: 'Windows では Windows 版のキッチンのみ対応しています',
  },
  started: {
    'zh-CN': '已启动厨房',
    'zh-TW': '已啟動廚房',
    en: 'Game started',
    ko: '주방을 실행했습니다',
    ja: 'キッチンを起動しました',
  },
}
