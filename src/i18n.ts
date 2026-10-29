import { computed, ref, watch } from 'vue'
import { dateEnUS, dateJaJP, dateKoKR, dateZhCN, dateZhTW, enUS, jaJP, koKR, zhCN, zhTW } from 'naive-ui'

/**
 * 多语言：以**固定中文标签**作 key，值为五语元组 [zh-TW, en, ko, ja]（zh-CN 即 key 本身）。
 * `t(key)` 的 key 是编译期联合类型 —— 传错或漏项 `vue-tsc` 会报错。
 * Naive UI 的组件文案通过 locale/dateLocale 同步切换。
 */
export type Locale = 'zh-CN' | 'zh-TW' | 'en' | 'ko' | 'ja'

type Entry = readonly [string, string, string, string]

const LOCALE_KEY = 'oc2-locale'
const LOCALES: Locale[] = ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']

export const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'ko', label: '한국어' },
  { value: 'ja', label: '日本語' },
]

export const locale = ref<Locale>(
  LOCALES.includes(localStorage.getItem(LOCALE_KEY) as Locale) ? (localStorage.getItem(LOCALE_KEY) as Locale) : 'zh-CN',
)
watch(locale, (value) => {
  localStorage.setItem(LOCALE_KEY, value)
  document.documentElement.lang = value
})

/** 受支持的界面语言（供安装语言的运行期校验）。 */
export const SUPPORTED_LOCALES: readonly Locale[] = LOCALES

/**
 * 是否已有用户选择过的语言。
 *
 * 「有 localStorage」= 用户在界面里选过（watch 会写）；没有则说明是首次启动，
 * 可以采纳安装程序写入的语言。
 */
export function hasStoredLocale(): boolean {
  return LOCALES.includes(localStorage.getItem(LOCALE_KEY) as Locale)
}

/**
 * 应用安装程序选择的语言（仅首次启动时调用）。
 *
 * 独立成函数而不是写成模块顶层副作用：`i18n.ts` 被几乎所有组件引用，
 * 顶层发起 IPC 会让测试与纯逻辑模块也被迫依赖 Tauri。由 `main.ts` 在挂载前调用。
 *
 * 无论是否命中，首次启动都会把当前语言落盘一次：这样「用户还没选过」这个状态
 * 在启动后就不再成立，安装语言只影响第一次启动。
 */
export function applyInstallLanguage(value: string | null | undefined): boolean {
  if (hasStoredLocale()) return false
  const applied = !!value && LOCALES.includes(value as Locale)
  if (applied) {
    // 赋值触发 watch → 写入 localStorage
    locale.value = value as Locale
  } else {
    // 没变化时 watch 不会触发，这里显式落盘默认语言
    localStorage.setItem(LOCALE_KEY, locale.value)
  }
  return applied
}

/** 五语表：key = 简体中文原文。新增文案在这里加一行即可。 */
const entries = {
  // 侧栏与标题栏
  '总览': ['總覽', 'Overview', '개요', '概要'],
  '框架管理': ['框架管理', 'Framework', '프레임워크', 'フレームワーク'],
  'MOD 安装': ['MOD 安裝', 'MOD Install', 'MOD 설치', 'MOD インストール'],
  'MOD 管理': ['MOD 管理', 'MOD Manager', 'MOD 관리', 'MOD マネージャー'],
  '工具': ['工具', 'Tools', '도구', 'ツール'],
  '设置': ['設定', 'Settings', '설정', '設定'],
  '快捷管理 《胡闹厨房2》的MOD': ['快捷管理《胡鬧廚房2》的MOD', 'Quickly manage MODs for Overcooked! 2', '《오버쿡드! 2》 MOD를 빠르게 관리', '『オーバークック2』のMODをすばやく管理'],
  'OC2 模组管理器': ['OC2 模組管理器', 'OC2 Mod Manager', 'OC2 모드 관리자', 'OC2 MODマネージャー'],
  '界面语言': ['介面語言', 'Language', '인터페이스 언어', '表示言語'],
  '界面缩放': ['介面縮放', 'UI scale', '인터페이스 배율', '表示倍率'],
  '主题模式': ['主題模式', 'Theme', '테마', 'テーマ'],
  '深色': ['深色', 'Dark', '다크', 'ダーク'],
  '浅色': ['淺色', 'Light', '라이트', 'ライト'],
  '主题色': ['主題色', 'Accent', '테마 색', 'テーマカラー'],
  '中性': ['中性', 'Neutral', '뉴트럴', 'ニュートラル'],
  '板岩': ['板岩', 'Slate', '슬레이트', 'スレート'],
  '灰色': ['灰色', 'Gray', '그레이', 'グレー'],
  '玫红': ['玫紅', 'Rose', '로즈', 'ローズ'],
  '蓝色': ['藍色', 'Blue', '블루', 'ブルー'],
  '绿色': ['綠色', 'Green', '그린', 'グリーン'],
  '紫色': ['紫色', 'Purple', '퍼플', 'パープル'],
  '红色': ['紅色', 'Red', '레드', 'レッド'],
  '橙色': ['橙色', 'Orange', '오렌지', 'オレンジ'],
  '琥珀': ['琥珀', 'Amber', '앰버', 'アンバー'],
  '青色': ['青色', 'Teal', '틸', 'ティール'],
  '最小化': ['最小化', 'Minimize', '최소화', '最小化'],
  '最大化': ['最大化', 'Maximize', '최대화', '最大化'],
  '还原': ['還原', 'Restore', '복원', '元に戻す'],
  '关闭': ['關閉', 'Close', '닫기', '閉じる'],

  // 总览
  '已安装 MOD': ['已安裝 MOD', 'Installed MODs', '설치된 MOD', 'インストール済みMOD'],
  '框架状态': ['框架狀態', 'Framework', '프레임워크 상태', 'フレームワーク状態'],
  '游戏路径': ['遊戲路徑', 'Game path', '게임 경로', 'ゲームパス'],
  '待处理事项': ['待處理事項', 'Pending', '처리 대기', '保留項目'],
  '已启用 {count} 个': ['已啟用 {count} 個', '{count} enabled', '{count}개 활성', '{count} 個有効'],
  '正常': ['正常', 'Ready', '정상', '正常'],
  '未安装': ['未安裝', 'Not installed', '설치 안 됨', '未インストール'],
  '已验证': ['已驗證', 'Verified', '검증됨', '検証済み'],
  '目录异常': ['目錄異常', 'Path invalid', '경로 오류', 'パス異常'],
  '未设置': ['未設定', 'Not set', '설정 안 됨', '未設定'],
  '无法探测游戏目录': ['無法偵測遊戲目錄', 'Cannot detect game directory', '게임 폴더를 감지할 수 없습니다', 'ゲームフォルダーを検出できません'],
  '暂无可更新内容': ['暫無可更新內容', 'Nothing to update', '업데이트할 항목 없음', '更新はありません'],
  '有可更新的 MOD': ['有可更新的 MOD', 'MOD updates available', '업데이트 가능한 MOD 있음', '更新可能なMODがあります'],

  // 框架
  'BepInEx 框架': ['BepInEx 框架', 'BepInEx framework', 'BepInEx 프레임워크', 'BepInEx フレームワーク'],
  'BepInEx 框架管理': ['BepInEx 框架管理', 'BepInEx framework', 'BepInEx 프레임워크 관리', 'BepInEx フレームワーク管理'],
  'BepInEx 配置': ['BepInEx 設定', 'BepInEx config', 'BepInEx 설정', 'BepInEx 設定'],
  '配置管理': ['設定管理', 'Manage config', '설정 관리', '設定を管理'],
  '运行形态': ['執行形態', 'Runtime', '실행 형태', '実行形態'],
  '原生': ['原生', 'Native', '네이티브', 'ネイティブ'],
  '游戏版本': ['遊戲版本', 'Game version', '게임 버전', 'ゲームのバージョン'],
  'CrossOver 运行 Windows 版，或使用 macOS 原生版': ['CrossOver 執行 Windows 版，或使用 macOS 原生版', 'Run the Windows build through CrossOver, or use the native macOS build', 'CrossOver로 Windows 버전을 실행하거나 macOS 네이티브 버전을 사용합니다', 'CrossOver で Windows 版を実行するか、macOS ネイティブ版を使用します'],
  '尚未安装，点击右侧按钮安装': ['尚未安裝，點擊右側按鈕安裝', 'Not installed — use the button on the right', '설치되지 않았습니다. 오른쪽 버튼으로 설치하세요', '未インストールです。右のボタンからインストールしてください'],
  '尚未安装框架，检测到已有的 plugins / config，安装后会保留': ['尚未安裝框架，偵測到既有的 plugins / config，安裝後會保留', 'Framework not installed. Existing plugins / config found and will be kept after install.', '프레임워크가 설치되지 않았습니다. 기존 plugins / config 가 있으며 설치 후 유지됩니다.', 'フレームワーク未インストール。既存の plugins / config はインストール後も保持されます。'],
  '管理': ['管理', 'Manage', '관리', '管理'],
  // BepInEx 平台安装引导
  '安装前准备': ['安裝前準備', 'Before installing', '설치 전 준비', 'インストール前の準備'],
  '还需要一步': ['還需要一步', 'One more step', '한 단계 더', 'あと 1 ステップ'],
  '准备 CrossOver 容器': ['準備 CrossOver 容器', 'Prepare the CrossOver bottle', 'CrossOver 병 준비', 'CrossOver ボトルを準備'],
  '确认游戏能在 CrossOver 中正常启动': ['確認遊戲能在 CrossOver 中正常啟動', 'Make sure the game launches in CrossOver', 'CrossOver에서 게임이 정상 실행되는지 확인', 'CrossOver でゲームが起動することを確認'],
  '安装字体': ['安裝字型', 'Install fonts', '글꼴 설치', 'フォントをインストール'],
  '补上 corefonts，否则 BepInEx 控制台会显示乱码': ['補上 corefonts，否則 BepInEx 主控台會顯示亂碼', 'Add corefonts or the BepInEx console shows garbled text', 'corefonts가 없으면 BepInEx 콘솔이 깨져 보입니다', 'corefonts がないと BepInEx コンソールが文字化けします'],
  '安装 BepInEx': ['安裝 BepInEx', 'Install BepInEx', 'BepInEx 설치', 'BepInEx をインストール'],
  '将把 BepInEx 框架文件（不含 plugins 与 config）移入回收站，可在回收站中恢复。继续卸载？': ['將把 BepInEx 框架檔案（不含 plugins 與 config）移入資源回收筒，可在資源回收筒中還原。繼續解除安裝？', 'The BepInEx framework files (excluding plugins and config) will be moved to the recycle bin and can be restored. Continue uninstalling?', 'BepInEx 프레임워크 파일(plugins·config 제외)을 휴지통으로 옮깁니다. 복원할 수 있습니다. 제거할까요?', 'BepInEx フレームワークのファイル（plugins・config を除く）をゴミ箱へ移動します（復元可能）。アンインストールを続行しますか？'],
  // 全屏遮罩文案（备份/恢复/卸载都是整目录拷贝，需要明确的等待提示）
  '正在备份 BepInEx 框架…': ['正在備份 BepInEx 框架…', 'Backing up the BepInEx framework…', 'BepInEx 프레임워크 백업 중…', 'BepInEx フレームワークをバックアップ中…'],
  '正在恢复 BepInEx 框架…': ['正在還原 BepInEx 框架…', 'Restoring the BepInEx framework…', 'BepInEx 프레임워크 복원 중…', 'BepInEx フレームワークを復元中…'],
  '正在备份并卸载 BepInEx 框架…': ['正在備份並解除安裝 BepInEx 框架…', 'Backing up and removing the BepInEx framework…', 'BepInEx 프레임워크 백업 및 제거 중…', 'BepInEx フレームワークをバックアップして削除中…'],
  '回到这里点击下方按钮继续': ['回到這裡點擊下方按鈕繼續', 'Come back here and continue below', '여기로 돌아와 아래 버튼을 누르세요', 'ここに戻って下のボタンを押してください'],
  '未检测到 CrossOver 容器。请先在 CrossOver 里启动一次《胡闹厨房2》，让容器生成后再回来。': ['未偵測到 CrossOver 容器。請先在 CrossOver 裡啟動一次《胡鬧廚房2》，讓容器產生後再回來。', 'No CrossOver bottle detected. Launch Overcooked! 2 once through CrossOver so the bottle is created, then come back.', 'CrossOver 병을 찾지 못했습니다. CrossOver에서 《오버쿡드! 2》를 한 번 실행해 병을 만든 뒤 다시 오세요.', 'CrossOver ボトルが見つかりません。CrossOver で『オーバークック2』を一度起動してボトルを作成してから戻ってください。'],
  '在终端执行以下命令安装字体': ['在終端機執行以下指令安裝字型', 'Run this in a terminal to install fonts', '터미널에서 아래 명령을 실행해 글꼴을 설치하세요', 'ターミナルで次のコマンドを実行してフォントをインストールします'],
  '这一步不是必须的：跳过只会让 BepInEx 控制台里的中文显示成方块，不影响 MOD 加载。': ['這一步不是必須的：跳過只會讓 BepInEx 主控台裡的中文顯示成方塊，不影響 MOD 載入。', 'Optional: skipping it only makes Chinese text in the BepInEx console show as boxes; MODs still load.', '필수는 아닙니다. 건너뛰면 BepInEx 콘솔의 한글이 네모로 보일 뿐 MOD 로드에는 영향이 없습니다.', '必須ではありません。省略すると BepInEx コンソールの日本語が□で表示されるだけで、MOD の読み込みには影響しません。'],
  'BepInEx 已安装。最后一步：把下面的命令粘贴到 Steam 里《胡闹厨房2》的启动选项中。': ['BepInEx 已安裝。最後一步：把下面的指令貼到 Steam 裡《胡鬧廚房2》的啟動選項中。', 'BepInEx is installed. Last step: paste the command below into the Steam launch options for Overcooked! 2.', 'BepInEx가 설치되었습니다. 마지막 단계: 아래 명령을 Steam의 《오버쿡드! 2》 시작 옵션에 붙여넣으세요.', 'BepInEx をインストールしました。最後の手順：以下のコマンドを Steam の『オーバークック2』起動オプションに貼り付けてください。'],
  '启动选项': ['啟動選項', 'Launch options', '시작 옵션', '起動オプション'],
  '尚未探测到游戏目录，无法生成启动命令。请先在设置里确认游戏目录。': ['尚未偵測到遊戲目錄，無法產生啟動指令。請先在設定裡確認遊戲目錄。', 'No game folder detected, so the launch command cannot be generated. Check the game folder in Settings first.', '게임 경로를 찾지 못해 시작 명령을 만들 수 없습니다. 설정에서 게임 경로를 먼저 확인하세요.', 'ゲームフォルダーが見つからないため起動コマンドを生成できません。設定でゲームフォルダーを確認してください。'],
  '库中右键游戏': ['在媒體庫中右鍵遊戲', 'Right-click the game in your library', '라이브러리에서 게임 우클릭', 'ライブラリでゲームを右クリック'],
  'Steam → 库 → 右键《胡闹厨房2》': ['Steam → 媒體庫 → 右鍵《胡鬧廚房2》', 'Steam → Library → right-click Overcooked! 2', 'Steam → 라이브러리 → 《오버쿡드! 2》 우클릭', 'Steam → ライブラリ → 『オーバークック2』を右クリック'],
  '打开属性': ['開啟內容', 'Open Properties', '속성 열기', 'プロパティを開く'],
  '选择「属性」→「通用」': ['選擇「內容」→「一般」', 'Choose Properties → General', '"속성" → "일반" 선택', '「プロパティ」→「一般」を選択'],
  '粘贴启动选项': ['貼上啟動選項', 'Paste the launch options', '시작 옵션 붙여넣기', '起動オプションを貼り付け'],
  '把上面的命令粘贴到「启动选项」': ['把上面的指令貼到「啟動選項」', 'Paste the command above into "Launch options"', '위 명령을 "시작 옵션"에 붙여넣으세요', '上のコマンドを「起動オプション」に貼り付けてください'],
  '我已完成所需操作': ['我已完成所需操作', 'I have completed these steps', '필요한 작업을 마쳤습니다', '必要な操作を完了しました'],
  '版本：{version}': ['版本：{version}', 'Version: {version}', '버전: {version}', 'バージョン: {version}'],
  '加载器：{state}': ['載入器：{state}', 'Loader: {state}', '로더: {state}', 'ローダー: {state}'],
  '框架已启用': ['框架已啟用', 'Framework enabled', '프레임워크 활성', 'フレームワーク有効'],
  '框架已禁用': ['框架已停用', 'Framework disabled', '프레임워크 비활성', 'フレームワーク無効'],
  '未找到加载器': ['未找到載入器', 'Loader missing', '로더 없음', 'ローダーが見つかりません'],
  '状态冲突': ['狀態衝突', 'State conflict', '상태 충돌', '状態の競合'],
  '运行形态：{runtime}': ['執行形態：{runtime}', 'Runtime: {runtime}', '실행 형태: {runtime}', '実行形態: {runtime}'],
  '下载渠道': ['下載渠道', 'Download channel', '다운로드 채널', 'ダウンロード経路'],
  'Gitee（中国大陆推荐）': ['Gitee（中國大陸推薦）', 'Gitee (recommended in mainland China)', 'Gitee(중국 본토 권장)', 'Gitee（中国本土推奨）'],
  'GitHub': ['GitHub', 'GitHub', 'GitHub', 'GitHub'],
  '重新安装': ['重新安裝', 'Reinstall', '재설치', '再インストール'],
  '安装框架': ['安裝框架', 'Install framework', '프레임워크 설치', 'フレームワークをインストール'],
  '创建备份': ['建立備份', 'Create backup', '백업 생성', 'バックアップを作成'],
  '卸载框架': ['解除安裝框架', 'Uninstall framework', '프레임워크 제거', 'フレームワークを削除'],
  '打开备份目录': ['開啟備份目錄', 'Open backup folder', '백업 폴더 열기', 'バックアップフォルダーを開く'],
  '框架启用（游戏加载 MOD）': ['框架啟用（遊戲載入 MOD）', 'Framework enabled (game loads MODs)', '프레임워크 활성(게임이 MOD 로드)', 'フレームワーク有効（ゲームがMODを読み込む）'],
  '框架备份': ['框架備份', 'Framework backups', '프레임워크 백업', 'フレームワークのバックアップ'],
  '暂无备份。卸载或重装前建议先创建备份。': ['暫無備份。解除安裝或重新安裝前建議先建立備份。', 'No backups yet. Create one before uninstalling or reinstalling.', '백업이 없습니다. 제거·재설치 전에 백업을 만드세요.', 'バックアップはありません。削除・再インストール前に作成してください。'],
  '恢复': ['還原', 'Restore', '복원', '復元'],
  '检测到 MOD 目录，安装后会保留': ['偵測到 MOD 目錄，安裝後會保留', 'Existing MOD folder found, kept after install', 'MOD 폴더가 있습니다. 설치 후에도 유지됩니다', 'MOD フォルダーがあります。インストール後も保持されます'],
  '检测到已有的 plugins / config，安装框架时会原样保留。': ['偵測到既有的 plugins / config，安裝框架時會原樣保留。', 'Existing plugins / config found; they are kept as-is when installing the framework.', '기존 plugins / config 가 있습니다. 프레임워크 설치 시 그대로 유지됩니다.', '既存の plugins / config があります。フレームワークのインストール時にそのまま保持されます。'],
  '安装后这里会出现启用开关、配置管理与备份入口。': ['安裝後這裡會出現啟用開關、設定管理與備份入口。', 'After installing, the enable switch, config manager and backups appear here.', '설치하면 활성 스위치, 설정 관리, 백업 항목이 여기에 표시됩니다.', 'インストールすると、有効スイッチ・設定管理・バックアップがここに表示されます。'],
  // 回收站
  '回收站': ['資源回收筒', 'Recycle bin', '휴지통', 'ゴミ箱'],
  '回收站为空': ['資源回收筒是空的', 'Recycle bin is empty', '휴지통이 비어 있습니다', 'ゴミ箱は空です'],
  '尚未删除任何 MOD。删除的 MOD 会先暂存在这里，可随时恢复。': ['尚未刪除任何 MOD。刪除的 MOD 會先暫存在這裡，可隨時還原。', 'No MODs deleted yet. Deleted MODs are kept here until you remove them permanently.', '삭제한 MOD가 없습니다. 삭제한 MOD는 여기에 보관되며 언제든 복원할 수 있습니다.', 'まだ MOD を削除していません。削除した MOD はここに保管され、いつでも復元できます。'],
  '正在读取回收站': ['正在讀取資源回收筒', 'Loading recycle bin', '휴지통 읽는 중', 'ゴミ箱を読み込み中'],
  '立即刷新': ['立即重新整理', 'Refresh now', '지금 새로 고침', '今すぐ更新'],
  '彻底删除': ['永久刪除', 'Delete permanently', '완전 삭제', '完全に削除'],
  '清空回收站': ['清空資源回收筒', 'Empty recycle bin', '휴지통 비우기', 'ゴミ箱を空にする'],
  '已清空回收站': ['已清空資源回收筒', 'Recycle bin emptied', '휴지통을 비웠습니다', 'ゴミ箱を空にしました'],
  '将永久删除回收站中的全部 {count} 条记录及其文件，无法恢复。确定清空？': ['將永久刪除資源回收筒中的全部 {count} 筆記錄及其檔案，無法復原。確定清空？', 'All {count} recycled entries and their files will be permanently deleted. This cannot be undone. Empty the bin?', '휴지통의 {count}개 항목과 파일을 모두 영구 삭제합니다. 되돌릴 수 없습니다. 비울까요?', 'ゴミ箱の全 {count} 件の記録とファイルを完全に削除します。元に戻せません。空にしますか？'],
  '全部彻底删除': ['全部永久刪除', 'Delete all permanently', '모두 완전 삭제', 'すべて完全削除'],
  '彻底删除 MOD': ['永久刪除 MOD', 'Delete MOD permanently', 'MOD 완전 삭제', 'MOD を完全に削除'],
  '将永久删除回收站中的 MOD 文件和保存的安装清单，此操作无法撤销。': ['將永久刪除資源回收筒中的 MOD 檔案與儲存的安裝清單，此操作無法復原。', 'This permanently deletes the MOD files and the saved install manifest. It cannot be undone.', '휴지통의 MOD 파일과 저장된 설치 목록을 영구 삭제합니다. 되돌릴 수 없습니다.', 'ゴミ箱内の MOD ファイルと保存されたインストール記録を完全に削除します。元に戻せません。'],
  '删除时间': ['刪除時間', 'Deleted', '삭제 시각', '削除日時'],
  '文件数量': ['檔案數量', 'Files', '파일 수', 'ファイル数'],
  '原游戏目录': ['原遊戲目錄', 'Original location', '원래 게임 경로', '元のゲームパス'],
  '查看文件清单': ['檢視檔案清單', 'View files', '파일 목록 보기', 'ファイル一覧を表示'],
  '游戏运行中，暂时不能恢复 MOD。': ['遊戲執行中，暫時無法還原 MOD。', 'The game is running; MODs cannot be restored right now.', '게임이 실행 중이라 지금은 MOD를 복원할 수 없습니다.', 'ゲーム実行中のため、現在 MOD を復元できません。'],
  // 回收站（工具页只保留「打开回收站」，开关在设置 → 通用设置）
  '打开回收站': ['開啟資源回收筒', 'Open recycle bin', '휴지통 열기', 'ゴミ箱を開く'],
  '查看已删除的 MOD，可恢复到原位置或彻底删除': ['檢視已刪除的 MOD，可還原到原位置或永久刪除', 'Review deleted MODs; restore them or delete permanently', '삭제된 MOD를 확인하고 복원하거나 완전 삭제합니다', '削除した MOD を確認し、復元または完全削除します'],
  '删除 MOD 时优先放入回收站': ['刪除 MOD 時優先放入資源回收筒', 'Move deleted MODs to the recycle bin', 'MOD 삭제 시 휴지통으로 이동', 'MOD 削除時にゴミ箱へ移動'],
  '关闭后删除的文件不会保留，无法恢复': ['關閉後刪除的檔案不會保留，無法還原', 'When off, deleted files are removed permanently', '끄면 삭제한 파일이 보관되지 않으며 복원할 수 없습니다', 'オフにすると削除したファイルは保存されず、復元できません'],
  '删除的 MOD 将先移入回收站': ['刪除的 MOD 會先移入資源回收筒', 'Deleted MODs will go to the recycle bin', '삭제한 MOD는 휴지통으로 이동합니다', '削除した MOD はゴミ箱に移動します'],
  '删除的 MOD 将直接删除，不进回收站': ['刪除的 MOD 將直接刪除，不進資源回收筒', 'Deleted MODs are removed permanently', '삭제한 MOD는 휴지통을 거치지 않고 바로 삭제됩니다', '削除した MOD はゴミ箱を経由せず直接削除されます'],
  '关闭回收站保护': ['關閉資源回收筒保護', 'Turn off recycle bin protection', '휴지통 보호 끄기', 'ゴミ箱の保護を無効にする'],
  '关闭后，管理器删除 MOD 时不再移入回收站，而是直接删除文件与安装清单。': ['關閉後，管理器刪除 MOD 時不再移入資源回收筒，而是直接刪除檔案與安裝清單。', 'With this off, deleting a MOD removes its files and install manifest immediately instead of moving them to the recycle bin.', '끄면 MOD 삭제 시 휴지통으로 옮기지 않고 파일과 설치 목록을 바로 삭제합니다.', 'オフにすると、MOD を削除するときにゴミ箱へ移さず、ファイルとインストール記録を直接削除します。'],
  '这个操作无法撤销，删除的 MOD 不能恢复；覆盖安装时旧版本也会被直接清除。请确认你确实想绕过回收站。': ['這個操作無法復原，刪除的 MOD 不能還原；覆蓋安裝時舊版本也會被直接清除。請確認你確實想繞過資源回收筒。', 'This cannot be undone: deleted MODs are unrecoverable, and overwriting an install also discards the previous version. Confirm that you really want to bypass the recycle bin.', '되돌릴 수 없습니다. 삭제한 MOD는 복원할 수 없고, 덮어쓰기 설치 시 이전 버전도 그대로 사라집니다. 정말 휴지통을 거치지 않겠습니까?', '元に戻せません。削除した MOD は復元できず、上書きインストール時は旧バージョンも直接消えます。本当にゴミ箱を経由しないようにしますか？'],
  '仍然直接删除': ['仍要直接刪除', 'Delete directly anyway', '그래도 바로 삭제', 'それでも直接削除する'],
  '保持开启': ['保持開啟', 'Keep it on', '켜 둔 채로 유지', 'オンのままにする'],
  '卸载': ['解除安裝', 'Uninstall', '제거', 'アンインストール'],
  '取消': ['取消', 'Cancel', '취소', 'キャンセル'],
  '卸载 BepInEx 框架': ['解除安裝 BepInEx 框架', 'Uninstall BepInEx', 'BepInEx 제거', 'BepInEx をアンインストール'],
  '将移除游戏目录中的 BepInEx 框架文件。建议先创建备份。继续卸载？': ['將移除遊戲目錄中的 BepInEx 框架檔案。建議先建立備份。繼續解除安裝？', 'This removes BepInEx files from the game folder. Create a backup first. Continue?', '게임 폴더의 BepInEx 파일을 제거합니다. 먼저 백업하는 것이 좋습니다. 계속할까요?', 'ゲームフォルダーの BepInEx ファイルを削除します。先にバックアップを推奨します。続行しますか？'],
  '删除备份': ['刪除備份', 'Delete backup', '백업 삭제', 'バックアップを削除'],
  '永久删除该备份？': ['永久刪除該備份？', 'Permanently delete this backup?', '이 백업을 영구 삭제할까요?', 'このバックアップを完全に削除しますか？'],

  // MOD 安装
  '共 {count} 个仓库': ['共 {count} 個倉庫', '{count} repositories', '저장소 {count}개', 'リポジトリ {count} 件'],
  '全部刷新': ['全部重新整理', 'Refresh all', '모두 새로 고침', 'すべて更新'],
  '暂无仓库配置': ['暫無倉庫設定', 'No repositories configured', '저장소 설정 없음', 'リポジトリ設定がありません'],
  '仓库文件': ['倉庫檔案', 'Repository file', '저장소 파일', 'リポジトリファイル'],
  '未启用': ['未啟用', 'Disabled', '비활성', '無効'],
  '仓库刷新失败': ['倉庫重新整理失敗', 'Repository refresh failed', '저장소 새로 고침 실패', 'リポジトリ更新失敗'],
  '刷新': ['重新整理', 'Refresh', '새로 고침', '更新'],
  '安装': ['安裝', 'Install', '설치', 'インストール'],
  '版本': ['版本', 'Versions', '버전', 'バージョン'],
  '未刷新': ['未重新整理', 'Not refreshed', '새로 고침 안 됨', '未更新'],
  '{count} 个版本': ['{count} 個版本', '{count} versions', '{count}개 버전', '{count} バージョン'],
  '预发布': ['預發布', 'Pre-release', '프리릴리스', 'プレリリース'],
  // 依赖提示：能匹配到线上来源时给「安装依赖」按钮，否则只显示「缺依赖」标签
  '安装依赖': ['安裝依賴', 'Install dependency', '의존성 설치', '依存関係をインストール'],
  '缺少依赖': ['缺少依賴', 'Missing dependency', '의존성 누락', '依存関係が不足'],
  '缺依赖': ['缺依賴', 'Missing dep', '의존성 없음', '依存不足'],
  // 关系冲突：装了不该装的，或对方版本不在兼容区间内。解法是卸载而不是安装。
  '冲突': ['衝突', 'Conflict', '충돌', '競合'],
  '去卸载': ['去解除安裝', 'Uninstall', '제거하기', 'アンインストールへ'],
  '卸载冲突的 MOD': ['解除安裝衝突的 MOD', 'Uninstall conflicting MOD', '충돌 MOD 제거', '競合 MOD をアンインストール'],
  '「{owner}」与「{target}」不兼容，需要卸载': [
    '「{owner}」與「{target}」不相容，需要解除安裝',
    '"{owner}" is incompatible with "{target}" and must be uninstalled',
    '"{owner}"은(는) "{target}"와(과) 호환되지 않아 제거가 필요합니다',
    '「{owner}」は「{target}」と互換性がないため、アンインストールが必要です',
  ],
  // 兼容列表：MOD 只和这些指定 MOD 的指定版本兼容（compatible 关系不满足）
  '不兼容': ['不相容', 'Incompatible', '비호환', '非互換'],
  // 仅使用内置仓库源：跳过按地区选择 raw 直链
  '仅使用内置仓库源': [
    '僅使用內建倉庫來源',
    'Use bundled sources only',
    '내장 저장소 소스만 사용',
    '組み込みソースのみ使用',
  ],
  '不访问 Gitee / GitHub 的仓库源直链，直接使用管理器自带的快照；自定义仓库源不受影响': [
    '不存取 Gitee / GitHub 的倉庫來源直鏈，直接使用管理器自帶的快照；自訂倉庫來源不受影響',
    'Skips the Gitee / GitHub raw source links and uses the snapshot shipped with the manager. Custom sources are not affected.',
    'Gitee / GitHub raw 링크를 사용하지 않고 관리자에 포함된 스냅샷을 사용합니다. 사용자 지정 소스는 영향받지 않습니다.',
    'Gitee / GitHub の raw リンクを使わず、マネージャー同梱のスナップショットを使用します。カスタムソースには影響しません。',
  ],
  '已切换到内置仓库源，刷新后生效': [
    '已切換到內建倉庫來源，重新整理後生效',
    'Switched to bundled sources. Refresh to apply.',
    '내장 저장소 소스로 전환했습니다. 새로 고침 후 적용됩니다.',
    '組み込みソースに切り替えました。更新後に反映されます。',
  ],
  '已恢复在线仓库源，刷新后生效': [
    '已恢復線上倉庫來源，重新整理後生效',
    'Restored online sources. Refresh to apply.',
    '온라인 저장소 소스로 복원했습니다. 새로 고침 후 적용됩니다.',
    'オンラインソースに戻しました。更新後に反映されます。',
  ],

  // 安装前检查：依赖 / 冲突 / 兼容一次处理完
  '安装前检查': ['安裝前檢查', 'Pre-install check', '설치 전 검사', 'インストール前の確認'],
  '安装「{name}」前，需要先处理以下关系：': [
    '安裝「{name}」前，需要先處理以下關係：',
    'Before installing "{name}", these relations must be handled first:',
    '"{name}" 설치 전에 다음 관계를 먼저 처리해야 합니다:',
    '「{name}」をインストールする前に、以下の関係を処理する必要があります：',
  ],
  '全部处理': ['全部處理', 'Handle all', '모두 처리', 'すべて処理'],
  '仍然安装': ['仍然安裝', 'Install anyway', '그래도 설치', 'それでもインストール'],
  '已完成 {count} 项处理': [
    '已完成 {count} 項處理',
    'Handled {count} item(s)',
    '{count}개 항목을 처리했습니다',
    '{count} 件を処理しました',
  ],
  '找不到要卸载的 MOD': ['找不到要解除安裝的 MOD', 'The MOD to uninstall was not found', '제거할 MOD를 찾을 수 없습니다', 'アンインストール対象の MOD が見つかりません'],
  '找不到可安装的来源': ['找不到可安裝的來源', 'No installable source found', '설치 가능한 소스를 찾을 수 없습니다', 'インストール可能なソースが見つかりません'],
  // 关系提醒：无法自动处理时的说明
  '「{name}」{version} 不兼容，需要 {range} 之间的版本': [
    '「{name}」{version} 不相容，需要 {range} 之間的版本',
    '"{name}" {version} is incompatible; a version in {range} is required',
    '"{name}" {version}은(는) 호환되지 않습니다. {range} 버전이 필요합니다',
    '「{name}」{version} は非互換です。{range} のバージョンが必要です',
  ],
  '「{name}」{version} 不兼容，请注意': [
    '「{name}」{version} 不相容，請注意',
    '"{name}" {version} is incompatible; please check',
    '"{name}" {version}은(는) 호환되지 않습니다. 확인해 주세요',
    '「{name}」{version} は非互換です。ご確認ください',
  ],
  '「{name}」{version} 冲突，请卸载': [
    '「{name}」{version} 衝突，請解除安裝',
    '"{name}" {version} conflicts; please uninstall it',
    '"{name}" {version}이(가) 충돌합니다. 제거해 주세요',
    '「{name}」{version} が競合しています。アンインストールしてください',
  ],
  '缺少依赖「{name}」，需要 {range} 之间的版本': [
    '缺少依賴「{name}」，需要 {range} 之間的版本',
    'Missing dependency "{name}"; a version in {range} is required',
    '의존성 "{name}"이(가) 없습니다. {range} 버전이 필요합니다',
    '依存関係「{name}」がありません。{range} のバージョンが必要です',
  ],
  '缺少依赖「{name}」': [
    '缺少依賴「{name}」',
    'Missing dependency "{name}"',
    '의존성 "{name}"이(가) 없습니다',
    '依存関係「{name}」がありません',
  ],
  // 版本号后的测试标记：解析不出数字序号时用这个字提示
  '测': ['測', 'T', '테', '試'],
  '安装此版本': ['安裝此版本', 'Install this version', '이 버전 설치', 'このバージョンをインストール'],
  '尚无缓存的 Release，请先刷新。': ['尚無快取的 Release，請先重新整理。', 'No cached releases. Refresh first.', '캐시된 Release가 없습니다. 먼저 새로 고침하세요.', 'キャッシュされたReleaseがありません。先に更新してください。'],
  '游戏运行中，已禁止安装。请先关闭游戏。': ['遊戲執行中，已禁止安裝。請先關閉遊戲。', 'The game is running. Close it before installing.', '게임 실행 중에는 설치할 수 없습니다. 먼저 게임을 종료하세요.', 'ゲーム実行中はインストールできません。先にゲームを終了してください。'],
  '获取到 {count} 个 Release': ['取得 {count} 個 Release', 'Fetched {count} releases', 'Release {count}개를 가져왔습니다', 'Release を {count} 件取得しました'],

  // MOD 管理
  '名称': ['名稱', 'Name', '이름', '名前'],
  '卡片': ['卡片', 'Cards', '카드', 'カード'],
  '横排': ['橫排', 'Rows', '행', '行'],
  '尚未安装任何 MOD': ['尚未安裝任何 MOD', 'No MODs installed', '설치된 MOD 없음', 'MODは未インストールです'],
  '有更新': ['有更新', 'Update', '업데이트', '更新あり'],
  '最新版': ['最新版', 'Latest', '최신', '最新'],
  '详情': ['詳情', 'Details', '상세', '詳細'],
  '收起': ['收起', 'Collapse', '접기', '折りたたむ'],
  '无更新': ['無更新', 'Up to date', '최신 상태', '最新です'],
  '未绑定': ['未綁定', 'Unbound', '미연결', '未リンク'],
  '仓库缺失': ['倉庫缺失', 'Repository missing', '저장소 없음', 'リポジトリなし'],
  '重复': ['重複', 'Duplicate', '중복', '重複'],
  '旧副本': ['舊副本', 'Old copy', '이전 사본', '古いコピー'],
  '加载警告': ['載入警告', 'Load warning', '로드 경고', '読み込み警告'],
  '已禁用': ['已停用', 'Disabled', '비활성', '無効'],
  'GUID：{guid}': ['GUID：{guid}', 'GUID: {guid}', 'GUID: {guid}', 'GUID: {guid}'],
  '文件：{file}': ['檔案：{file}', 'File: {file}', '파일: {file}', 'ファイル: {file}'],
  'MOD 包：{name}（{count} 个）': ['MOD 包：{name}（{count} 個）', 'MOD package: {name} ({count})', 'MOD 패키지: {name}({count}개)', 'MOD パッケージ: {name}（{count} 件）'],
  '警告：{warning}': ['警告：{warning}', 'Warning: {warning}', '경고: {warning}', '警告: {warning}'],
  '删除 MOD': ['刪除 MOD', 'Delete MOD', 'MOD 삭제', 'MOD を削除'],
  '「{name}」将被移入回收站，可在回收站中恢复。继续？': ['「{name}」將被移入資源回收筒，可在資源回收筒中還原。繼續？', '"{name}" will be moved to the recycle bin and can be restored. Continue?', '"{name}"을(를) 휴지통으로 옮깁니다. 복원할 수 있습니다. 계속할까요?', '「{name}」をゴミ箱へ移動します（復元可能）。続行しますか？'],
  '移入回收站': ['移入資源回收筒', 'Move to recycle bin', '휴지통으로 이동', 'ゴミ箱へ移動'],
  // 关闭「优先放入回收站」后的删除文案：必须明确不可恢复
  '「{name}」将被直接删除，无法恢复。继续？': ['「{name}」將被直接刪除，無法還原。繼續？', '"{name}" will be deleted permanently. This cannot be undone. Continue?', '"{name}"을(를) 바로 삭제합니다. 복원할 수 없습니다. 계속할까요?', '「{name}」を直接削除します。元に戻せません。続行しますか？'],
  '直接删除': ['直接刪除', 'Delete permanently', '바로 삭제', '直接削除'],
  '该 MOD 属于 MOD 包': ['該 MOD 屬於 MOD 包', 'This MOD belongs to a package', '이 MOD는 패키지에 속합니다', 'このMODはパッケージに属します'],
  '它与 {count} 个子 MOD 同属一个包，一并删除？': ['它與 {count} 個子 MOD 同屬一個包，一併刪除？', 'It shares a package with {count} MODs. Delete them together?', '{count}개 MOD와 같은 패키지입니다. 함께 삭제할까요?', '{count} 件のMODと同じパッケージです。まとめて削除しますか？'],
  '整包删除': ['整包刪除', 'Delete package', '패키지 삭제', 'パッケージを削除'],
  '切换布局（卡片 / 横排）': ['切換佈局（卡片 / 橫排）', 'Switch layout (cards / rows)', '레이아웃 전환(카드/행)', 'レイアウト切替（カード／行）'],
  '排序：{sort}': ['排序：{sort}', 'Sort: {sort}', '정렬: {sort}', '並び: {sort}'],
  '全部刷新（逐仓独立，互不阻塞）': ['全部重新整理（逐倉獨立，互不阻塞）', 'Refresh all (per repository, non-blocking)', '모두 새로 고침(저장소별 독립)', 'すべて更新（リポジトリ単位・非ブロッキング）'],

  // 设置
  '路径管理': ['路徑管理', 'Paths', '경로 관리', 'パス管理'],
  '游戏目录': ['遊戲目錄', 'Game directory', '게임 폴더', 'ゲームフォルダー'],
  '游戏与 BepInEx 框架所在目录；DIYLevel 自定义地图框架路径后续也在此管理': ['遊戲與 BepInEx 框架所在目錄；DIYLevel 自訂地圖框架路徑後續也在此管理', 'Folder containing the game and BepInEx; DIYLevel map framework paths will live here too', '게임과 BepInEx가 있는 폴더. DIYLevel 지도 프레임워크 경로도 여기서 관리합니다', 'ゲームとBepInExのあるフォルダー。DIYLevel マップフレームワークのパスもここで管理します'],
  '恢复默认路径': ['還原預設路徑', 'Reset path', '기본 경로로 복원', '既定パスに戻す'],
  '打开路径': ['開啟路徑', 'Open folder', '경로 열기', 'パスを開く'],
  '选择路径': ['選擇路徑', 'Choose path', '경로 선택', 'パスを選択'],
  '程序更新': ['程式更新', 'App update', '앱 업데이트', 'アプリの更新'],
  // 总览页四张卡片
  '去更新': ['去更新', 'Review', '업데이트하기', '更新へ'],
  '去管理': ['去管理', 'Manage', '관리하기', '管理へ'],
  '去安装': ['去安裝', 'Browse', '설치하기', 'インストールへ'],
  '在线 MOD': ['線上 MOD', 'Online MODs', '온라인 MOD', 'オンラインMOD'],
  '本地 MOD': ['本機 MOD', 'Local MODs', '로컬 MOD', 'ローカルMOD'],
  '去刷新': ['去重新整理', 'Refresh', '새로 고침', '更新へ'],
  '共 {count} 个仓库 · {failed} 个刷新失败（{names}）': ['共 {count} 個倉庫 · {failed} 個重新整理失敗（{names}）', '{count} repositories · {failed} failed to refresh ({names})', '저장소 {count}개 · {failed}개 새로 고침 실패({names})', 'リポジトリ {count} 件 · {failed} 件が更新失敗（{names}）'],
  '共 {count} 个仓库 · 全部正常': ['共 {count} 個倉庫 · 全部正常', '{count} repositories · all healthy', '저장소 {count}개 · 모두 정상', 'リポジトリ {count} 件 · すべて正常'],
  '尚未加载任何仓库': ['尚未載入任何倉庫', 'No repositories loaded yet', '아직 저장소가 없습니다', 'リポジトリがまだありません'],
  '等 {count} 个': ['等 {count} 個', 'and {count} total', '등 {count}개', 'ほか {count} 件'],
  '共 {count} 个，其中 {updates} 个可更新': ['共 {count} 個，其中 {updates} 個可更新', '{count} installed, {updates} can update', '{count}개 중 {updates}개 업데이트 가능', '{count} 件中 {updates} 件が更新可能'],
  '共 {count} 个，全部为最新版': ['共 {count} 個，全部為最新版', '{count} installed, all up to date', '{count}개 모두 최신 버전', '{count} 件すべて最新版'],
  '暂无已删除的 MOD': ['暫無已刪除的 MOD', 'Nothing deleted yet', '삭제된 MOD가 없습니다', '削除された MOD はありません'],
  '{count} 个已删除的 MOD，共 {files} 个文件': ['{count} 個已刪除的 MOD，共 {files} 個檔案', '{count} deleted MODs, {files} files total', '삭제된 MOD {count}개, 파일 {files}개', '削除済み MOD {count} 件、ファイル {files} 個'],
  '全部 MOD': ['全部 MOD', 'All MODs', '모든 MOD', 'すべてのMOD'],
  '已启用 {enabled} / {total}': ['已啟用 {enabled} / {total}', '{enabled} of {total} enabled', '{total}개 중 {enabled}개 활성', '{total} 件中 {enabled} 件が有効'],
  '有 MOD 未启用': ['有 MOD 未啟用', 'Some MODs are disabled', '비활성 MOD가 있습니다', '無効な MOD があります'],
  '全部启用': ['全部啟用', 'Enable all', '모두 활성화', 'すべて有効化'],
  '全部禁用': ['全部停用', 'Disable all', '모두 비활성화', 'すべて無効化'],
  '禁用全部 MOD': ['停用全部 MOD', 'Disable all MODs', '모든 MOD 비활성화', 'すべてのMODを無効化'],
  '将禁用全部 {count} 个 MOD，游戏将不加载任何 MOD。可以随时重新启用。': ['將停用全部 {count} 個 MOD，遊戲將不載入任何 MOD。可以隨時重新啟用。', 'All {count} MODs will be disabled; the game will load none of them. You can re-enable them anytime.', 'MOD {count}개를 모두 비활성화합니다. 게임이 어떤 MOD도 불러오지 않습니다. 언제든 다시 활성화할 수 있습니다.', 'MOD {count} 件をすべて無効化します。ゲームはどの MOD も読み込みません。いつでも再度有効化できます。'],
  '安装后才能加载 MOD': ['安裝後才能載入 MOD', 'Install it to load MODs', '설치해야 MOD를 불러올 수 있습니다', 'インストールしないとMODを読み込めません'],
  '检测更新': ['檢查更新', 'Check', '확인', '確認'],
  '只刷新已安装 MOD 绑定的仓库，检查是否有新版本': ['只重新整理已安裝 MOD 綁定的倉庫，檢查是否有新版本', 'Refresh only the repositories your installed MODs are bound to, and check for new versions', '설치된 MOD가 연결된 저장소만 새로 고침해 새 버전이 있는지 확인합니다', 'インストール済み MOD が紐づくリポジトリだけを更新し、新しいバージョンを確認します'],
  '没有已安装 MOD 绑定到可用的在线仓库': ['沒有已安裝 MOD 綁定到可用的線上倉庫', 'No installed MOD is bound to an available online repository', '사용 가능한 온라인 저장소에 연결된 설치된 MOD가 없습니다', '利用可能なオンラインリポジトリに紐づくインストール済み MOD はありません'],
  '通用设置': ['一般設定', 'General', '일반 설정', '一般設定'],
  '重装': ['重新安裝', 'Reinstall', '재설치', '再インストール'],
  '启动游戏': ['啟動遊戲', 'Launch game', '게임 실행', 'ゲームを起動'],
  '停止游戏': ['停止遊戲', 'Stop game', '게임 중지', 'ゲームを停止'],
  '启动时检查程序更新': ['啟動時檢查程式更新', 'Check for updates on startup', '시작 시 업데이트 확인', '起動時に更新を確認'],
  '启动管理器时检查新版本，可在设置中关闭': ['啟動管理器時檢查新版本，可在設定中關閉', 'Check for a new version when the manager starts; can be turned off in settings', '관리자 시작 시 새 버전을 확인합니다. 설정에서 끌 수 있습니다', 'マネージャー起動時に新バージョンを確認します（設定で無効化できます）'],
  '自动检测游戏路径': ['自動偵測遊戲路徑', 'Auto-detect game path', '게임 경로 자동 감지', 'ゲームパスを自動検出'],
  '启动与刷新时自动探测 Steam 库': ['啟動與重新整理時自動探測 Steam 庫', 'Probe Steam libraries on launch and refresh', '시작·새로 고침 시 Steam 라이브러리를 탐색합니다', '起動時と更新時にSteamライブラリを検出します'],
  '关于': ['關於', 'About', '정보', 'このアプリについて'],
  '当前路径': ['目前路徑', 'Current path', '현재 경로', '現在のパス'],
  '选择或输入目录路径': ['選擇或輸入目錄路徑', 'Choose or enter a folder path', '폴더 경로를 선택하거나 입력하세요', 'フォルダーパスを選択または入力'],
  '浏览…': ['瀏覽…', 'Browse…', '찾아보기…', '参照…'],
  '确认保存': ['確認儲存', 'Save', '저장', '保存'],
  '选择游戏目录': ['選擇遊戲目錄', 'Choose game directory', '게임 폴더 선택', 'ゲームフォルダーを選択'],
  '目录需包含 Overcooked!2.exe；保存后自动验证。': ['目錄需包含 Overcooked!2.exe；儲存後自動驗證。', 'The folder must contain Overcooked!2.exe; it is verified on save.', '폴더에 Overcooked!2.exe가 있어야 하며 저장 후 검증합니다.', 'Overcooked!2.exe を含む必要があります。保存時に検証します。'],

  // 工具
  '支持开发': ['支持開發', 'Support development', '개발 후원', '開発を支援'],
  '觉得好用，请作者喝杯咖啡': ['覺得好用，請作者喝杯咖啡', 'Buy the author a coffee', '유용했다면 커피 한 잔 부탁드립니다', '役に立ったらコーヒー一杯お願いします'],
  '所有功能永久免费，赞助用于服务器与开发动力。感谢支持！': ['所有功能永久免費，贊助用於伺服器與開發動力。感謝支持！', 'All features stay free; sponsorships fund servers and development. Thank you!', '모든 기능은 무료이며 후원은 서버와 개발에 사용됩니다. 감사합니다!', 'すべての機能は無料です。支援はサーバーと開発に使われます。ありがとうございます！'],
  '日志': ['日誌', 'Logs', '로그', 'ログ'],
  '打开 Rust 后端日志': ['開啟 Rust 後端日誌', 'Open Rust log', 'Rust 로그 열기', 'Rust ログを開く'],
  '安装、下载与文件操作的详细记录': ['安裝、下載與檔案操作的詳細記錄', 'Detailed record of installs, downloads and file operations', '설치·다운로드·파일 작업의 상세 기록', 'インストール・ダウンロード・ファイル操作の詳細'],
  '打开前端日志': ['開啟前端日誌', 'Open frontend log', '프런트엔드 로그 열기', 'フロントエンドログを開く'],
  '界面渲染与调用的记录': ['介面渲染與呼叫的記錄', 'UI rendering and IPC calls', 'UI 렌더링과 호출 기록', '画面描画と呼び出しの記録'],
  '打开崩溃日志': ['開啟崩潰日誌', 'Open crash log', '크래시 로그 열기', 'クラッシュログを開く'],
  '程序异常退出时的诊断信息': ['程式異常結束時的診斷資訊', 'Diagnostics from abnormal exits', '비정상 종료 시 진단 정보', '異常終了時の診断情報'],
  '高级': ['進階', 'Advanced', '고급', '詳細設定'],
  '打开开发者工具': ['開啟開發者工具', 'Open developer tools', '개발자 도구 열기', '開発者ツールを開く'],
  'WebView 开发者控制台（调试用）': ['WebView 開發者主控台（偵錯用）', 'WebView developer console (debugging)', 'WebView 개발자 콘솔(디버깅)', 'WebView 開発者コンソール（デバッグ用）'],
  '重置窗口状态': ['重設視窗狀態', 'Reset window state', '창 상태 초기화', 'ウィンドウ状態をリセット'],
  '窗口位置或尺寸异常时恢复默认布局': ['視窗位置或尺寸異常時恢復預設版面', 'Restore the default layout when the window is misplaced', '창 위치·크기가 이상할 때 기본 배치로 복원', 'ウィンドウの位置・サイズが異常なとき既定の配置に戻します'],
  '末尾 512 KiB': ['末尾 512 KiB', 'Last 512 KiB', '끝 512 KiB', '末尾 512 KiB'],
  '（空）': ['（空）', '(empty)', '(비어 있음)', '（空）'],

  // 更新弹窗
  '发现新版本': ['發現新版本', 'Update available', '새 버전 발견', '新しいバージョン'],
  '更新内容：': ['更新內容：', 'What\'s new:', '업데이트 내용:', '更新内容：'],
  '本次更新没有提供说明。': ['本次更新沒有提供說明。', 'No release notes were provided.', '이번 업데이트에는 설명이 없습니다.', '今回の更新に説明はありません。'],
  '当前 {current} → 新版 {next}': ['目前 {current} → 新版 {next}', '{current} → {next}', '{current} → {next}', '{current} → {next}'],
  '准备下载更新…': ['準備下載更新…', 'Preparing download…', '업데이트 다운로드 준비 중…', '更新のダウンロードを準備中…'],
  '更新': ['更新', 'Update', '업데이트', '更新'],
  '稍后': ['稍後', 'Later', '나중에', '後で'],
  '当前已是最新版本': ['目前已是最新版本', 'You are up to date', '최신 버전입니다', '最新バージョンです'],
  '更新检查失败': ['檢查更新失敗', 'Update check failed', '업데이트 확인 실패', '更新確認に失敗しました'],
  '发现新版本 {version}': ['發現新版本 {version}', 'Version {version} available', '{version} 버전이 있습니다', 'バージョン {version} があります'],
  '全部渠道检查失败，可前往发布页手动下载。': ['全部渠道檢查失敗，可前往發布頁手動下載。', 'All channels failed. Download manually from the release page.', '모든 채널이 실패했습니다. 릴리스 페이지에서 직접 받으세요.', 'すべての経路が失敗しました。リリースページから手動で取得してください。'],
  '打开发布页': ['開啟發布頁', 'Open release page', '릴리스 페이지 열기', 'リリースページを開く'],

  '未归类仓库': ['未歸類倉庫', 'Uncategorized repositories', '미분류 저장소', '未分類リポジトリ'],
  '未分组': ['未分組', 'Ungrouped', '미분류', '未分類'],
  '最新版本': ['最新版本', 'Latest version', '최신 버전', '最新バージョン'],
  '下载量': ['下載量', 'Downloads', '다운로드', 'ダウンロード数'],
  '已就绪': ['已就緒', 'Ready', '준비됨', '準備完了'],
  '社区仓库': ['社群倉庫', 'Community repositories', '커뮤니티 저장소', 'コミュニティリポジトリ'],
  '来源未知': ['來源未知', 'Unknown origin', '출처 불명', '出所未詳'],
  '搜索': ['搜尋', 'Search', '검색', '検索'],
  '权限': ['權限', 'Permissions', '권한', '権限'],
  '以管理员身份运行': ['以管理員身分執行', 'Run as administrator', '관리자 권한으로 실행', '管理者として実行'],
  '当前已具备管理员权限': ['目前具備管理員權限', 'Administrator rights are active', '관리자 권한이 있습니다', '管理者権限があります'],
  '仅在游戏目录不可写时需要': ['僅在遊戲目錄不可寫時需要', 'Only needed when the game folder is not writable', '게임 폴더에 쓸 수 없을 때만 필요합니다', 'ゲームフォルダーに書き込めない場合のみ必要です'],
  '已提权': ['已提權', 'Elevated', '권한 있음', '昇格済み'],
  '普通权限': ['一般權限', 'Standard', '일반 권한', '通常権限'],
  '以管理员重启': ['以管理員重新啟動', 'Restart as administrator', '관리자 권한으로 재시작', '管理者として再起動'],
  '游戏目录可写': ['遊戲目錄可寫', 'Game folder writable', '게임 폴더 쓰기 가능', 'ゲームフォルダー書き込み可'],
  '可以直接安装和卸载 MOD': ['可以直接安裝和解除安裝 MOD', 'MODs can be installed and removed directly', 'MOD를 바로 설치·제거할 수 있습니다', 'MOD を直接インストール・削除できます'],
  '需要管理员权限才能写入': ['需要管理員權限才能寫入', 'Administrator rights are required to write', '쓰려면 관리자 권한이 필요합니다', '書き込むには管理者権限が必要です'],
  '可写': ['可寫', 'Writable', '쓰기 가능', '書き込み可'],
  '不可写': ['不可寫', 'Not writable', '쓰기 불가', '書き込み不可'],
  '在 MOD 安装中查看': ['在 MOD 安裝中檢視', 'View in MOD Install', 'MOD 설치에서 보기', 'MOD インストールで表示'],
  // 缺依赖/冲突标签的跳转：目标是**依赖的**仓库，不是这个 MOD 自己的仓库
  '在 MOD 安装中查看依赖': [
    '在 MOD 安裝中檢視依賴',
    'View dependency in MOD Install',
    'MOD 설치에서 의존성 보기',
    'MOD インストールで依存関係を表示',
  ],
  '没有可安装的来源': [
    '沒有可安裝的來源',
    'No installable source',
    '설치 가능한 소스 없음',
    'インストール可能なソースがありません',
  ],
  // 关系标签可点时的补充说明：点了会在本页高亮目标仓库
  '点击定位到该仓库': [
    '點擊定位到該倉庫',
    'Click to locate that repository',
    '클릭하면 해당 저장소로 이동합니다',
    'クリックでそのリポジトリへ移動します',
  ],
  '在 MOD 管理中查看': ['在 MOD 管理中檢視', 'View in MOD Manager', 'MOD 관리에서 보기', 'MOD マネージャーで表示'],
  '在页面内定位': ['在頁面內定位', 'Locate on this page', '페이지에서 찾기', 'このページ内で表示'],
  // MOD 管理工具栏与配置编辑器
  '打开所在文件夹并定位': ['開啟所在資料夾並定位', 'Reveal in folder', '폴더에서 위치 열기', 'フォルダーで表示'],
  '查看日志': ['檢視日誌', 'View logs', '로그 보기', 'ログを表示'],
  '打开目录': ['開啟目錄', 'Open folder', '폴더 열기', 'フォルダーを開く'],
  '打开': ['開啟', 'Open', '열기', '開く'],
  // DIYLevel 自定义关卡框架（本质是 MOD，但承担框架角色）
  'DIYLevel 框架': ['DIYLevel 框架', 'DIYLevel framework', 'DIYLevel 프레임워크', 'DIYLevel フレームワーク'],
  'DIYLevel 框架管理': ['DIYLevel 框架管理', 'DIYLevel framework', 'DIYLevel 프레임워크 관리', 'DIYLevel フレームワーク管理'],
  '地图集': ['地圖集', 'Map packs', '맵 팩', 'マップパック'],
  '安装来源': ['安裝來源', 'Install source', '설치 소스', 'インストール元'],
  '未找到可在线安装的仓库': [
    '找不到可線上安裝的倉庫',
    'No repository can install it online',
    '온라인 설치가 가능한 저장소가 없습니다',
    'オンラインインストールできるリポジトリがありません',
  ],
  '打开地图目录': ['開啟地圖目錄', 'Open map folder', '맵 폴더 열기', 'マップフォルダーを開く'],
  '无法在线安装：MOD 安装页没有声明此 GUID 的仓库，请手动安装后回到这里管理。': [
    '無法線上安裝：MOD 安裝頁沒有宣告此 GUID 的倉庫，請手動安裝後回到這裡管理。',
    'Not available online: no repository declares this GUID. Install it manually, then manage it here.',
    '온라인 설치 불가: 이 GUID를 선언한 저장소가 없습니다. 직접 설치한 뒤 여기로 돌아와 관리하세요.',
    'オンラインインストール不可：この GUID を宣言したリポジトリがありません。手動でインストールしてからここで管理してください。',
  ],
  '安装后才能加载自制地图': [
    '安裝後才能載入自製地圖',
    'Custom maps load only after installing this',
    '설치해야 커스텀 맵을 불러올 수 있습니다',
    'インストール後に自作マップを読み込めます',
  ],
  '无法在线安装 DIYLevel，请手动安装后在这里管理': [
    '無法線上安裝 DIYLevel，請手動安裝後在這裡管理',
    'DIYLevel is not available online. Install it manually, then manage it here.',
    'DIYLevel은 온라인 설치가 불가능합니다. 직접 설치한 뒤 여기에서 관리하세요.',
    'DIYLevel はオンラインインストールできません。手動でインストールしてからここで管理してください。',
  ],
  '卸载 DIYLevel 框架': ['解除安裝 DIYLevel 框架', 'Uninstall DIYLevel framework', 'DIYLevel 프레임워크 제거', 'DIYLevel フレームワークをアンインストール'],
  '卸载前会自动备份整个 DIYLevel 目录。其中的自制地图文件会一并移入回收站。继续？': [
    '解除安裝前會自動備份整個 DIYLevel 目錄。其中的自製地圖檔案會一併移入資源回收筒。繼續？',
    'The whole DIYLevel folder is backed up before uninstalling. Custom maps inside are moved to the recycle bin too. Continue?',
    '제거 전에 DIYLevel 폴더 전체를 자동으로 백업합니다. 안의 커스텀 맵도 휴지통으로 이동합니다. 계속할까요?',
    'アンインストール前に DIYLevel フォルダー全体を自動でバックアップします。中の自作マップもゴミ箱へ移動します。続行しますか？',
  ],
  '正在备份并卸载 DIYLevel…': [
    '正在備份並解除安裝 DIYLevel…',
    'Backing up and uninstalling DIYLevel…',
    'DIYLevel 백업 및 제거 중…',
    'DIYLevel をバックアップしてアンインストール中…',
  ],
  '正在备份 DIYLevel 目录…': [
    '正在備份 DIYLevel 目錄…',
    'Backing up the DIYLevel folder…',
    'DIYLevel 폴더 백업 중…',
    'DIYLevel フォルダーをバックアップ中…',
  ],
  '恢复 DIYLevel 备份': [
    '還原 DIYLevel 備份',
    'Restore DIYLevel backup',
    'DIYLevel 백업 복원',
    'DIYLevel バックアップを復元',
  ],
  '当前的 DIYLevel 目录会被删除，然后还原成这个备份的内容。继续？': [
    '目前的 DIYLevel 目錄會被刪除，然後還原成這個備份的內容。繼續？',
    'The current DIYLevel folder will be deleted, then replaced with this backup. Continue?',
    '현재 DIYLevel 폴더를 삭제한 뒤 이 백업 내용으로 되돌립니다. 계속할까요?',
    '現在の DIYLevel フォルダーを削除してから、このバックアップの内容に戻します。続行しますか？',
  ],
  '正在恢复 DIYLevel 备份…': [
    '正在還原 DIYLevel 備份…',
    'Restoring DIYLevel backup…',
    'DIYLevel 백업 복원 중…',
    'DIYLevel バックアップを復元中…',
  ],
  '这个备份会被永久删除，无法恢复。继续？': [
    '這個備份會被永久刪除，無法還原。繼續？',
    'This backup will be deleted permanently. Continue?',
    '이 백업은 영구 삭제되며 되돌릴 수 없습니다. 계속할까요?',
    'このバックアップは完全に削除され、元に戻せません。続行しますか？',
  ],
  'DIYLevel 框架已启用': ['DIYLevel 框架已啟用', 'DIYLevel framework enabled', 'DIYLevel 프레임워크 활성화됨', 'DIYLevel フレームワークを有効にしました'],
  'DIYLevel 框架已禁用': ['DIYLevel 框架已禁用', 'DIYLevel framework disabled', 'DIYLevel 프레임워크 비활성화됨', 'DIYLevel フレームワークを無効にしました'],
  '尚未安装 DIYLevel 框架': ['尚未安裝 DIYLevel 框架', 'DIYLevel framework is not installed', 'DIYLevel 프레임워크가 설치되어 있지 않습니다', 'DIYLevel フレームワークがインストールされていません'],
  '已打开 DIYLevel 目录': ['已開啟 DIYLevel 目錄', 'Opened the DIYLevel folder', 'DIYLevel 폴더를 열었습니다', 'DIYLevel フォルダーを開きました'],
  '尚未安装 DIYLevel 框架，或安装结构与预期不同': [
    '尚未安裝 DIYLevel 框架，或安裝結構與預期不同',
    'DIYLevel is not installed, or its layout differs from the expected one.',
    'DIYLevel이 설치되어 있지 않거나 설치 구조가 예상과 다릅니다.',
    'DIYLevel がインストールされていないか、構成が想定と異なります。',
  ],
  // 工具页：缓存维护
  '缓存': ['快取', 'Cache', '캐시', 'キャッシュ'],
  '清除头像缓存': ['清除頭像快取', 'Clear avatar cache', '아바타 캐시 지우기', 'アバターキャッシュを消去'],
  '删除已下载的仓库头像；下次进入 MOD 安装页会重新获取': [
    '刪除已下載的倉庫頭像；下次進入 MOD 安裝頁會重新取得',
    'Deletes downloaded repository avatars. They will be fetched again the next time you open the MOD install page.',
    '다운로드한 저장소 아바타를 삭제합니다. 다음에 MOD 설치 페이지를 열면 다시 가져옵니다.',
    'ダウンロード済みのリポジトリのアバターを削除します。次に MOD インストールページを開いたときに再取得します。',
  ],
  '已清除 {count} 个头像缓存': [
    '已清除 {count} 個頭像快取',
    'Cleared {count} cached avatar(s)',
    '아바타 캐시 {count}개를 지웠습니다',
    'アバターキャッシュを {count} 件消去しました',
  ],
  '安装 MOD': ['安裝 MOD', 'Install MOD', 'MOD 설치', 'MOD をインストール'],
  '安装详情': ['安裝詳情', 'Install details', '설치 세부 정보', 'インストールの詳細'],
  '本地文件': ['本機檔案', 'Local file', '로컬 파일', 'ローカルファイル'],
  '在线直链': ['線上直連', 'Direct URL', '직접 링크', '直リンク'],
  '依赖': ['相依', 'Dependency', '의존성', '依存ファイル'],
  '本地安装': ['本機安裝', 'Local install', '로컬 설치', 'ローカルインストール'],
  '单个 MOD': ['單一 MOD', 'Single MOD', '단일 MOD', '単一 MOD'],
  'MOD 包': ['MOD 包', 'MOD package', 'MOD 패키지', 'MOD パッケージ'],
  '把 .dll 或 .zip 拖到这里，或点击选择文件': ['把 .dll 或 .zip 拖到這裡，或點擊選擇檔案', 'Drop a .dll or .zip here, or click to choose a file', '.dll 또는 .zip을 여기로 끌어오거나 클릭해 선택하세요', '.dll または .zip をここにドロップするか、クリックして選択します'],
  '下载并解析': ['下載並解析', 'Download and analyze', '다운로드 후 분석', 'ダウンロードして解析'],
  '重新选择': ['重新選擇', 'Choose again', '다시 선택', '選び直す'],
  '立即安装': ['立即安裝', 'Install now', '지금 설치', '今すぐインストール'],
  '这个压缩包里没有 DLL 文件': ['這個壓縮檔裡沒有 DLL 檔案', 'This archive contains no DLL files', '이 압축 파일에는 DLL이 없습니다', 'このアーカイブに DLL は含まれていません'],
  '只支持 .dll 或 .zip 文件': ['只支援 .dll 或 .zip 檔案', 'Only .dll or .zip files are supported', '.dll 또는 .zip 파일만 지원합니다', '.dll または .zip のみ対応しています'],
  '{count} 个 DLL · {mods} 个 MOD · {deps} 个依赖': ['{count} 個 DLL · {mods} 個 MOD · {deps} 個相依', '{count} DLLs · {mods} MODs · {deps} dependencies', 'DLL {count}개 · MOD {mods}개 · 의존성 {deps}개', 'DLL {count} 件 · MOD {mods} 件 · 依存 {deps} 件'],
  '下载并安装': ['下載並安裝', 'Download and install', '다운로드 후 설치', 'ダウンロードしてインストール'],
  '选择本地的 .dll 或 .zip 文件，按标准布局安装到厨房目录。': ['選擇本機的 .dll 或 .zip 檔案，依標準版面配置安裝到廚房目錄。', 'Pick a local .dll or .zip file; it is installed into the kitchen folder using the standard layout rules.', '로컬 .dll 또는 .zip 파일을 선택하면 표준 배치 규칙으로 게임 폴더에 설치합니다', 'ローカルの .dll または .zip を選ぶと、標準レイアウト規則でゲームフォルダーにインストールします'],
  '粘贴 zip 或 DLL 的下载直链，由管理器下载并安装。': ['貼上 zip 或 DLL 的下載直連，由管理器下載並安裝。', 'Paste a direct download URL for a zip or DLL; the manager downloads and installs it.', 'zip 또는 DLL의 직접 다운로드 링크를 붙여넣으면 관리자가 내려받아 설치합니다', 'zip または DLL の直リンクを貼り付けると、マネージャーがダウンロードしてインストールします'],
  '请输入 zip 或 DLL 的直链网址': ['請輸入 zip 或 DLL 的直連網址', 'Enter a direct URL to a zip or DLL', 'zip 또는 DLL 직접 링크를 입력하세요', 'zip または DLL の直リンクを入力してください'],
  '删除 MOD 包': ['刪除 MOD 包', 'Delete MOD package', 'MOD 패키지 삭제', 'MOD パッケージを削除'],
  '「{name}」的 {count} 个文件将被整包移入回收站，可在回收站中恢复。继续？': ['「{name}」的 {count} 個檔案將被整包移入資源回收筒，可在資源回收筒中還原。繼續？', 'All {count} files of "{name}" will be moved to the recycle bin and can be restored. Continue?', '"{name}"의 파일 {count}개를 패키지째 휴지통으로 옮깁니다. 복원할 수 있습니다. 계속할까요?', '「{name}」の {count} ファイルをまとめてゴミ箱へ移動します（復元可能）。続行しますか？'],
  '「{name}」的 {count} 个文件将被直接删除，无法恢复。继续？': ['「{name}」的 {count} 個檔案將被直接刪除，無法還原。繼續？', 'All {count} files of "{name}" will be deleted permanently. This cannot be undone. Continue?', '"{name}"의 파일 {count}개를 바로 삭제합니다. 복원할 수 없습니다. 계속할까요?', '「{name}」の {count} ファイルを直接削除します。元に戻せません。続行しますか？'],
  '配置编辑': ['設定編輯', 'Edit config', '설정 편집', '設定を編集'],
  'API 凭据': ['API 憑證', 'API credentials', 'API 자격 증명', 'API 認証情報'],
  '访问令牌': ['存取權杖', 'Access tokens', '액세스 토큰', 'アクセストークン'],
  '配置后 API 请求直接使用 Token，避免匿名限流': ['設定後 API 請求直接使用 Token，避免匿名限流', 'With a token, API requests use it directly instead of the anonymous quota', '토큰을 설정하면 API 요청이 익명 대신 토큰을 사용합니다', 'トークンを設定すると API リクエストは匿名ではなくトークンを使用します'],
  '使用 Authorization: Bearer 请求头': ['使用 Authorization: Bearer 請求頭', 'Sent as an Authorization: Bearer header', 'Authorization: Bearer 헤더로 전송됩니다', 'Authorization: Bearer ヘッダーで送信します'],
  '使用 access_token 请求头': ['使用 access_token 請求頭', 'Sent as an access_token header', 'access_token 헤더로 전송됩니다', 'access_token ヘッダーで送信します'],
  '已配置': ['已設定', 'Configured', '설정됨', '設定済み'],
  '已保存，留空则不修改': ['已儲存，留空則不修改', 'Saved — leave blank to keep', '저장됨 — 비워 두면 유지', '保存済み — 空欄なら変更しません'],
  '粘贴 Gitee 私人令牌': ['貼上 Gitee 私人權杖', 'Paste your Gitee private token', 'Gitee 개인 토큰을 붙여넣으세요', 'Gitee のプライベートトークンを貼り付け'],
  '清除全部': ['清除全部', 'Clear all', '모두 지우기', 'すべて削除'],
  '已清除': ['已清除', 'Cleared', '지워짐', '削除しました'],
  '配置 Token 后，API 请求直接使用 Token；Token 自身被限流时才回退到代理。': ['設定 Token 後，API 請求直接使用 Token；Token 自身被限流時才回退到代理。', 'With tokens set, requests use them directly and only fall back to proxies when the token itself is rate limited.', '토큰을 설정하면 요청에 직접 사용하고, 토큰이 제한될 때만 프록시로 전환합니다.', 'トークンを設定すると直接使用し、トークンが制限された場合のみプロキシに切り替えます。'],
  '说明': ['說明', 'Details', '설명', '説明'],
  '下载安装': ['下載安裝', 'Download', '다운로드 및 설치', 'ダウンロードとインストール'],
  '未保存': ['未儲存', 'Unsaved', '저장 안 됨', '未保存'],
  '有未保存的修改，已取消': ['有未儲存的修改，已取消', 'Unsaved changes were discarded', '저장하지 않은 변경 사항이 있습니다', '未保存の変更があります'],
  '先刷新源文档，再并行刷新各仓库': ['先重新整理來源文件，再並行整理各倉庫', 'Reload source documents, then refresh repositories in parallel', '소스 문서를 먼저 새로 고친 뒤 저장소를 병렬로 새로 고침', 'ソース文書を更新してから各リポジトリを並列更新'],
  '仓库源刷新失败，请稍后重试': ['倉庫源重新整理失敗，請稍後重試', 'Source refresh failed. Try again later.', '소스 새로 고침 실패, 잠시 후 다시 시도하세요', 'ソースの更新に失敗しました。後で再試行してください'],
  '中国大陆地区': ['中國大陸地區', 'Mainland China', '중국 본토', '中国本土'],
  '国际地区': ['國際地區', 'International', '해외', '海外'],
  '许可证': ['授權條款', 'License', '라이선스', 'ライセンス'],
  '最后更新': ['最後更新', 'Updated', '최종 업데이트', '最終更新'],
  '标识': ['識別碼', 'Identifier', '식별자', '識別子'],
  '版本绑定': ['版本綁定', 'Version binding', '버전 바인딩', 'バージョン紐付け'],
  '源码': ['原始碼', 'Source', '소스', 'ソース'],
  'Release': ['Release', 'Release', 'Release', 'Release'],
  '该仓库没有备注': ['該倉庫沒有備註', 'This repository has no note', '이 저장소에는 메모가 없습니다', 'このリポジトリにはメモがありません'],
  '正在加载仓库说明': ['正在載入倉庫說明', 'Loading repository README', '저장소 README 불러오는 중', 'リポジトリの説明を読み込み中'],
  '没有匹配的仓库': ['沒有符合的倉庫', 'No matching repositories', '일치하는 저장소가 없습니다', '一致するリポジトリがありません'],
  '部分启用': ['部分啟用', 'Partially enabled', '일부 활성', '一部有効'],
  '可更新': ['可更新', 'Update', '업데이트 가능', '更新あり'],
  '刷新失败': ['重新整理失敗', 'Refresh failed', '새로 고침 실패', '更新失敗'],
  '已最新': ['已最新', 'Up to date', '최신 상태', '最新です'],
  '低版本': ['低版本', 'Older version', '이전 버전', '旧バージョン'],
  '版本未知': ['版本未知', 'Version unknown', '버전 미확인', 'バージョン不明'],
  '检测中': ['檢測中', 'Checking', '확인 중', '確認中'],
  '正在检测更新': ['正在檢查更新', 'Checking for updates', '업데이트 확인 중', '更新を確認中'],
  '排序': ['排序', 'Sort', '정렬', '並び替え'],
  '重新扫描本地 MOD': ['重新掃描本機 MOD', 'Rescan local MODs', '로컬 MOD 다시 검사', 'ローカル MOD を再スキャン'],
  '已重新扫描本地 MOD': ['已重新掃描本機 MOD', 'Local MODs rescanned', '로컬 MOD를 다시 검사했습니다', 'ローカル MOD を再スキャンしました'],
  '准备中': ['準備中', 'Preparing', '준비 중', '準備中'],
  '准备中…': ['準備中…', 'Preparing…', '준비 중…', '準備中…'],
  '正在连接下载源…': ['正在連線下載來源…', 'Connecting to download source…', '다운로드 소스에 연결 중…', 'ダウンロード元に接続中…'],
  '正在切换更快的源…': ['正在切換更快的來源…', 'Switching to a faster source…', '더 빠른 소스로 전환 중…', 'より速いソースに切り替え中…'],
  '正在下载': ['正在下載', 'Downloading', '다운로드 중', 'ダウンロード中'],
  '已完成': ['已完成', 'Completed', '완료', '完了'],
  '{count} 源竞速': ['{count} 源競速', '{count} sources racing', '{count}개 소스 경쟁', '{count} ソース競争'],
  '第 {current}/{total} 次尝试': ['第 {current}/{total} 次嘗試', 'Attempt {current}/{total}', '{current}/{total}번째 시도', '{current}/{total} 回目の試行'],
  '选择版本': ['選擇版本', 'Select version', '버전 선택', 'バージョンを選択'],
  '选择文件': ['選擇檔案', 'Select file', '파일 선택', 'ファイルを選択'],
  '大小': ['大小', 'Size', '크기', 'サイズ'],
  '下载 {count} 次': ['下載 {count} 次', '{count} downloads', '{count}회 다운로드', '{count} 回ダウンロード'],
  '另有 {count} 个匹配文件': ['另有 {count} 個符合檔案', '{count} more matching files', '{count}개 추가 일치 파일', '他に {count} 件の一致ファイル'],
  '该版本没有命中资产规则的文件': ['該版本沒有符合資產規則的檔案', 'No asset in this release matches the asset rules', '이 버전에는 자산 규칙과 일치하는 파일이 없습니다', 'このバージョンにアセット規則と一致するファイルがありません'],
  '仓库文件模式直接下载该文件': ['倉庫檔案模式直接下載該檔案', 'Repository-file mode downloads this file directly', '저장소 파일 모드는 이 파일을 바로 받습니다', 'リポジトリファイルモードはこのファイルを直接ダウンロードします'],
  '当前渠道没有社区仓库': ['目前渠道沒有社群倉庫', 'No community repositories in this channel', '이 채널에는 커뮤니티 저장소가 없습니다', 'このチャンネルにはコミュニティリポジトリがありません'],
  // MOD 详情（本地管理）
  '标题': ['標題', 'Title', '제목', 'タイトル'],
  '产品名': ['產品名', 'Product name', '제품명', '製品名'],
  '原始文件名': ['原始檔名', 'Original filename', '원본 파일명', '元のファイル名'],
  '文件': ['檔案', 'File', '파일', 'ファイル'],
  '插件名': ['外掛名', 'Plugin name', '플러그인 이름', 'プラグイン名'],
  '作者': ['作者', 'Author', '제작자', '作者'],
  '版权': ['版權', 'Copyright', '저작권', '著作権'],
  '版本来源': ['版本來源', 'Version source', '버전 출처', 'バージョン取得元'],
  '启用状态': ['啟用狀態', 'Enabled', '활성 상태', '有効状態'],
  '配置文件': ['設定檔', 'Config file', '설정 파일', '設定ファイル'],
  '重复 GUID': ['重複 GUID', 'Duplicate GUID', '중복 GUID', '重複 GUID'],
  '已启用': ['已啟用', 'Enabled', '활성', '有効'],
  '已安装': ['已安裝', 'Installed', '설치됨', 'インストール済み'],
  '有': ['有', 'Yes', '있음', 'あり'],
  '无': ['無', 'No', '없음', 'なし'],
  '是': ['是', 'Yes', '예', 'はい'],
  '否': ['否', 'No', '아니요', 'いいえ'],
  '状态标签': ['狀態標籤', 'Status labels', '상태 라벨', 'ステータスラベル'],
  '提交日志': ['提交日誌', 'Release notes', '릴리스 노트', 'リリースノート'],
  '预发布版本': ['預發布版本', 'Pre-release version', '프리릴리스 버전', 'プレリリース版'],
  '此版本为预览版本，可能存在未知问题，是否继续安装？': ['此版本為預覽版本，可能存在未知問題，是否繼續安裝？', 'This is a pre-release build and may have unknown issues. Continue installing?', '이 버전은 프리릴리스이며 알려지지 않은 문제가 있을 수 있습니다. 계속 설치할까요?', 'このバージョンはプレリリースのため、未知の問題が含まれる可能性があります。インストールを続行しますか？'],
  '继续安装': ['繼續安裝', 'Continue installing', '계속 설치', 'インストールを続行'],
  '该版本没有提交日志': ['該版本沒有提交日誌', 'This release has no notes', '이 버전에는 릴리스 노트가 없습니다', 'このバージョンにはリリースノートがありません'],
  '绑定仓库': ['綁定倉庫', 'Bound repository', '연결된 저장소', '紐付けリポジトリ'],
  '删除': ['刪除', 'Delete', '삭제', '削除'],
  // MOD 改名（多彩名字）
  '输入名字': ['輸入名字', 'Enter name', '이름 입력', '名前を入力'],
  '支持任意字符，例如中文、英文与符号': ['支援任意字元，例如中文、英文與符號', 'Any characters, e.g. Chinese, English, symbols', '모든 문자 사용 가능 (예: 중국어, 영어, 기호)', '任意の文字（中文・英字・記号）に対応'],
  '在这里输入要修改的名字': ['在這裡輸入要修改的名字', 'Enter the name you want to restyle', '꾸밀 이름을 입력하세요', '装飾したい名前を入力'],
  '请输入名字': ['請輸入名字', 'Please enter a name', '이름을 입력하세요', '名前を入力してください'],
  '改多彩名': ['改多彩名', 'Colorful name', '다채로운 이름', 'カラフル名'],
  '开始自定义': ['開始自訂', 'Start customizing', '커스터마이즈 시작', 'カスタマイズ開始'],
  '重新输入': ['重新輸入', 'Start over', '다시 입력', '入力し直す'],
  '选择要修改的文字': ['選擇要修改的文字', 'Select text to edit', '수정할 텍스트 선택', '編集する文字を選択'],
  '拖动选中预览里的字符，再应用颜色或字号': ['拖曳選取預覽裡的字符，再套用顏色或字號', 'Drag-select characters in the preview, then apply color or size', '미리보기에서 글자를 드래그해 선택한 뒤 색상이나 크기를 적용하세요', 'プレビュー内の文字をドラッグ選択して色やサイズを適用します'],
  '请先选择要修改的文字': ['請先選擇要修改的文字', 'Select some text first', '먼저 텍스트를 선택하세요', '先に文字を選択してください'],
  '选择颜色': ['選擇顏色', 'Choose color', '색상 선택', '色を選択'],
  '更多颜色': ['更多顏色', 'More colors', '추가 색상', 'その他の色'],
  '字号增量': ['字號增量', 'Size delta', '글자 크기 증감', 'サイズ増分'],
  '重置样式': ['重設樣式', 'Reset style', '스타일 초기화', 'スタイルをリセット'],
  '生成的名字代码': ['生成的名字代碼', 'Generated name code', '생성된 이름 코드', '生成された名前コード'],
  '名字代码超过限制': ['名字代碼超過限制', 'Name code is over the limit', '이름 코드가 제한을 초과했습니다', '名前コードが上限を超えています'],
  '超出 238 字节限制': ['超出 238 位元組限制', 'Exceeds the 238-byte limit', '238바이트 제한 초과', '238 バイト上限を超過'],
  '正在推送到 街机MOD': ['正在推送到 主機實用工具', 'Pushing to HostUtilities', '호스트 유틸리티로 보내는 중', 'ホストユーティリティへ送信中'],
  '全部等级': ['全部等級', 'All levels', '모든 레벨', 'すべてのレベル'],
  '{count} 行': ['{count} 行', '{count} lines', '{count}줄', '{count} 行'],
  '复制代码': ['複製代碼', 'Copy code', '코드 복사', 'コードをコピー'],
  '已复制，可粘贴到街机MOD 的多彩名字菜单': ['已複製，可貼到主機實用工具的多彩名字選單', 'Copied. Paste it into HostUtilities\' colorful-name menu.', '복사되었습니다. 호스트 유틸리티의 다채로운 이름 메뉴에 붙여넣으세요', 'コピーしました。ホストユーティリティのカラフルネームメニューに貼り付けられます'],
  '推送到 街机MOD': ['推送到 主機實用工具', 'Push To HostUtilities', '호스트 유틸리티로 보내기', 'ホストユーティリティへ送信'],
  '已推送到 街机MOD': ['已推送到 主機實用工具', 'Pushed to HostUtilities', '호스트 유틸리티로 보냈습니다', 'ホストユーティリティに送信しました'],
  '复制后粘贴到游戏内街机MOD 的多彩名字菜单，或直接推送': ['複製後貼到遊戲內主機實用工具的多彩名字選單，或直接推送', 'Copy and paste into HostUtilities\' colorful-name menu in game, or push directly', '복사해 게임 내 호스트 유틸리티의 다채로운 이름 메뉴에 붙여넣거나 바로 보내세요', 'コピーしてゲーム内ホストユーティリティのカラフルネームメニューに貼り付けるか、直接送信します'],
  '{count} 个': ['{count} 個', '{count}', '{count}개', '{count} 件'],
  // 通用动作与状态
  '保存并验证': ['儲存並驗證', 'Save and verify', '저장 후 검증', '保存して検証'],
  '恢复默认': ['還原預設', 'Reset', '기본값 복원', '既定に戻す'],
  '已保存': ['已儲存', 'Saved', '저장됨', '保存しました'],
  '游戏运行中': ['遊戲執行中', 'Game running', '게임 실행 중', 'ゲーム実行中'],
  '取消下载': ['取消下載', 'Cancel', '다운로드 취소', 'ダウンロード中止'],
  '等待大小': ['等待大小', 'Waiting for size', '크기 대기', 'サイズ待ち'],
  '代理': ['代理', 'Proxy', '프록시', 'プロキシ'],
  '直连': ['直連', 'Direct', '직접 연결', '直接'],
  '这个目录不存在或缺少游戏文件': ['這個目錄不存在或缺少遊戲檔案', 'The folder is missing or has no game files', '폴더가 없거나 게임 파일이 없습니다', 'フォルダーがないか、ゲームファイルがありません'],
  '保存后自动验证。': ['儲存後自動驗證。', 'Verified automatically after saving.', '저장 후 자동으로 검증합니다.', '保存後に自動検証します。'],
  '文件不存在': ['檔案不存在', 'File not found', '파일 없음', 'ファイルがありません'],
  '游戏目录已变化，请重新添加模组': ['遊戲目錄已變更，請重新新增模組', 'Game directory changed; re-add the mods', '게임 폴더가 변경되었습니다. 모드를 다시 추가하세요', 'ゲームフォルダーが変更されました。MODを再追加してください'],
} as const satisfies Record<string, Entry>

export type TranslationKey = keyof typeof entries

const INDEX: Record<Locale, number> = { 'zh-CN': -1, 'zh-TW': 0, en: 1, ko: 2, ja: 3 }

/** 取翻译；支持 {placeholder} 替换。未知 key 原样返回，便于渐进迁移。 */
export function t(key: TranslationKey | (string & {}), params?: Record<string, string | number>): string {
  const entry = (entries as Record<string, Entry>)[key]
  const table = INDEX[locale.value]
  let text = table === -1 || !entry ? String(key) : entry[table] || String(key)
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.split(`{${name}}`).join(String(value))
    }
  }
  return text
}

/** Naive UI 组件文案。 */
export const naiveLocale = computed(() => ({
  'zh-CN': zhCN, 'zh-TW': zhTW, en: enUS, ko: koKR, ja: jaJP,
}[locale.value]))
export const naiveDateLocale = computed(() => ({
  'zh-CN': dateZhCN, 'zh-TW': dateZhTW, en: dateEnUS, ko: dateKoKR, ja: dateJaJP,
}[locale.value]))

/** v2 语言选择器标签（随当前语言显示本族名）。 */
export const currentLocaleLabel = computed(() => ({
  'zh-CN': '简体中文', 'zh-TW': '繁體中文', en: 'English', ko: '한국어', ja: '日本語',
}[locale.value]))

export function setLocale(value: Locale) {
  locale.value = value
}

/**
 * 从多语言映射里按当前界面语言取文案，缺失时回退。
 *
 * 回退顺序：完整标记 → `zh-CN` → 任意第一个非空值 → `fallback`。
 * 取「任意第一个」是为了兼容源文档只写了一两种语言的情况——
 * 显示一个外语名称总比显示空白好。
 *
 * 放在 i18n 模块是因为它天然依赖 `locale.value`，且要多处共用
 * （仓库名、分组名、包名快照、回收站条目）。
 */
export function pickI18n(
  map: Record<string, string> | undefined | null,
  fallback: string,
): string {
  if (!map) return fallback
  const exact = map[locale.value]
  if (exact && exact.trim()) return exact
  const chinese = map['zh-CN']
  if (chinese && chinese.trim()) return chinese
  for (const value of Object.values(map)) {
    if (value && value.trim()) return value
  }
  return fallback
}
