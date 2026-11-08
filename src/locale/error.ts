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
    'zh-CN': '备份与框架目录重叠，已停止操作以避免递归复制。请确认游戏目录不在管理器数据目录内，且管理器没有安装在 BepInEx 里。',
    'zh-TW': '備份與框架目錄重疊，已停止操作以避免遞迴複製。請確認遊戲目錄不在管理器資料目錄內，且管理器沒有安裝在 BepInEx 裡。',
    en: 'The backup and framework folders overlap, so the operation was stopped to avoid recursive copying. Make sure the game folder is outside the manager data folder and the manager is not installed inside BepInEx.',
    ko: '백업과 프레임워크 폴더가 겹쳐 재귀 복사를 막기 위해 작업을 중단했습니다. 게임 폴더가 관리자 데이터 폴더 안에 없고 관리자가 BepInEx 안에 설치되지 않았는지 확인하세요.',
    ja: 'バックアップとフレームワークのフォルダーが重なっているため、再帰コピーを避けて中止しました。ゲームフォルダーがマネージャーのデータフォルダー内にないこと、マネージャーが BepInEx 内にインストールされていないことを確認してください。',
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
  'mods.dllNotFound': {
    'zh-CN': '找不到对应的 MOD DLL 文件',
    'zh-TW': '找不到對應的 MOD DLL 檔案',
    en: 'The corresponding MOD DLL was not found',
    ko: '해당 MOD DLL 파일을 찾을 수 없습니다',
    ja: '対応する MOD の DLL が見つかりません',
  },
  'mods.notInPlugins': {
    'zh-CN': '只能删除 BepInEx/plugins 下的 DLL 文件',
    'zh-TW': '只能刪除 BepInEx/plugins 下的 DLL 檔案',
    en: 'Only DLL files under BepInEx/plugins can be deleted',
    ko: 'BepInEx/plugins 아래의 DLL 파일만 삭제할 수 있습니다',
    ja: 'BepInEx/plugins 配下の DLL のみ削除できます',
  },

  // ===== 回收站 =====
  'recycle.recordMissing': {
    'zh-CN': '回收站记录不存在',
    'zh-TW': '資源回收筒記錄不存在',
    en: 'That recycle-bin entry does not exist',
    ko: '휴지통 기록이 없습니다',
    ja: 'ゴミ箱の記録がありません',
  },
  'recycle.fileSharedByOtherPackage': {
    'zh-CN': '该文件仍由其他 MOD 包使用，不能移入回收站',
    'zh-TW': '該檔案仍由其他 MOD 包使用，不能移入資源回收筒',
    en: 'That file is still used by another MOD package and cannot be moved to the recycle bin',
    ko: '해당 파일은 다른 MOD 패키지에서 사용 중이라 휴지통으로 옮길 수 없습니다',
    ja: 'このファイルは他の MOD パッケージが使用中のため、ゴミ箱へ移動できません',
  },
  'recycle.insideModsDirectory': {
    'zh-CN': '回收站不能位于游戏的 MOD 目录内',
    'zh-TW': '資源回收筒不能位於遊戲的 MOD 目錄內',
    en: 'The recycle bin cannot be inside the game MOD folder',
    ko: '휴지통은 게임 MOD 폴더 안에 있을 수 없습니다',
    ja: 'ゴミ箱をゲームの MOD フォルダー内に置くことはできません',
  },
  'recycle.partialFailure': {
    'zh-CN': '操作失败，部分文件保留在回收站，请查看日志',
    'zh-TW': '操作失敗，部分檔案保留在資源回收筒，請查看日誌',
    en: 'The operation failed; some files remain in the recycle bin. Check the logs.',
    ko: '작업이 실패했습니다. 일부 파일이 휴지통에 남아 있습니다. 로그를 확인하세요.',
    ja: '操作に失敗しました。一部のファイルがゴミ箱に残っています。ログを確認してください。',
  },
  'recycle.packageUpdated': {
    'zh-CN': '当前 MOD 包已更新，无法合并原安装清单',
    'zh-TW': '目前 MOD 包已更新，無法合併原安裝清單',
    en: 'The MOD package was updated, so the original manifest cannot be merged',
    ko: 'MOD 패키지가 업데이트되어 기존 설치 목록을 병합할 수 없습니다',
    ja: 'MOD パッケージが更新されているため、元のインストール記録を統合できません',
  },
  'recycle.wrongGameDirectory': {
    'zh-CN': '请切回原游戏目录后再恢复',
    'zh-TW': '請切回原遊戲目錄後再還原',
    en: 'Switch back to the original game folder before restoring',
    ko: '원래 게임 폴더로 돌아간 뒤 복원하세요',
    ja: '元のゲームフォルダーに戻してから復元してください',
  },

  // ===== 下载 =====
  'download.invalidVolumeSet': {
    'zh-CN': 'ZIP 分卷不完整或编号重复：需要同一 Release 中连续的 .z01、.z02…及最后的 .zip',
    'zh-TW': 'ZIP 分卷不完整或編號重複：需要同一 Release 中連續的 .z01、.z02…及最後的 .zip',
    en: 'Incomplete or duplicate ZIP volumes: consecutive .z01, .z02… and the final .zip must belong to the same Release',
    ko: 'ZIP 분할 파일이 누락되었거나 중복됩니다. 같은 Release의 연속된 .z01, .z02… 및 마지막 .zip이 필요합니다',
    ja: 'ZIP 分割ファイルが不足または重複しています。同じ Release の連続した .z01、.z02… と最後の .zip が必要です',
  },
  'download.invalidSplitZip': {
    'zh-CN': 'ZIP 分卷结构损坏，无法重组；请检查所有分卷是否属于同一个压缩包',
    'zh-TW': 'ZIP 分卷結構損壞，無法重組；請檢查所有分卷是否屬於同一個壓縮檔',
    en: 'Invalid split ZIP structure. Check that all volumes belong to the same archive',
    ko: '분할 ZIP 구조가 잘못되었습니다. 모든 분할 파일이 같은 압축 파일인지 확인하세요',
    ja: '分割 ZIP の構造が不正です。すべての分割ファイルが同じアーカイブか確認してください',
  },
  'download.unsupportedSplitZip': {
    'zh-CN': '暂不支持此分卷 ZIP 格式（包括 ZIP64）；请使用普通 ZIP 或经典分卷 ZIP',
    'zh-TW': '暫不支援此分卷 ZIP 格式（包括 ZIP64）；請使用普通 ZIP 或經典分卷 ZIP',
    en: 'Unsupported split ZIP format (including ZIP64). Use a regular ZIP or classic split ZIP',
    ko: '지원하지 않는 분할 ZIP 형식입니다(ZIP64 포함). 일반 ZIP 또는 기존 분할 ZIP을 사용하세요',
    ja: '未対応の分割 ZIP 形式です（ZIP64 を含む）。通常の ZIP または従来の分割 ZIP を使用してください',
  },
  'download.cancelled': {
    'zh-CN': '下载已取消',
    'zh-TW': '下載已取消',
    en: 'Download cancelled',
    ko: '다운로드가 취소되었습니다',
    ja: 'ダウンロードを中止しました',
  },
  'download.empty': {
    'zh-CN': '下载文件为空',
    'zh-TW': '下載檔案為空',
    en: 'The downloaded file is empty',
    ko: '다운로드한 파일이 비어 있습니다',
    ja: 'ダウンロードしたファイルが空です',
  },
  'download.incomplete': {
    'zh-CN': '下载长度不完整',
    'zh-TW': '下載長度不完整',
    en: 'The download is incomplete',
    ko: '다운로드 길이가 완전하지 않습니다',
    ja: 'ダウンロードが不完全です',
  },
  'download.httpStatus': {
    'zh-CN': '下载返回 HTTP {status}',
    'zh-TW': '下載回傳 HTTP {status}',
    en: 'The download returned HTTP {status}',
    ko: '다운로드가 HTTP {status}를 반환했습니다',
    ja: 'ダウンロードが HTTP {status} を返しました',
  },
  'download.checksumFailed': {
    'zh-CN': 'SHA-256 校验失败',
    'zh-TW': 'SHA-256 校驗失敗',
    en: 'SHA-256 verification failed',
    ko: 'SHA-256 검증에 실패했습니다',
    ja: 'SHA-256 の検証に失敗しました',
  },
  'download.checksumMismatch': {
    'zh-CN': 'SHA-256 校验失败，期望 {expected}，实际 {actual}',
    'zh-TW': 'SHA-256 校驗失敗，期望 {expected}，實際 {actual}',
    en: 'SHA-256 mismatch: expected {expected}, got {actual}',
    ko: 'SHA-256 불일치: 예상 {expected}, 실제 {actual}',
    ja: 'SHA-256 が一致しません：期待 {expected}、実際 {actual}',
  },
  'download.chunkTooLong': {
    'zh-CN': '分段长度超出预期：收到 {bytes} 字节',
    'zh-TW': '分段長度超出預期：收到 {bytes} 位元組',
    en: 'Chunk longer than expected: received {bytes} bytes',
    ko: '청크 길이가 예상보다 깁니다: {bytes}바이트 수신',
    ja: 'チャンクが想定より長いです：{bytes} バイト受信',
  },
  'download.chunkLengthMismatch': {
    'zh-CN': '分段长度错误：收到 {bytes} 字节',
    'zh-TW': '分段長度錯誤：收到 {bytes} 位元組',
    en: 'Wrong chunk length: received {bytes} bytes',
    ko: '청크 길이가 잘못되었습니다: {bytes}바이트 수신',
    ja: 'チャンク長が不正です：{bytes} バイト受信',
  },

  // ===== 归档与解压 =====
  'archive.invalidZipName': {
    'zh-CN': 'ZIP 文件名无效',
    'zh-TW': 'ZIP 檔名無效',
    en: 'Invalid ZIP file name',
    ko: 'ZIP 파일 이름이 잘못되었습니다',
    ja: 'ZIP のファイル名が不正です',
  },
  'archive.unsafePath': {
    'zh-CN': '拒绝不安全 ZIP 路径：{entry}',
    'zh-TW': '拒絕不安全的 ZIP 路徑：{entry}',
    en: 'Refused an unsafe ZIP path: {entry}',
    ko: '안전하지 않은 ZIP 경로를 거부했습니다: {entry}',
    ja: '安全でない ZIP パスを拒否しました：{entry}',
  },
  'archive.pathEscape': {
    'zh-CN': '路径逃逸游戏目录',
    'zh-TW': '路徑逃逸遊戲目錄',
    en: 'The path escapes the game folder',
    ko: '경로가 게임 폴더를 벗어납니다',
    ja: 'パスがゲームフォルダーの外に出ています',
  },
  'archive.noInstallableContent': {
    'zh-CN': '所选文件没有可安装内容',
    'zh-TW': '所選檔案沒有可安裝內容',
    en: 'The selected file contains nothing installable',
    ko: '선택한 파일에 설치할 내용이 없습니다',
    ja: '選択したファイルにインストールできる内容がありません',
  },
  'archive.sourceEqualsTarget': {
    'zh-CN': '所选文件已经位于安装目标，不能覆盖自身',
    'zh-TW': '所選檔案已經位於安裝目標，不能覆寫自身',
    en: 'The selected file is already at the install target and cannot overwrite itself',
    ko: '선택한 파일이 이미 설치 대상 위치에 있어 스스로를 덮어쓸 수 없습니다',
    ja: '選択したファイルは既にインストール先にあるため、自身を上書きできません',
  },
  'archive.targetExists': {
    'zh-CN': '目标文件已存在：{path}',
    'zh-TW': '目標檔案已存在：{path}',
    en: 'The target file already exists: {path}',
    ko: '대상 파일이 이미 있습니다: {path}',
    ja: '対象のファイルが既に存在します：{path}',
  },

  // ===== 仓库源导入与解析 =====
  'source.addressEmpty': {
    'zh-CN': '仓库源地址不能为空',
    'zh-TW': '倉庫源位址不能為空',
    en: 'The repository source address cannot be empty',
    ko: '저장소 소스 주소를 비워 둘 수 없습니다',
    ja: 'リポジトリソースのアドレスは空にできません',
  },
  'source.addressInvalid': {
    'zh-CN': '仓库源地址无效：{reason}',
    'zh-TW': '倉庫源位址無效：{reason}',
    en: 'Invalid repository source address: {reason}',
    ko: '저장소 소스 주소가 잘못되었습니다: {reason}',
    ja: 'リポジトリソースのアドレスが不正です：{reason}',
  },
  'source.addressHasCredentials': {
    'zh-CN': '仓库源地址不能包含认证信息',
    'zh-TW': '倉庫源位址不能包含認證資訊',
    en: 'The repository source address cannot contain credentials',
    ko: '저장소 소스 주소에 인증 정보를 포함할 수 없습니다',
    ja: 'リポジトリソースのアドレスに認証情報を含めることはできません',
  },
  'source.builtinPathUnsafe': {
    'zh-CN': '内置仓库源路径不安全',
    'zh-TW': '內建倉庫源路徑不安全',
    en: 'The built-in repository source path is unsafe',
    ko: '내장 저장소 소스 경로가 안전하지 않습니다',
    ja: '組み込みリポジトリソースのパスが安全ではありません',
  },
  'source.builtinNotToml': {
    'zh-CN': '内置仓库源必须是 TOML 文件',
    'zh-TW': '內建倉庫源必須是 TOML 檔案',
    en: 'A built-in repository source must be a TOML file',
    ko: '내장 저장소 소스는 TOML 파일이어야 합니다',
    ja: '組み込みリポジトリソースは TOML ファイルである必要があります',
  },
  'source.builtinFileMissing': {
    'zh-CN': '内置仓库源文件不存在：{path}',
    'zh-TW': '內建倉庫源檔案不存在：{path}',
    en: 'The built-in repository source file does not exist: {path}',
    ko: '내장 저장소 소스 파일이 없습니다: {path}',
    ja: '組み込みリポジトリソースのファイルがありません：{path}',
  },
  'source.notBuiltin': {
    'zh-CN': '仓库源不是内置文件',
    'zh-TW': '倉庫源不是內建檔案',
    en: 'That repository source is not a built-in file',
    ko: '해당 저장소 소스는 내장 파일이 아닙니다',
    ja: 'そのリポジトリソースは組み込みファイルではありません',
  },
  'source.idOrNicknameEmpty': {
    'zh-CN': '仓库源 ID 和昵称不能为空',
    'zh-TW': '倉庫源 ID 和暱稱不能為空',
    en: 'Repository source ID and nickname cannot be empty',
    ko: '저장소 소스 ID와 별명을 비워 둘 수 없습니다',
    ja: 'リポジトリソースの ID とニックネームは空にできません',
  },
  'source.proxyNotHttps': {
    'zh-CN': '代理来源地址必须是 HTTPS URL',
    'zh-TW': '代理來源位址必須是 HTTPS URL',
    en: 'A proxy source address must be an HTTPS URL',
    ko: '프록시 소스 주소는 HTTPS URL이어야 합니다',
    ja: 'プロキシソースのアドレスは HTTPS URL である必要があります',
  },
  'source.proxyAddressInvalid': {
    'zh-CN': '代理来源地址无效：{reason}',
    'zh-TW': '代理來源位址無效：{reason}',
    en: 'Invalid proxy source address: {reason}',
    ko: '프록시 소스 주소가 잘못되었습니다: {reason}',
    ja: 'プロキシソースのアドレスが不正です：{reason}',
  },
  'source.tooLarge': {
    'zh-CN': '仓库源文件超过 1 MiB',
    'zh-TW': '倉庫源檔案超過 1 MiB',
    en: 'The repository source file exceeds 1 MiB',
    ko: '저장소 소스 파일이 1 MiB를 초과합니다',
    ja: 'リポジトリソースのファイルが 1 MiB を超えています',
  },
  'source.tomlInvalid': {
    'zh-CN': '仓库源 TOML 解析失败：{reason}',
    'zh-TW': '倉庫源 TOML 解析失敗：{reason}',
    en: 'Failed to parse the repository source TOML: {reason}',
    ko: '저장소 소스 TOML 구문 분석 실패: {reason}',
    ja: 'リポジトリソースの TOML 解析に失敗しました：{reason}',
  },
  /*
   * 系统 Shell 打开 / 定位失败（`src-tauri/src/shell_open.rs`）。
   *
   * 两个入口都不再调 explorer.exe：`openFailed` 的 `reason` 是 ShellExecuteW
   * 的原始错误码（2/3 找不到、5 拒绝访问、31 没有关联程序），`revealFailed`
   * 带的是没定位成功的路径。
   */
  'shell.openFailed': {
    'zh-CN': '系统没能打开这个位置（错误码 {reason}），请检查默认打开方式',
    'zh-TW': '系統沒能開啟這個位置（錯誤代碼 {reason}），請檢查預設開啟方式',
    en: 'The system could not open this location (code {reason}). Check the default handler.',
    ko: '시스템이 이 위치를 열지 못했습니다(코드 {reason}). 기본 연결 프로그램을 확인하세요.',
    ja: 'システムがこの場所を開けませんでした（コード {reason}）。既定の関連付けを確認してください。',
  },
  'shell.revealFailed': {
    'zh-CN': '没能在文件管理器中定位这个位置：{path}',
    'zh-TW': '沒能在檔案管理員中定位這個位置：{path}',
    en: 'Could not reveal this location in the file manager: {path}',
    ko: '파일 관리자에서 이 위치를 찾을 수 없습니다: {path}',
    ja: 'ファイルマネージャーでこの場所を表示できませんでした：{path}',
  },
  /*
   * Tauri 宿主返回的 `Command xxx not found` 不是后端 tag，是英文串。
   * 常见场景：前端已热更新、正在跑的后端二进制还是旧的（新命令没编进去）。
   * 由 `src/locale/backend.ts` 在识别出该形态时兜底使用。
   */
  'error.commandUnavailable': {
    'zh-CN': '管理器没有命令 {command}，正在运行的版本可能和界面不一致；请重启管理器后重试',
    'zh-TW': '管理器沒有命令 {command}，目前執行的版本可能與介面不一致；請重新啟動管理器後再試',
    en: 'The manager has no command {command}. The running build differs from the UI; restart the manager and try again.',
    ko: '관리자에 {command} 명령이 없습니다. 실행 중인 빌드가 화면과 다를 수 있으니 관리자를 다시 시작한 뒤 다시 시도하세요.',
    ja: 'マネージャーに {command} コマンドがありません。実行中のビルドが画面と一致していない可能性があります。マネージャーを再起動してから再試行してください。',
  },
}
