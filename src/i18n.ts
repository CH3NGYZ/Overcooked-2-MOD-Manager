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

/** 五语表：key = 简体中文原文。新增文案在这里加一行即可。 */
const entries = {
  // 侧栏与标题栏
  '总览': ['總覽', 'Overview', '개요', '概要'],
  '框架管理': ['框架管理', 'Framework', '프레임워크', 'フレームワーク'],
  'MOD 安装': ['MOD 安裝', 'MOD Install', 'MOD 설치', 'MOD インストール'],
  'MOD 管理': ['MOD 管理', 'MOD Manager', 'MOD 관리', 'MOD マネージャー'],
  '工具': ['工具', 'Tools', '도구', 'ツール'],
  '设置': ['設定', 'Settings', '설정', '設定'],
  '游戏模组管理器': ['遊戲模組管理器', 'Game mod manager', '게임 모드 관리자', 'ゲームMODマネージャー'],
  '界面语言': ['介面語言', 'Language', '인터페이스 언어', '表示言語'],
  '界面缩放': ['介面縮放', 'UI scale', '인터페이스 배율', '表示倍率'],
  '主题模式': ['主題模式', 'Theme', '테마', 'テーマ'],
  '深色': ['深色', 'Dark', '다크', 'ダーク'],
  '浅色': ['淺色', 'Light', '라이트', 'ライト'],
  '主题色': ['主題色', 'Accent', '테마 색', 'テーマカラー'],
  '中性': ['中性', 'Neutral', '뉴트럴', 'ニュートラル'],
  '蓝色': ['藍色', 'Blue', '블루', 'ブルー'],
  '绿色': ['綠色', 'Green', '그린', 'グリーン'],
  '紫色': ['紫色', 'Purple', '퍼플', 'パープル'],
  '红色': ['紅色', 'Red', '레드', 'レッド'],
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
  '尚未检测到游戏目录': ['尚未偵測到遊戲目錄', 'No game directory detected', '게임 폴더를 찾지 못했습니다', 'ゲームフォルダー未検出'],
  '暂无可更新内容': ['暫無可更新內容', 'Nothing to update', '업데이트할 항목 없음', '更新はありません'],
  '有可更新的 MOD': ['有可更新的 MOD', 'MOD updates available', '업데이트 가능한 MOD 있음', '更新可能なMODがあります'],

  // 框架
  'BepInEx 框架': ['BepInEx 框架', 'BepInEx framework', 'BepInEx 프레임워크', 'BepInEx フレームワーク'],
  '不完整': ['不完整', 'Incomplete', '불완전', '不完全'],
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
  '刷新失败': ['重新整理失敗', 'Refresh failed', '새로 고침 실패', '更新失敗'],
  '刷新': ['重新整理', 'Refresh', '새로 고침', '更新'],
  '安装': ['安裝', 'Install', '설치', 'インストール'],
  '版本': ['版本', 'Versions', '버전', 'バージョン'],
  '未刷新': ['未重新整理', 'Not refreshed', '새로 고침 안 됨', '未更新'],
  '{count} 个版本': ['{count} 個版本', '{count} versions', '{count}개 버전', '{count} バージョン'],
  '预发布': ['預發布', 'Pre-release', '프리릴리스', 'プレリリース'],
  '安装此版本': ['安裝此版本', 'Install this version', '이 버전 설치', 'このバージョンをインストール'],
  '尚无缓存的 Release，请先刷新。': ['尚無快取的 Release，請先重新整理。', 'No cached releases. Refresh first.', '캐시된 Release가 없습니다. 먼저 새로 고침하세요.', 'キャッシュされたReleaseがありません。先に更新してください。'],
  '游戏运行中，已禁止安装。请先关闭游戏。': ['遊戲執行中，已禁止安裝。請先關閉遊戲。', 'The game is running. Close it before installing.', '게임 실행 중에는 설치할 수 없습니다. 먼저 게임을 종료하세요.', 'ゲーム実行中はインストールできません。先にゲームを終了してください。'],
  '获取到 {count} 个 Release': ['取得 {count} 個 Release', 'Fetched {count} releases', 'Release {count}개를 가져왔습니다', 'Release を {count} 件取得しました'],

  // MOD 管理
  '共 {count} 个 · 启用 {enabled} · 可更新 {updates} · 排序：{sort}': ['共 {count} 個 · 啟用 {enabled} · 可更新 {updates} · 排序：{sort}', '{count} total · {enabled} enabled · {updates} updates · Sort: {sort}', '총 {count}개 · 활성 {enabled} · 업데이트 {updates} · 정렬: {sort}', '合計 {count} · 有効 {enabled} · 更新 {updates} · 並び: {sort}'],
  '按名称': ['依名稱', 'By name', '이름순', '名前順'],
  '按版本': ['依版本', 'By version', '버전순', 'バージョン順'],
  '按启用': ['依啟用', 'By enabled', '활성순', '有効順'],
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
  '仓库不可用': ['倉庫不可用', 'Repository unavailable', '저장소 사용 불가', 'リポジトリ利用不可'],
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
  '立即检测更新': ['立即檢查更新', 'Check for updates', '지금 업데이트 확인', '今すぐ更新を確認'],
  '检查是否有新版本可用，检测到更新会弹出确认': ['檢查是否有新版本可用，偵測到更新會彈出確認', 'Check for a new version; a dialog appears when one is found', '새 버전이 있는지 확인합니다. 있으면 대화상자가 표시됩니다', '新しいバージョンを確認し、あればダイアログを表示します'],
  '检测更新': ['檢查更新', 'Check', '확인', '確認'],
  '更新设置': ['更新設定', 'Update settings', '업데이트 설정', '更新設定'],
  '自动检查与安装偏好': ['自動檢查與安裝偏好', 'Automatic check and install preferences', '자동 확인·설치 설정', '自動確認とインストールの設定'],
  '通用设置': ['一般設定', 'General', '일반 설정', '一般設定'],
  '启动时检查程序更新': ['啟動時檢查程式更新', 'Check for updates on startup', '시작 시 업데이트 확인', '起動時に更新を確認'],
  '每天首次启动时检查新版本': ['每天首次啟動時檢查新版本', 'Check once per day on first launch', '매일 첫 실행 시 확인합니다', '毎日最初の起動時に確認します'],
  '自动检测游戏路径': ['自動偵測遊戲路徑', 'Auto-detect game path', '게임 경로 자동 감지', 'ゲームパスを自動検出'],
  '启动与刷新时自动探测 Steam 库': ['啟動與重新整理時自動探測 Steam 庫', 'Probe Steam libraries on launch and refresh', '시작·새로 고침 시 Steam 라이브러리를 탐색합니다', '起動時と更新時にSteamライブラリを検出します'],
  '关于': ['關於', 'About', '정보', 'このアプリについて'],
  '当前路径': ['目前路徑', 'Current path', '현재 경로', '現在のパス'],
  '选择或输入目录路径': ['選擇或輸入目錄路徑', 'Choose or enter a folder path', '폴더 경로를 선택하거나 입력하세요', 'フォルダーパスを選択または入力'],
  '浏览…': ['瀏覽…', 'Browse…', '찾아보기…', '参照…'],
  '确认保存': ['確認儲存', 'Save', '저장', '保存'],
  '选择游戏目录': ['選擇遊戲目錄', 'Choose game directory', '게임 폴더 선택', 'ゲームフォルダーを選択'],
  '目录需包含 Overcooked!2.exe；保存后自动验证。': ['目錄需包含 Overcooked!2.exe；儲存後自動驗證。', 'The folder must contain Overcooked!2.exe; it is verified on save.', '폴더에 Overcooked!2.exe가 있어야 하며 저장 후 검증합니다.', 'Overcooked!2.exe を含む必要があります。保存時に検証します。'],
  '更新检查按地区走 Gitee / GitHub 双渠道；更新包经 minisign 签名校验后由 Tauri Updater 安装。': ['更新檢查按地區走 Gitee / GitHub 雙渠道；更新包經 minisign 簽名校驗後由 Tauri Updater 安裝。', 'Updates follow the regional Gitee/GitHub channels and are signature-checked before the Tauri Updater installs them.', '업데이트는 지역별 Gitee/GitHub 채널을 사용하며 minisign 서명 검증 후 Tauri Updater가 설치합니다.', '更新は地域別の Gitee／GitHub 経路を使用し、minisign 署名検証後に Tauri Updater がインストールします。'],
  '启动时检查程序更新（旧）': ['啟動時檢查程式更新', 'Check for updates on startup', '시작 시 업데이트 확인', '起動時に更新を確認'],

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
  '更新检查失败，可稍后重试': ['檢查更新失敗，可稍後重試', 'Update check failed; try again later', '업데이트 확인 실패. 나중에 다시 시도하세요', '更新確認に失敗しました。後で再試行してください'],
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
  '搜索作者 / MOD 名称 / 简介 / 分组': ['搜尋作者 / MOD 名稱 / 簡介 / 分組', 'Search author / MOD name / description / group', '작성자 / MOD 이름 / 설명 / 그룹 검색', '作者 / MOD名 / 説明 / グループを検索'],
  '刷新全部仓库源': ['重新整理全部倉庫源', 'Refresh all sources', '모든 소스 새로 고침', 'すべてのソースを更新'],
  '中国大陆地区': ['中國大陸地區', 'Mainland China', '중국 본토', '中国本土'],
  '国际地区': ['國際地區', 'International', '해외', '海外'],
  '许可证': ['授權條款', 'License', '라이선스', 'ライセンス'],
  '最后更新': ['最後更新', 'Updated', '최종 업데이트', '最終更新'],
  '标识': ['識別碼', 'Identifier', '식별자', '識別子'],
  '源码': ['原始碼', 'Source', '소스', 'ソース'],
  'Release': ['Release', 'Release', 'Release', 'Release'],
  '该仓库没有备注': ['該倉庫沒有備註', 'This repository has no note', '이 저장소에는 메모가 없습니다', 'このリポジトリにはメモがありません'],
  '正在加载仓库说明': ['正在載入倉庫說明', 'Loading repository README', '저장소 README 불러오는 중', 'リポジトリの説明を読み込み中'],
  '正在加载仓库说明…': ['正在載入倉庫說明…', 'Loading README…', 'README 불러오는 중…', 'README を読み込み中…'],
  '没有匹配的仓库': ['沒有符合的倉庫', 'No matching repositories', '일치하는 저장소가 없습니다', '一致するリポジトリがありません'],
  '部分启用': ['部分啟用', 'Partially enabled', '일부 활성', '一部有効'],
  '{count} 个可更新': ['{count} 個可更新', '{count} updatable', '{count}개 업데이트 가능', '{count} 件更新可能'],
  '排序': ['排序', 'Sort', '정렬', '並び替え'],
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
  'zh-CN': '文/简体中文', 'zh-TW': '文/繁體中文', en: '文/English', ko: '文/한국어', ja: '文/日本語',
}[locale.value]))

export function setLocale(value: Locale) {
  locale.value = value
}
