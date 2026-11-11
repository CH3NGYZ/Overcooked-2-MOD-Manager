import type { DomainMessages } from './types'

export const tutorialManager: DomainMessages = {
  'manager.title': {
    'zh-CN': '教程管理', 'zh-TW': '教學管理', en: 'Tutorial manager', ko: '튜토리얼 관리', ja: 'チュートリアル管理',
  },
  'manager.description': {
    'zh-CN': '按章节查看已看进度，重播整章或从某一步开始。',
    'zh-TW': '依章節查看已看進度，重播整章或從某一步開始。',
    en: 'Check your progress by chapter, replay a chapter, or start at any step.',
    ko: '장별로 시청 진행 상황을 확인하고, 장 전체를 다시 보거나 원하는 단계부터 시작하세요.',
    ja: '章ごとの進捗を確認し、章全体や好きなステップから再生できます。',
  },
  'manager.basics': {
    'zh-CN': '界面基础', 'zh-TW': '介面基礎', en: 'Interface basics', ko: '인터페이스 기초', ja: '画面の基本操作',
  },
  'manager.progress': {
    'zh-CN': '已看 {seen} / {total}', 'zh-TW': '已看 {seen} / {total}', en: '{seen} / {total} viewed',
    ko: '{seen} / {total} 시청', ja: '{seen} / {total} 閲覧済み',
  },
  'manager.stepCount': {
    'zh-CN': '{count} 步', 'zh-TW': '{count} 步', en: '{count} steps', ko: '{count}단계', ja: '{count} ステップ',
  },
  'manager.seen': {
    'zh-CN': '已看', 'zh-TW': '已看', en: 'Viewed', ko: '시청 완료', ja: '閲覧済み',
  },
  'manager.unseen': {
    'zh-CN': '未看', 'zh-TW': '未看', en: 'Unviewed', ko: '미시청', ja: '未閲覧',
  },
  'manager.loading': {
    'zh-CN': '正在读取已看进度…', 'zh-TW': '正在讀取已看進度…', en: 'Loading viewing progress…',
    ko: '시청 진행 상황을 불러오는 중…', ja: '閲覧状況を読み込み中…',
  },
  'manager.progressUnknown': {
    'zh-CN': '已看进度暂不可用', 'zh-TW': '已看進度暫不可用', en: 'Viewing progress is unavailable',
    ko: '시청 진행 상황을 확인할 수 없습니다', ja: '閲覧状況を取得できません',
  },
  'manager.readFailed': {
    'zh-CN': '读取进度失败，仍可选择教程播放，或重试读取。',
    'zh-TW': '讀取進度失敗，仍可選擇教學播放，或重試讀取。',
    en: 'Progress could not be loaded. You can still play a tutorial or try again.',
    ko: '진행 상황을 불러오지 못했습니다. 튜토리얼을 재생하거나 다시 시도할 수 있습니다.',
    ja: '閲覧状況を読み込めませんでした。チュートリアルの再生や再読み込みは引き続きできます。',
  },
  'manager.retry': {
    'zh-CN': '重新读取', 'zh-TW': '重新讀取', en: 'Retry', ko: '다시 불러오기', ja: '再読み込み',
  },
  'manager.playSection': {
    'zh-CN': '重播整章', 'zh-TW': '重播整章', en: 'Replay chapter', ko: '장 다시 보기', ja: '章を再生',
  },
  'manager.playStep': {
    'zh-CN': '从此步开始', 'zh-TW': '從此步開始', en: 'Start here', ko: '여기서 시작', ja: 'ここから再生',
  },
  'manager.playStepLabel': {
    'zh-CN': '从“{title}”开始播放', 'zh-TW': '從「{title}」開始播放', en: 'Start from “{title}”',
    ko: '“{title}”부터 재생', ja: '「{title}」から再生',
  },
  'manager.playAll': {
    'zh-CN': '重播全部教程', 'zh-TW': '重播全部教學', en: 'Replay all tutorials', ko: '전체 튜토리얼 다시 보기', ja: 'すべて再生',
  },
  'manager.empty': {
    'zh-CN': '当前平台没有可播放的教程步骤。', 'zh-TW': '目前平台沒有可播放的教學步驟。',
    en: 'No tutorial steps are available on this platform.', ko: '이 플랫폼에서 재생할 수 있는 튜토리얼 단계가 없습니다.',
    ja: 'このプラットフォームで再生できるステップはありません。',
  },
}
