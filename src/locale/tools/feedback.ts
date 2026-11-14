import type { DomainMessages } from '../types.ts'

/** 工具页反馈渠道。同一 key 的五语保持集中。 */
export const toolsUiFeedback: DomainMessages = {
  feedbackChannels: {
    'zh-CN': '反馈渠道',
    'zh-TW': '回饋管道',
    en: 'Feedback channels',
    ko: '피드백 채널',
    ja: 'フィードバック窓口',
  },
  arcadeModFeedbackGroup1: {
    'zh-CN': '街机MOD反馈群1',
    'zh-TW': '街機MOD回饋群1',
    en: 'Arcade MOD feedback group 1',
    ko: '아케이드 MOD 피드백 그룹 1',
    ja: 'アーケードMOD フィードバックグループ1',
  },
  arcadeModFeedbackGroup2: {
    'zh-CN': '街机MOD反馈群2',
    'zh-TW': '街機MOD回饋群2',
    en: 'Arcade MOD feedback group 2',
    ko: '아케이드 MOD 피드백 그룹 2',
    ja: 'アーケードMOD フィードバックグループ2',
  },
  modManagerFeedbackGroup: {
    'zh-CN': '模组管理器反馈群',
    'zh-TW': '模組管理器回饋群',
    en: 'MOD Manager feedback group',
    ko: 'MOD 관리자 피드백 그룹',
    ja: 'MODマネージャー フィードバックグループ',
  },
  qqFeedbackGroupDescription: {
    'zh-CN': '在 QQ 群中反馈问题与建议',
    'zh-TW': '在 QQ 群中回報問題與建議',
    en: 'Share issues and suggestions in the QQ group',
    ko: 'QQ 그룹에서 문제와 제안을 공유하세요',
    ja: 'QQグループで問題やご意見をお寄せください',
  },
}
