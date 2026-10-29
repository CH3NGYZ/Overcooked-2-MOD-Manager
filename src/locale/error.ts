import type { DomainMessages } from './types'

/**
 * 后端错误与状态 tag。
 *
 * 这些是后端**唯一**会返回给前端做展示的文案：后端只回 tag
 * （例如 `error.diyLevel.notRunning`），前端在这里翻译。
 *
 * 命名约定：`<对象>.<失败动作>`，不要写技术细节（"HTTP 500"）
 * 当 tag —— 那些应该落日志，用户看到的是可执行的建议。
 *
 * 参数用 `{}` 占位，由调用方传入。后端回参数时以 JSON 附带，
 * 前端把参数原样传给 `t()`。
 */
export const error: DomainMessages = {
  // ===== 多彩名字推送（原 commands.rs 里的中文字面量）=====
  'diyLevel.notRunning': {
    'zh-CN': '请打开游戏并确认 街机MOD 正在运行',
    'zh-TW': '請開啟遊戲並確認主機實用工具正在執行',
    en: 'Open the game and make sure HostUtilities is running',
    ko: '게임을 실행하고 호스트 유틸리티가 실행 중인지 확인하세요',
    ja: 'ゲームを起動し、ホストユーティリティが動作していることを確認してください',
  },
  'diyLevel.rejected': {
    'zh-CN': '街机MOD 拒绝了改名请求，请更新到最新版街机MOD 并重启游戏后重试',
    'zh-TW': '主機實用工具拒絕了改名請求，請更新到最新版並重新啟動遊戲後重試',
    en: 'HostUtilities rejected the rename request. Update HostUtilities to the latest version and restart the game.',
    ko: '호스트 유틸리티가 이름 변경 요청을 거부했습니다. 최신 버전으로 업데이트하고 게임을 다시 시작하세요.',
    ja: 'ホストユーティリティが改名リクエストを拒否しました。最新版に更新してゲームを再起動してください。',
  },
  'rename.nameEmpty': {
    'zh-CN': '名字不能为空',
    'zh-TW': '名字不能為空',
    en: 'Name cannot be empty',
    ko: '이름을 비워 둘 수 없습니다',
    ja: '名前を空にはできません',
  },
  'rename.nameHasControlChars': {
    'zh-CN': '名字不能包含控制字符',
    'zh-TW': '名字不能包含控制字元',
    en: 'Name cannot contain control characters',
    ko: '이름에 제어 문자를 포함할 수 없습니다',
    ja: '名前に制御文字を含めることはできません',
  },
  'rename.nameTooLong': {
    'zh-CN': '名字代码超过 MOD 限制（UTF-16LE {bytes} / {limit} 字节）',
    'zh-TW': '名字代碼超過 MOD 限制（UTF-16LE {bytes} / {limit} 位元組）',
    en: 'Name exceeds the MOD limit ({bytes} / {limit} bytes in UTF-16LE)',
    ko: '이름이 MOD 제한을 초과했습니다(UTF-16LE {bytes} / {limit} 바이트)',
    ja: '名前が MOD の上限を超えています（UTF-16LE {bytes} / {limit} バイト）',
  },
  'colorfulName.httpClient': {
    'zh-CN': '创建 HTTP 客户端失败：{reason}',
    'zh-TW': '建立 HTTP 用戶端失敗：{reason}',
    en: 'Failed to create the HTTP client: {reason}',
    ko: 'HTTP 클라이언트 생성 실패: {reason}',
    ja: 'HTTP クライアントの作成に失敗しました：{reason}',
  },

  // ===== 游戏与框架 =====
  'game.notFound': {
    'zh-CN': '尚未找到厨房目录',
    'zh-TW': '尚未找到廚房目錄',
    en: 'Kitchen folder not found yet',
    ko: '주방 폴더를 아직 찾지 못했습니다',
    ja: 'キッチンフォルダーが見つかりません',
  },
  'game.running': {
    'zh-CN': '《胡闹厨房2》正在运行，请先关闭后重试',
    'zh-TW': '《煮過頭2》正在執行，請先關閉後重試',
    en: 'Overcooked! 2 is running. Close it and try again.',
    ko: '《오버쿡드! 2》가 실행 중입니다. 종료한 뒤 다시 시도하세요.',
    ja: '『オーバークック2』が実行中です。終了してから再試行してください。',
  },
  'game.unsupportedPlatform': {
    'zh-CN': '当前平台不支持启动厨房',
    'zh-TW': '目前平台不支援啟動廚房',
    en: 'Launching the kitchen is not supported on this platform',
    ko: '현재 플랫폼에서는 주방 실행을 지원하지 않습니다',
    ja: 'このプラットフォームではキッチンを起動できません',
  },
  'framework.bepinexMissing': {
    'zh-CN': 'BepInEx 未安装',
    'zh-TW': 'BepInEx 未安裝',
    en: 'BepInEx is not installed',
    ko: 'BepInEx가 설치되어 있지 않습니다',
    ja: 'BepInEx がインストールされていません',
  },
  'framework.bepinexOnlyResidue': {
    'zh-CN': 'BepInEx 框架未安装（仅存在 MOD 目录），无需卸载',
    'zh-TW': 'BepInEx 框架未安裝（僅存在 MOD 目錄），無需解除安裝',
    en: 'BepInEx is not installed (only a MOD folder exists); nothing to uninstall',
    ko: 'BepInEx 프레임워크가 설치되어 있지 않습니다(MOD 폴더만 있음). 제거할 필요가 없습니다',
    ja: 'BepInEx フレームワークは未インストールです（MOD フォルダーのみ）。アンインストール不要です',
  },
  'framework.alreadyInstalled': {
    'zh-CN': 'BepInEx 已安装，如需覆盖请使用重新安装',
    'zh-TW': 'BepInEx 已安裝，如需覆寫請使用重新安裝',
    en: 'BepInEx is already installed. Use reinstall to overwrite.',
    ko: 'BepInEx가 이미 설치되어 있습니다. 덮어쓰려면 재설치를 사용하세요.',
    ja: 'BepInEx は既にインストール済みです。上書きするには再インストールしてください。',
  },
  'framework.cleanupFailed': {
    'zh-CN': '清理旧 BepInEx 失败：{reason}',
    'zh-TW': '清理舊 BepInEx 失敗：{reason}',
    en: 'Failed to clean up the old BepInEx: {reason}',
    ko: '기존 BepInEx 정리 실패: {reason}',
    ja: '既存の BepInEx の削除に失敗しました：{reason}',
  },
  'framework.diyLevelMissing': {
    'zh-CN': '尚未安装 DIYLevel 框架',
    'zh-TW': '尚未安裝 DIYLevel 框架',
    en: 'DIYLevel framework is not installed',
    ko: 'DIYLevel 프레임워크가 설치되어 있지 않습니다',
    ja: 'DIYLevel フレームワークがインストールされていません',
  },
  'framework.diyLevelDirMissing': {
    'zh-CN': '未找到 DIYLevel 目录：{path}',
    'zh-TW': '找不到 DIYLevel 目錄：{path}',
    en: 'DIYLevel folder not found: {path}',
    ko: 'DIYLevel 폴더를 찾을 수 없습니다: {path}',
    ja: 'DIYLevel フォルダーが見つかりません：{path}',
  },

  // ===== 备份与恢复 =====
  'backup.notFound': {
    'zh-CN': '备份目录不存在',
    'zh-TW': '備份目錄不存在',
    en: 'Backup folder not found',
    ko: '백업 폴더가 없습니다',
    ja: 'バックアップフォルダーがありません',
  },
  'backup.unavailable': {
    'zh-CN': '备份目录不可用',
    'zh-TW': '備份目錄無法使用',
    en: 'Backup folder is unavailable',
    ko: '백업 폴더를 사용할 수 없습니다',
    ja: 'バックアップフォルダーを利用できません',
  },
  'backup.wrongKind': {
    'zh-CN': '只能操作该框架的时间戳备份',
    'zh-TW': '只能操作該框架的時間戳備份',
    en: 'Only timestamped backups of this framework can be handled',
    ko: '이 프레임워크의 타임스탬프 백업만 처리할 수 있습니다',
    ja: 'このフレームワークのタイムスタンプ付きバックアップのみ操作できます',
  },
  'backup.outsideRoot': {
    'zh-CN': '备份目录不在允许的备份位置',
    'zh-TW': '備份目錄不在允許的備份位置',
    en: 'The backup is not inside an allowed backup location',
    ko: '허용된 백업 위치에 있지 않은 폴더입니다',
    ja: '許可されたバックアップ場所にないフォルダーです',
  },
  'backup.overlap': {
    'zh-CN': '备份与框架目录重叠，已停止操作以避免递归复制。请把管理器安装到游戏目录外，或把数据目录移出 BepInEx。',
    'zh-TW': '備份與框架目錄重疊，已停止操作以避免遞迴複製。請把管理器安裝到遊戲目錄外，或把資料目錄移出 BepInEx。',
    en: 'The backup and framework folders overlap, so the operation was stopped to avoid recursive copying. Install the manager outside the game folder, or move the data folder out of BepInEx.',
    ko: '백업과 프레임워크 폴더가 겹쳐 재귀 복사를 막기 위해 작업을 중단했습니다. 관리자를 게임 폴더 밖에 설치하거나 데이터 폴더를 BepInEx 밖으로 옮기세요.',
    ja: 'バックアップとフレームワークのフォルダーが重なっているため、再帰コピーを避けるべく中止しました。マネージャーをゲームフォルダーの外に置くか、データフォルダーを BepInEx の外へ移動してください。',
  },
  'backup.sameDirectory': {
    'zh-CN': '备份目标与源目录是同一个目录，已停止操作以避免覆盖数据。',
    'zh-TW': '備份目標與來源目錄是同一個目錄，已停止操作以避免覆寫資料。',
    en: 'The backup target and source are the same folder; stopped to avoid overwriting data.',
    ko: '백업 대상과 원본이 같은 폴더여서 데이터 덮어쓰기를 막기 위해 중단했습니다.',
    ja: 'バックアップ先とコピー元が同じフォルダーのため、データ上書きを避けるべく中止しました。',
  },
  'backup.recycleFailed': {
    'zh-CN': '文件回收失败，请尝试手动删除框架文件。（{reason}）备份已保存在 {path}',
    'zh-TW': '檔案回收失敗，請嘗試手動刪除框架檔案。（{reason}）備份已保存在 {path}',
    en: 'Could not move the files to the recycle bin; delete the framework files manually. ({reason}) A backup is saved at {path}',
    ko: '파일을 휴지통으로 옮기지 못했습니다. 프레임워크 파일을 직접 삭제해 주세요. ({reason}) 백업 위치: {path}',
    ja: 'ファイルをゴミ箱へ移動できませんでした。フレームワークのファイルを手動で削除してください。（{reason}）バックアップ：{path}',
  },
  'backup.noReclaimableFiles': {
    'zh-CN': '未找到可回收的框架文件。备份已保存在 {path}',
    'zh-TW': '找不到可回收的框架檔案。備份已保存在 {path}',
    en: 'No framework files could be recycled. A backup is saved at {path}',
    ko: '회수할 프레임워크 파일을 찾지 못했습니다. 백업 위치: {path}',
    ja: '回収できるフレームワークファイルがありません。バックアップ：{path}',
  },

  // ===== MOD 卸载与回收站 =====
  'mods.packageConfirmation': {
    'zh-CN': '当前 MOD 与其他 MOD 一并安装',
    'zh-TW': '目前 MOD 與其他 MOD 一併安裝',
    en: 'This MOD is installed together with other MODs',
    ko: '이 MOD는 다른 MOD와 함께 설치되어 있습니다',
    ja: 'この MOD は他の MOD と一緒にインストールされています',
  },
  'mods.unmanagedConfirmation': {
    'zh-CN': '该 MOD 不是通过本管理器安装的，管理器中只会删除这个 DLL，其他相关文件需手动处理。是否继续？',
    'zh-TW': '該 MOD 不是透過本管理器安裝的，管理員中只會刪除這個 DLL，其他相關檔案需手動處理。是否繼續？',
    en: 'This MOD was not installed by this manager. Only this DLL will be deleted here; other related files must be handled manually. Continue?',
    ko: '이 MOD는 이 관리자로 설치된 것이 아닙니다. 여기서는 이 DLL만 삭제되며 관련 파일은 직접 처리해야 합니다. 계속할까요?',
    ja: 'この MOD はこのマネージャーでインストールされたものではありません。ここではこの DLL のみ削除され、関連ファイルは手動で処理する必要があります。続行しますか？',
  },
  'mods.unknownFiles': {
    'zh-CN': '没有安装清单，拒绝删除未知文件',
    'zh-TW': '沒有安裝清單，拒絕刪除未知檔案',
    en: 'No install manifest; refusing to delete unknown files',
    ko: '설치 목록이 없어 알 수 없는 파일 삭제를 거부합니다',
    ja: 'インストール記録がないため、不明なファイルの削除を中止しました',
  },
  'mods.dllNotInPackage': {
    'zh-CN': '当前 DLL 不属于这个 MOD 包',
    'zh-TW': '目前 DLL 不屬於這個 MOD 包',
    en: 'This DLL does not belong to that MOD package',
    ko: '이 DLL은 해당 MOD 패키지에 속하지 않습니다',
    ja: 'この DLL はその MOD パッケージに属していません',
  },
}
