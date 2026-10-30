import type { DomainMessages } from './types'

/**
 * common 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const uiCommon: DomainMessages = {
  // 保存并验证
  saveAndVerify: {
    'zh-CN': '保存并验证',
    'zh-TW': '儲存並驗證',
    en: 'Save and verify',
    ko: '저장 후 검증',
    ja: '保存して検証',
  },
  // 恢复默认
  reset: {
    'zh-CN': '恢复默认',
    'zh-TW': '還原預設',
    en: 'Reset',
    ko: '기본값 복원',
    ja: '既定に戻す',
  },
  // 已保存
  saved: {
    'zh-CN': '已保存',
    'zh-TW': '已儲存',
    en: 'Saved',
    ko: '저장됨',
    ja: '保存しました',
  },
  // 游戏运行中
  gameRunning: {
    'zh-CN': '游戏运行中',
    'zh-TW': '遊戲執行中',
    en: 'Game running',
    ko: '게임 실행 중',
    ja: 'ゲーム実行中',
  },
  // 取消下载
  cancelDownload: {
    'zh-CN': '取消下载',
    'zh-TW': '取消下載',
    en: 'Cancel',
    ko: '다운로드 취소',
    ja: 'ダウンロード中止',
  },
  // 等待大小
  waitingForSize: {
    'zh-CN': '等待大小',
    'zh-TW': '等待大小',
    en: 'Waiting for size',
    ko: '크기 대기',
    ja: 'サイズ待ち',
  },
  // 代理
  proxy: {
    'zh-CN': '代理',
    'zh-TW': '代理',
    en: 'Proxy',
    ko: '프록시',
    ja: 'プロキシ',
  },
  // 直连
  direct: {
    'zh-CN': '直连',
    'zh-TW': '直連',
    en: 'Direct',
    ko: '직접 연결',
    ja: '直接',
  },
  // 这个目录不存在或缺少游戏文件
  theFolderIsMissingOrHasNoGameFiles: {
    'zh-CN': '这个目录不存在或缺少游戏文件',
    'zh-TW': '這個目錄不存在或缺少遊戲檔案',
    en: 'The folder is missing or has no game files',
    ko: '폴더가 없거나 게임 파일이 없습니다',
    ja: 'フォルダーがないか、ゲームファイルがありません',
  },
  // 保存后自动验证。
  verifiedAutomaticallyAfterSaving: {
    'zh-CN': '保存后自动验证。',
    'zh-TW': '儲存後自動驗證。',
    en: 'Verified automatically after saving.',
    ko: '저장 후 자동으로 검증합니다.',
    ja: '保存後に自動検証します。',
  },
  // 文件不存在
  fileNotFound: {
    'zh-CN': '文件不存在',
    'zh-TW': '檔案不存在',
    en: 'File not found',
    ko: '파일 없음',
    ja: 'ファイルがありません',
  },
  // 游戏目录已变化，请重新添加模组
  gameDirectoryChangedReAddTheMods: {
    'zh-CN': '游戏目录已变化，请重新添加模组',
    'zh-TW': '遊戲目錄已變更，請重新新增模組',
    en: 'Game directory changed; re-add the mods',
    ko: '게임 폴더가 변경되었습니다. 모드를 다시 추가하세요',
    ja: 'ゲームフォルダーが変更されました。MODを再追加してください',
  },
}
