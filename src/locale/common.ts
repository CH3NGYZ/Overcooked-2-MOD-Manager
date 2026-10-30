import type { DomainMessages } from './types'

/**
 * 通用文案：跨页面复用的动作词与状态词。
 *
 * 只放「换了页面也一样叫这个」的词。带上下文的文案（例如「删除 MOD 包」）
 * 属于具体域，不要为了复用把语义磨平。
 */
export const common: DomainMessages = {
  'action.ok': {
    'zh-CN': '确定',
    'zh-TW': '確定',
    en: 'OK',
    ko: '확인',
    ja: 'OK',
  },
  'action.cancel': {
    'zh-CN': '取消',
    'zh-TW': '取消',
    en: 'Cancel',
    ko: '취소',
    ja: 'キャンセル',
  },
  'action.save': {
    'zh-CN': '保存',
    'zh-TW': '儲存',
    en: 'Save',
    ko: '저장',
    ja: '保存',
  },
  'action.delete': {
    'zh-CN': '删除',
    'zh-TW': '刪除',
    en: 'Delete',
    ko: '삭제',
    ja: '削除',
  },
  'action.open': {
    'zh-CN': '打开',
    'zh-TW': '開啟',
    en: 'Open',
    ko: '열기',
    ja: '開く',
  },
  'action.retry': {
    'zh-CN': '重试',
    'zh-TW': '重試',
    en: 'Retry',
    ko: '다시 시도',
    ja: '再試行',
  },
  'action.refresh': {
    'zh-CN': '刷新',
    'zh-TW': '重新整理',
    en: 'Refresh',
    ko: '새로 고침',
    ja: '更新',
  },
  'action.copy': {
    'zh-CN': '复制',
    'zh-TW': '複製',
    en: 'Copy',
    ko: '복사',
    ja: 'コピー',
  },
  'action.copied': {
    'zh-CN': '已复制',
    'zh-TW': '已複製',
    en: 'Copied',
    ko: '복사되었습니다',
    ja: 'コピーしました',
  },
  'action.restore': {
    'zh-CN': '恢复',
    'zh-TW': '還原',
    en: 'Restore',
    ko: '복원',
    ja: '復元',
  },
  'action.done': {
    'zh-CN': '完成',
    'zh-TW': '完成',
    en: 'Done',
    ko: '완료',
    ja: '完了',
  },
  'state.enabled': {
    'zh-CN': '已启用',
    'zh-TW': '已啟用',
    en: 'Enabled',
    ko: '활성화됨',
    ja: '有効',
  },
  'state.disabled': {
    'zh-CN': '已禁用',
    'zh-TW': '已禁用',
    en: 'Disabled',
    ko: '비활성화됨',
    ja: '無効',
  },
  'state.installed': {
    'zh-CN': '已安装',
    'zh-TW': '已安裝',
    en: 'Installed',
    ko: '설치됨',
    ja: 'インストール済み',
  },
  'state.notInstalled': {
    'zh-CN': '未安装',
    'zh-TW': '未安裝',
    en: 'Not installed',
    ko: '설치되지 않음',
    ja: '未インストール',
  },
  'state.version': {
    'zh-CN': '版本',
    'zh-TW': '版本',
    en: 'Version',
    ko: '버전',
    ja: 'バージョン',
  },
  'state.versionUnknown': {
    'zh-CN': '版本未知',
    'zh-TW': '版本未知',
    en: 'Unknown version',
    ko: '버전을 알 수 없음',
    ja: 'バージョン不明',
  },
  'loading': {
    'zh-CN': '加载中…',
    'zh-TW': '載入中…',
    en: 'Loading…',
    ko: '불러오는 중…',
    ja: '読み込み中…',
  },
  other: {
    'zh-CN': '其他',
    'zh-TW': '其他',
    en: 'Other',
    ko: '기타',
    ja: 'その他',
  },
}
