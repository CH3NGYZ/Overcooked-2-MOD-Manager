import type { DomainMessages } from './types'

/**
 * recycle 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const recycleUi: DomainMessages = {
  // 回收站
  recycleBin: {
    'zh-CN': '回收站',
    'zh-TW': '資源回收筒',
    en: 'Recycle bin',
    ko: '휴지통',
    ja: 'ゴミ箱',
  },
  // 回收站为空
  recycleBinIsEmpty: {
    'zh-CN': '回收站为空',
    'zh-TW': '資源回收筒是空的',
    en: 'Recycle bin is empty',
    ko: '휴지통이 비어 있습니다',
    ja: 'ゴミ箱は空です',
  },
  // 尚未删除任何 MOD。删除的 MOD 会先暂存在这里，可随时恢复。
  noModsDeletedYetDeletedModsAreKeptHereUntilYouRemoveThemPermanently: {
    'zh-CN': '尚未删除任何 MOD。删除的 MOD 会先暂存在这里，可随时恢复。',
    'zh-TW': '尚未刪除任何 MOD。刪除的 MOD 會先暫存在這裡，可隨時還原。',
    en: 'No MODs deleted yet. Deleted MODs are kept here until you remove them permanently.',
    ko: '삭제한 MOD가 없습니다. 삭제한 MOD는 여기에 보관되며 언제든 복원할 수 있습니다.',
    ja: 'まだ MOD を削除していません。削除した MOD はここに保管され、いつでも復元できます。',
  },
  // 正在读取回收站
  loadingRecycleBin: {
    'zh-CN': '正在读取回收站',
    'zh-TW': '正在讀取資源回收筒',
    en: 'Loading recycle bin',
    ko: '휴지통 읽는 중',
    ja: 'ゴミ箱を読み込み中',
  },
  // 立即刷新
  refreshNow: {
    'zh-CN': '立即刷新',
    'zh-TW': '立即重新整理',
    en: 'Refresh now',
    ko: '지금 새로 고침',
    ja: '今すぐ更新',
  },
  // 彻底删除
  deletePermanently: {
    'zh-CN': '彻底删除',
    'zh-TW': '永久刪除',
    en: 'Delete permanently',
    ko: '완전 삭제',
    ja: '完全に削除',
  },
  // 清空回收站
  emptyRecycleBin: {
    'zh-CN': '清空回收站',
    'zh-TW': '清空資源回收筒',
    en: 'Empty recycle bin',
    ko: '휴지통 비우기',
    ja: 'ゴミ箱を空にする',
  },
  // 已清空回收站
  recycleBinEmptied: {
    'zh-CN': '已清空回收站',
    'zh-TW': '已清空資源回收筒',
    en: 'Recycle bin emptied',
    ko: '휴지통을 비웠습니다',
    ja: 'ゴミ箱を空にしました',
  },
  // 将永久删除回收站中的全部 {count} 条记录及其文件，无法恢复。确定清空？
  allRecycledEntriesAndTheirFilesWillBePermanentlyDeletedThisCannotBeUndoneEmptyTheBin: {
    'zh-CN': '将永久删除回收站中的全部 {count} 条记录及其文件，无法恢复。确定清空？',
    'zh-TW': '將永久刪除資源回收筒中的全部 {count} 筆記錄及其檔案，無法復原。確定清空？',
    en: 'All {count} recycled entries and their files will be permanently deleted. This cannot be undone. Empty the bin?',
    ko: '휴지통의 {count}개 항목과 파일을 모두 영구 삭제합니다. 되돌릴 수 없습니다. 비울까요?',
    ja: 'ゴミ箱の全 {count} 件の記録とファイルを完全に削除します。元に戻せません。空にしますか？',
  },
  // 全部彻底删除
  deleteAllPermanently: {
    'zh-CN': '全部彻底删除',
    'zh-TW': '全部永久刪除',
    en: 'Delete all permanently',
    ko: '모두 완전 삭제',
    ja: 'すべて完全削除',
  },
  // 彻底删除 MOD
  deleteModPermanently: {
    'zh-CN': '彻底删除 MOD',
    'zh-TW': '永久刪除 MOD',
    en: 'Delete MOD permanently',
    ko: 'MOD 완전 삭제',
    ja: 'MOD を完全に削除',
  },
  // 将永久删除回收站中的 MOD 文件和保存的安装清单，此操作无法撤销。
  thisPermanentlyDeletesTheModFilesAndTheSavedInstallManifestItCannotBeUndone: {
    'zh-CN': '将永久删除回收站中的 MOD 文件和保存的安装清单，此操作无法撤销。',
    'zh-TW': '將永久刪除資源回收筒中的 MOD 檔案與儲存的安裝清單，此操作無法復原。',
    en: 'This permanently deletes the MOD files and the saved install manifest. It cannot be undone.',
    ko: '휴지통의 MOD 파일과 저장된 설치 목록을 영구 삭제합니다. 되돌릴 수 없습니다.',
    ja: 'ゴミ箱内の MOD ファイルと保存されたインストール記録を完全に削除します。元に戻せません。',
  },
  // 删除时间
  deleted: {
    'zh-CN': '删除时间',
    'zh-TW': '刪除時間',
    en: 'Deleted',
    ko: '삭제 시각',
    ja: '削除日時',
  },
  // 文件数量
  files: {
    'zh-CN': '文件数量',
    'zh-TW': '檔案數量',
    en: 'Files',
    ko: '파일 수',
    ja: 'ファイル数',
  },
  // 原游戏目录
  originalLocation: {
    'zh-CN': '原游戏目录',
    'zh-TW': '原遊戲目錄',
    en: 'Original location',
    ko: '원래 게임 경로',
    ja: '元のゲームパス',
  },
  // 查看文件清单
  viewFiles: {
    'zh-CN': '查看文件清单',
    'zh-TW': '檢視檔案清單',
    en: 'View files',
    ko: '파일 목록 보기',
    ja: 'ファイル一覧を表示',
  },
  // 游戏运行中，暂时不能恢复 MOD。
  theGameIsRunningModsCannotBeRestoredRightNow: {
    'zh-CN': '游戏运行中，暂时不能恢复 MOD。',
    'zh-TW': '遊戲執行中，暫時無法還原 MOD。',
    en: 'The game is running; MODs cannot be restored right now.',
    ko: '게임이 실행 중이라 지금은 MOD를 복원할 수 없습니다.',
    ja: 'ゲーム実行中のため、現在 MOD を復元できません。',
  },
  // 打开回收站
  openRecycleBin: {
    'zh-CN': '打开回收站',
    'zh-TW': '開啟資源回收筒',
    en: 'Open recycle bin',
    ko: '휴지통 열기',
    ja: 'ゴミ箱を開く',
  },
  // 查看已删除的 MOD，可恢复到原位置或彻底删除
  reviewDeletedModsRestoreThemOrDeletePermanently: {
    'zh-CN': '查看已删除的 MOD，可恢复到原位置或彻底删除',
    'zh-TW': '檢視已刪除的 MOD，可還原到原位置或永久刪除',
    en: 'Review deleted MODs; restore them or delete permanently',
    ko: '삭제된 MOD를 확인하고 복원하거나 완전 삭제합니다',
    ja: '削除した MOD を確認し、復元または完全削除します',
  },
  // 删除 MOD 时优先放入回收站
  moveDeletedModsToTheRecycleBin: {
    'zh-CN': '删除 MOD 时优先放入回收站',
    'zh-TW': '刪除 MOD 時優先放入資源回收筒',
    en: 'Move deleted MODs to the recycle bin',
    ko: 'MOD 삭제 시 휴지통으로 이동',
    ja: 'MOD 削除時にゴミ箱へ移動',
  },
  // 关闭后删除的文件不会保留，无法恢复
  whenOffDeletedFilesAreRemovedPermanently: {
    'zh-CN': '关闭后删除的文件不会保留，无法恢复',
    'zh-TW': '關閉後刪除的檔案不會保留，無法還原',
    en: 'When off, deleted files are removed permanently',
    ko: '끄면 삭제한 파일이 보관되지 않으며 복원할 수 없습니다',
    ja: 'オフにすると削除したファイルは保存されず、復元できません',
  },
  // 删除的 MOD 将先移入回收站
  deletedModsWillGoToTheRecycleBin: {
    'zh-CN': '删除的 MOD 将先移入回收站',
    'zh-TW': '刪除的 MOD 會先移入資源回收筒',
    en: 'Deleted MODs will go to the recycle bin',
    ko: '삭제한 MOD는 휴지통으로 이동합니다',
    ja: '削除した MOD はゴミ箱に移動します',
  },
  // 删除的 MOD 将直接删除，不进回收站
  deletedModsAreRemovedPermanently: {
    'zh-CN': '删除的 MOD 将直接删除，不进回收站',
    'zh-TW': '刪除的 MOD 將直接刪除，不進資源回收筒',
    en: 'Deleted MODs are removed permanently',
    ko: '삭제한 MOD는 휴지통을 거치지 않고 바로 삭제됩니다',
    ja: '削除した MOD はゴミ箱を経由せず直接削除されます',
  },
  // 关闭回收站保护
  turnOffRecycleBinProtection: {
    'zh-CN': '关闭回收站保护',
    'zh-TW': '關閉資源回收筒保護',
    en: 'Turn off recycle bin protection',
    ko: '휴지통 보호 끄기',
    ja: 'ゴミ箱の保護を無効にする',
  },
  // 关闭后，管理器删除 MOD 时不再移入回收站，而是直接删除文件与安装清单。
  withThisOffDeletingAModRemovesItsFilesAndInstallManifestImmediatelyInsteadOfMovingThemToTheRecycleBin: {
    'zh-CN': '关闭后，管理器删除 MOD 时不再移入回收站，而是直接删除文件与安装清单。',
    'zh-TW': '關閉後，管理器刪除 MOD 時不再移入資源回收筒，而是直接刪除檔案與安裝清單。',
    en: 'With this off, deleting a MOD removes its files and install manifest immediately instead of moving them to the recycle bin.',
    ko: '끄면 MOD 삭제 시 휴지통으로 옮기지 않고 파일과 설치 목록을 바로 삭제합니다.',
    ja: 'オフにすると、MOD を削除するときにゴミ箱へ移さず、ファイルとインストール記録を直接削除します。',
  },
  // 这个操作无法撤销，删除的 MOD 不能恢复；覆盖安装时旧版本也会被直接清除。请确认你确实想绕过回收站。
  thisCannotBeUndoneDeletedModsAreUnrecoverableAndOverwritingAnInstallAlsoDiscardsThePreviousVersionConfirmThatYouReallyWantToBypassTheRecycleBin: {
    'zh-CN': '这个操作无法撤销，删除的 MOD 不能恢复；覆盖安装时旧版本也会被直接清除。请确认你确实想绕过回收站。',
    'zh-TW': '這個操作無法復原，刪除的 MOD 不能還原；覆蓋安裝時舊版本也會被直接清除。請確認你確實想繞過資源回收筒。',
    en: 'This cannot be undone: deleted MODs are unrecoverable, and overwriting an install also discards the previous version. Confirm that you really want to bypass the recycle bin.',
    ko: '되돌릴 수 없습니다. 삭제한 MOD는 복원할 수 없고, 덮어쓰기 설치 시 이전 버전도 그대로 사라집니다. 정말 휴지통을 거치지 않겠습니까?',
    ja: '元に戻せません。削除した MOD は復元できず、上書きインストール時は旧バージョンも直接消えます。本当にゴミ箱を経由しないようにしますか？',
  },
  // 仍然直接删除
  deleteDirectlyAnyway: {
    'zh-CN': '仍然直接删除',
    'zh-TW': '仍要直接刪除',
    en: 'Delete directly anyway',
    ko: '그래도 바로 삭제',
    ja: 'それでも直接削除する',
  },
  // 保持开启
  keepItOn: {
    'zh-CN': '保持开启',
    'zh-TW': '保持開啟',
    en: 'Keep it on',
    ko: '켜 둔 채로 유지',
    ja: 'オンのままにする',
  },
  // 卸载
  uninstall: {
    'zh-CN': '卸载',
    'zh-TW': '解除安裝',
    en: 'Uninstall',
    ko: '제거',
    ja: 'アンインストール',
  },
  // 升级
  upgrade: {
    'zh-CN': '升级',
    'zh-TW': '升級',
    en: 'Upgrade',
    ko: '업그레이드',
    ja: 'アップグレード',
  },
  // 降级
  downgrade: {
    'zh-CN': '降级',
    'zh-TW': '降級',
    en: 'Downgrade',
    ko: '다운그레이드',
    ja: 'ダウングレード',
  },
  // 取消
  cancel: {
    'zh-CN': '取消',
    'zh-TW': '取消',
    en: 'Cancel',
    ko: '취소',
    ja: 'キャンセル',
  },
  // 卸载 BepInEx 框架
  uninstallBepinex: {
    'zh-CN': '卸载 BepInEx 框架',
    'zh-TW': '解除安裝 BepInEx 框架',
    en: 'Uninstall BepInEx',
    ko: 'BepInEx 제거',
    ja: 'BepInEx をアンインストール',
  },
  // 将移除游戏目录中的 BepInEx 框架文件。建议先创建备份。继续卸载？
  thisRemovesBepinexFilesFromTheGameFolderCreateABackupFirstContinue: {
    'zh-CN': '将移除游戏目录中的 BepInEx 框架文件。建议先创建备份。继续卸载？',
    'zh-TW': '將移除遊戲目錄中的 BepInEx 框架檔案。建議先建立備份。繼續解除安裝？',
    en: 'This removes BepInEx files from the game folder. Create a backup first. Continue?',
    ko: '게임 폴더의 BepInEx 파일을 제거합니다. 먼저 백업하는 것이 좋습니다. 계속할까요?',
    ja: 'ゲームフォルダーの BepInEx ファイルを削除します。先にバックアップを推奨します。続行しますか？',
  },
  // 删除备份
  deleteBackup: {
    'zh-CN': '删除备份',
    'zh-TW': '刪除備份',
    en: 'Delete backup',
    ko: '백업 삭제',
    ja: 'バックアップを削除',
  },
  // 永久删除该备份？
  permanentlyDeleteThisBackup: {
    'zh-CN': '永久删除该备份？',
    'zh-TW': '永久刪除該備份？',
    en: 'Permanently delete this backup?',
    ko: '이 백업을 영구 삭제할까요?',
    ja: 'このバックアップを完全に削除しますか？',
  },
  // 已恢复
  restored: {
    'zh-CN': '已恢复',
    'zh-TW': '已還原',
    en: 'Restored',
    ko: '복원되었습니다',
    ja: '復元しました',
  },
  // 备份已删除
  backupDeleted: {
    'zh-CN': '备份已删除',
    'zh-TW': '備份已刪除',
    en: 'Backup deleted',
    ko: '백업이 삭제되었습니다',
    ja: 'バックアップを削除しました',
  },
  // 框架启用（游戏加载此框架）
  frameworkEnabledTheGameLoadsThisFramework: {
    'zh-CN': '框架启用（游戏加载此框架）',
    'zh-TW': '框架啟用（遊戲載入此框架）',
    en: 'Framework enabled (the game loads this framework)',
    ko: '프레임워크 활성화(게임이 이 프레임워크를 불러옵니다)',
    ja: 'フレームワーク有効（ゲームがこのフレームワークを読み込みます）',
  },
}
