import type { DomainMessages } from '../types.ts'

/** 清除管理器数据的入口、危险确认与后端拒绝原因。 */
export const toolsUiDataReset: DomainMessages = {
  configuration: { 'zh-CN': '配置', 'zh-TW': '設定', en: 'Configuration', ko: '설정', ja: '設定' },
  clearAllConfig: { 'zh-CN': '清除所有配置', 'zh-TW': '清除所有設定', en: 'Clear all configuration', ko: '모든 설정 삭제', ja: 'すべての設定を削除' },
  clearAllConfigDescription: {
    'zh-CN': '删除管理器的全部本地数据并退出，下次启动恢复默认设置。',
    'zh-TW': '刪除管理器的全部本機資料並結束，下次啟動恢復預設設定。',
    en: 'Delete all local manager data and exit. The next launch uses default settings.',
    ko: '관리자의 모든 로컬 데이터를 삭제하고 종료합니다. 다음 실행 시 기본 설정으로 시작합니다.',
    ja: 'マネージャーのローカルデータをすべて削除して終了します。次回は初期設定で起動します。',
  },
  clearAllConfigConfirm: {
    'zh-CN': '此操作不可撤销。下方管理器数据目录内的所有设置、安装清单、缓存、备份和回收站内容都将被永久删除。',
    'zh-TW': '此操作無法復原。下方管理器資料目錄內的所有設定、安裝清單、快取、備份和資源回收筒內容都將永久刪除。',
    en: 'This cannot be undone. All settings, installation records, caches, backups and recycle bin contents in the manager data directory below will be permanently deleted.',
    ko: '이 작업은 되돌릴 수 없습니다. 아래 관리자 데이터 폴더의 모든 설정, 설치 기록, 캐시, 백업 및 휴지통 내용이 영구 삭제됩니다.',
    ja: 'この操作は元に戻せません。以下のマネージャーデータフォルダー内の設定、インストール記録、キャッシュ、バックアップ、ごみ箱の内容をすべて完全に削除します。',
  },
  managerDataDirectory: { 'zh-CN': '将清理的数据目录：', 'zh-TW': '將清理的資料目錄：', en: 'Data directory to clear:', ko: '삭제할 데이터 폴더:', ja: '削除対象のデータフォルダー：' },
  clearAllConfigPreservesGame: {
    'zh-CN': '游戏目录中已经安装的 MOD、地图和其他文件不会被删除。',
    'zh-TW': '遊戲目錄中已安裝的 MOD、地圖及其他檔案不會被刪除。',
    en: 'Installed MODs, maps and other files in the game directory will remain.',
    ko: '게임 폴더에 설치된 MOD, 지도 및 기타 파일은 삭제되지 않습니다.',
    ja: 'ゲームフォルダーにインストール済みのMOD、マップ、その他のファイルは削除しません。',
  },
  clearAllConfigExit: {
    'zh-CN': '确认后管理器将退出，退出后清理数据；下次启动将恢复默认设置。',
    'zh-TW': '確認後管理器將結束，結束後清理資料；下次啟動將恢復預設設定。',
    en: 'The manager will exit, then clear its data. The next launch will use default settings.',
    ko: '확인하면 관리자가 종료된 후 데이터를 삭제합니다. 다음 실행 시 기본 설정으로 시작합니다.',
    ja: '確認後にマネージャーを終了し、データを削除します。次回は初期設定で起動します。',
  },
  clearAllConfigAndExit: { 'zh-CN': '清除全部并退出', 'zh-TW': '全部清除並結束', en: 'Clear everything and exit', ko: '모두 삭제하고 종료', ja: 'すべて削除して終了' },
  clearAllConfigBusy: {
    'zh-CN': '仍有任务正在执行，请等待完成后再清除配置。',
    'zh-TW': '仍有工作正在執行，請等待完成後再清除設定。',
    en: 'An operation is still running. Wait for it to finish before clearing configuration.',
    ko: '실행 중인 작업이 있습니다. 완료된 후 설정을 삭제해 주세요.',
    ja: '処理が実行中です。完了してから設定を削除してください。',
  },
  clearAllConfigFailed: {
    'zh-CN': '无法准备清除管理器数据：{reason}',
    'zh-TW': '無法準備清除管理器資料：{reason}',
    en: 'Could not prepare to clear manager data: {reason}',
    ko: '관리자 데이터 삭제를 준비할 수 없습니다: {reason}',
    ja: 'マネージャーデータの削除を準備できませんでした：{reason}',
  },
  clearAllConfigUnsafe: {
    'zh-CN': '管理器数据目录未通过安全校验，已取消清除。',
    'zh-TW': '管理器資料目錄未通過安全檢查，已取消清除。',
    en: 'The manager data directory failed the safety check. Clearing was cancelled.',
    ko: '관리자 데이터 폴더가 안전성 검사를 통과하지 못해 삭제를 취소했습니다.',
    ja: 'マネージャーデータフォルダーの安全性を確認できなかったため、削除を中止しました。',
  },
}
