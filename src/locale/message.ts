import type { DomainMessages } from './types'

/**
 * 后端**成功/状态提示** tag。
 *
 * 与 `error.ts` 对称：后端不再返回拼好的中文句子，而是 `message.<域>.<原因>`，
 * 前端翻译。带参数的用 `{}` 占位，参数由 tag 查询串传入。
 *
 * 为什么成功提示也要走 tag，而不是「反正成功提示没人看」：
 *   · 一致性 —— 一处走 tag、一处走中文，读代码时无法判断该用哪种；
 *   · 这些提示经常包含关键信息（备份保存到哪个路径），英文用户看不懂等于没有。
 */
export const message: DomainMessages = {
  'backup.created': {
    'zh-CN': '备份完成：{path}',
    'zh-TW': '備份完成：{path}',
    en: 'Backup created: {path}',
    ko: '백업 완료: {path}',
    ja: 'バックアップ完了：{path}',
  },
  'backup.bepInExRestored': {
    'zh-CN': 'BepInEx 备份已恢复',
    'zh-TW': 'BepInEx 備份已還原',
    en: 'BepInEx backup restored',
    ko: 'BepInEx 백업이 복원되었습니다',
    ja: 'BepInEx バックアップを復元しました',
  },
  'backup.diyLevelRestored': {
    'zh-CN': 'DIYLevel 备份已恢复',
    'zh-TW': 'DIYLevel 備份已還原',
    en: 'DIYLevel backup restored',
    ko: 'DIYLevel 백업이 복원되었습니다',
    ja: 'DIYLevel バックアップを復元しました',
  },
  'backup.deleted': {
    'zh-CN': '备份已删除',
    'zh-TW': '備份已刪除',
    en: 'Backup deleted',
    ko: '백업이 삭제되었습니다',
    ja: 'バックアップを削除しました',
  },
  'framework.movedToRecycleBin': {
    'zh-CN': '已移入回收站（{count} 个文件），备份保存在 {path}',
    'zh-TW': '已移入資源回收筒（{count} 個檔案），備份保存在 {path}',
    en: 'Moved {count} files to the recycle bin; backup saved at {path}',
    ko: '파일 {count}개를 휴지통으로 옮겼습니다. 백업 위치: {path}',
    ja: '{count} 個のファイルをゴミ箱へ移動しました。バックアップ：{path}',
  },
  'framework.diyLevelMovedToRecycleBin': {
    'zh-CN': 'DIYLevel 已移入回收站（{count} 个文件），备份保存在 {path}',
    'zh-TW': 'DIYLevel 已移入資源回收筒（{count} 個檔案），備份保存在 {path}',
    en: 'DIYLevel moved {count} files to the recycle bin; backup saved at {path}',
    ko: 'DIYLevel 파일 {count}개를 휴지통으로 옮겼습니다. 백업 위치: {path}',
    ja: 'DIYLevel の {count} 個のファイルをゴミ箱へ移動しました。バックアップ：{path}',
  },
  'framework.diyLevelMovedToRecycleBinNoCount': {
    'zh-CN': 'DIYLevel 已移入回收站，备份保存在 {path}',
    'zh-TW': 'DIYLevel 已移入資源回收筒，備份保存在 {path}',
    en: 'DIYLevel moved to the recycle bin; backup saved at {path}',
    ko: 'DIYLevel을 휴지통으로 옮겼습니다. 백업 위치: {path}',
    ja: 'DIYLevel をゴミ箱へ移動しました。バックアップ：{path}',
  },
  'framework.diyLevelDeleted': {
    'zh-CN': 'DIYLevel 已删除，备份保存在 {path}',
    'zh-TW': 'DIYLevel 已刪除，備份保存在 {path}',
    en: 'DIYLevel deleted; backup saved at {path}',
    ko: 'DIYLevel이 삭제되었습니다. 백업 위치: {path}',
    ja: 'DIYLevel を削除しました。バックアップ：{path}',
  },
  'framework.diyLevelEnabled': {
    'zh-CN': 'DIYLevel 框架已启用',
    'zh-TW': 'DIYLevel 框架已啟用',
    en: 'DIYLevel framework enabled',
    ko: 'DIYLevel 프레임워크가 활성화되었습니다',
    ja: 'DIYLevel フレームワークを有効にしました',
  },
  'framework.diyLevelDisabled': {
    'zh-CN': 'DIYLevel 框架已禁用',
    'zh-TW': 'DIYLevel 框架已禁用',
    en: 'DIYLevel framework disabled',
    ko: 'DIYLevel 프레임워크가 비활성화되었습니다',
    ja: 'DIYLevel フレームワークを無効にしました',
  },
  'framework.diyLevelDirOpened': {
    'zh-CN': '已打开 DIYLevel 目录',
    'zh-TW': '已開啟 DIYLevel 目錄',
    en: 'Opened the DIYLevel folder',
    ko: 'DIYLevel 폴더를 열었습니다',
    ja: 'DIYLevel フォルダーを開きました',
  },
  'framework.stateEnabled': {
    'zh-CN': '已启用',
    'zh-TW': '已啟用',
    en: 'Enabled',
    ko: '활성화됨',
    ja: '有効',
  },
  'framework.stateDisabled': {
    'zh-CN': '已禁用',
    'zh-TW': '已禁用',
    en: 'Disabled',
    ko: '비활성화됨',
    ja: '無効',
  },

  // ===== MOD 卸载 =====
  'mods.movedToRecycleBin': {
    'zh-CN': 'MOD 已移入回收站',
    'zh-TW': 'MOD 已移入資源回收筒',
    en: 'MOD moved to the recycle bin',
    ko: 'MOD를 휴지통으로 옮겼습니다',
    ja: 'MOD をゴミ箱へ移動しました',
  },
  'mods.deleted': {
    'zh-CN': 'MOD 已删除',
    'zh-TW': 'MOD 已刪除',
    en: 'MOD deleted',
    ko: 'MOD가 삭제되었습니다',
    ja: 'MOD を削除しました',
  },
  'mods.packageMovedToRecycleBin': {
    'zh-CN': 'MOD 包已移入回收站',
    'zh-TW': 'MOD 包已移入資源回收筒',
    en: 'MOD package moved to the recycle bin',
    ko: 'MOD 패키지를 휴지통으로 옮겼습니다',
    ja: 'MOD パッケージをゴミ箱へ移動しました',
  },
  'mods.packageDeleted': {
    'zh-CN': 'MOD 包已删除',
    'zh-TW': 'MOD 包已刪除',
    en: 'MOD package deleted',
    ko: 'MOD 패키지가 삭제되었습니다',
    ja: 'MOD パッケージを削除しました',
  },
  'mods.packageMovedToRecycleBinWithCount': {
    'zh-CN': 'MOD 包已移入回收站（{count} 个文件）',
    'zh-TW': 'MOD 包已移入資源回收筒（{count} 個檔案）',
    en: 'MOD package moved to the recycle bin ({count} files)',
    ko: 'MOD 패키지를 휴지통으로 옮겼습니다(파일 {count}개)',
    ja: 'MOD パッケージをゴミ箱へ移動しました（{count} ファイル）',
  },
  'mods.bulkToggleEnabled': {
    'zh-CN': '{count} 个 MOD 已启用',
    'zh-TW': '{count} 個 MOD 已啟用',
    en: '{count} MODs enabled',
    ko: 'MOD {count}개를 활성화했습니다',
    ja: '{count} 個の MOD を有効にしました',
  },
  'mods.bulkToggleDisabled': {
    'zh-CN': '{count} 个 MOD 已禁用',
    'zh-TW': '{count} 個 MOD 已停用',
    en: '{count} MODs disabled',
    ko: 'MOD {count}개를 비활성화했습니다',
    ja: '{count} 個の MOD を無効にしました',
  },
  'levelSet.enabled': {
    'zh-CN': '地图已启用',
    'zh-TW': '地圖已啟用',
    en: 'Map enabled',
    ko: '지도를 활성화했습니다',
    ja: 'マップを有効にしました',
  },
  'levelSet.disabled': {
    'zh-CN': '地图已停用',
    'zh-TW': '地圖已停用',
    en: 'Map disabled',
    ko: '지도를 비활성화했습니다',
    ja: 'マップを無効にしました',
  },
  'levelSet.dirOpened': {
    'zh-CN': '已打开地图目录',
    'zh-TW': '已開啟地圖目錄',
    en: 'Opened the map folder',
    ko: '지도 폴더를 열었습니다',
    ja: 'マップフォルダーを開きました',
  },
  'levelSet.movedToRecycleBin': {
    'zh-CN': '地图已移入回收站',
    'zh-TW': '地圖已移入資源回收筒',
    en: 'Map moved to the recycle bin',
    ko: '지도를 휴지통으로 옮겼습니다',
    ja: 'マップをゴミ箱へ移動しました',
  },
  'levelSet.deleted': {
    'zh-CN': '地图已删除',
    'zh-TW': '地圖已刪除',
    en: 'Map deleted',
    ko: '지도가 삭제되었습니다',
    ja: 'マップを削除しました',
  },
  'levelSet.bulkEnabled': {
    'zh-CN': '{count} 张地图已启用',
    'zh-TW': '{count} 張地圖已啟用',
    en: '{count} map(s) enabled',
    ko: '지도 {count}개를 활성화했습니다',
    ja: 'マップ {count} 個を有効にしました',
  },
  'levelSet.bulkDisabled': {
    'zh-CN': '{count} 张地图已禁用',
    'zh-TW': '{count} 張地圖已停用',
    en: '{count} map(s) disabled',
    ko: '지도 {count}개를 비활성화했습니다',
    ja: 'マップ {count} 個を無効にしました',
  },
  'levelSet.nothingToToggle': {
    'zh-CN': '所有地图已经是这个状态',
    'zh-TW': '所有地圖已經是這個狀態',
    en: 'Every map is already in that state',
    ko: '모든 지도가 이미 이 상태입니다',
    ja: 'すべてのマップが既にその状態です',
  },
  // 地图发布页的写操作结果（后端 `OperationResult.message`）
  'mapPublish.releaseDeleted': {
    'zh-CN': '版本已删除',
    'zh-TW': '版本已刪除',
    en: 'Release deleted',
    ko: '릴리스가 삭제되었습니다',
    ja: 'リリースを削除しました',
  },
  'mapPublish.assetDeleted': {
    'zh-CN': '附件已删除',
    'zh-TW': '附件已刪除',
    en: 'Asset deleted',
    ko: '첨부 파일이 삭제되었습니다',
    ja: '添付ファイルを削除しました',
  },
}
