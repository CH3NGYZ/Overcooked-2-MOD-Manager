import type { DomainMessages } from '../types.ts'

export const diychefAppearance: DomainMessages = {
  changeHat: { 'zh-CN': '换帽子', 'zh-TW': '換帽子', en: 'Change hat', ko: '모자 변경', ja: '帽子を変更' },
  hatTitle: { 'zh-CN': '给 {name} 换帽子', 'zh-TW': '給 {name} 換帽子', en: 'Choose a hat for {name}', ko: '{name}의 모자 선택', ja: '{name}の帽子を選択' },
  hatSearch: { 'zh-CN': '搜索帽子', 'zh-TW': '搜尋帽子', en: 'Search hats', ko: '모자 검색', ja: '帽子を検索' },
  hatAuto: { 'zh-CN': '跟随游戏', 'zh-TW': '跟隨遊戲', en: 'Follow the game', ko: '게임 설정 따르기', ja: 'ゲームに合わせる' },
  hatNone: { 'zh-CN': '不戴帽子', 'zh-TW': '不戴帽子', en: 'No hat', ko: '모자 없음', ja: '帽子なし' },
  hatFancy: { 'zh-CN': '厨师帽', 'zh-TW': '廚師帽', en: 'Chef hat', ko: '셰프 모자', ja: 'コック帽' },
  hatSanta: { 'zh-CN': '圣诞帽', 'zh-TW': '聖誕帽', en: 'Santa hat', ko: '산타 모자', ja: 'サンタ帽' },
  hatBaseball: { 'zh-CN': '棒球帽', 'zh-TW': '棒球帽', en: 'Baseball cap', ko: '야구 모자', ja: '野球帽' },
  hatAutoHint: { 'zh-CN': '帽子由游戏场景决定', 'zh-TW': '帽子由遊戲場景決定', en: 'The game scene determines the hat', ko: '게임 장면에 따라 모자가 결정됩니다', ja: 'ゲームのシーンによって帽子が決まります' },
  hatApply: { 'zh-CN': '应用帽子', 'zh-TW': '套用帽子', en: 'Apply hat', ko: '모자 적용', ja: '帽子を適用' },
  hatApplyWithoutPreview: { 'zh-CN': '保存帽子选择', 'zh-TW': '儲存帽子選擇', en: 'Save hat choice', ko: '모자 선택 저장', ja: '帽子の選択を保存' },
  hatDisabledHint: { 'zh-CN': '试帽不会启用这位厨师，选择仅在本次编辑中保留。', 'zh-TW': '試帽不會啟用這位廚師，選擇僅在本次編輯中保留。', en: 'Trying a hat does not enable this chef. The choice is kept for this editing session.', ko: '모자를 골라도 셰프가 활성화되지는 않습니다. 이번 편집 동안만 선택이 유지됩니다.', ja: '帽子を選んでもこのシェフは有効になりません。今回の編集中のみ保持します。' },
  previewUnavailable: { 'zh-CN': '暂不可预览', 'zh-TW': '暫不可預覽', en: 'Preview unavailable', ko: '미리보기 불가', ja: 'プレビューできません' },
  previewUnavailableHint: { 'zh-CN': '当前组合暂不可完整预览，仍可保存有效的帽子选择。', 'zh-TW': '目前組合暫不可完整預覽，仍可儲存有效的帽子選擇。', en: 'This combination cannot be fully previewed. A valid hat choice can still be saved.', ko: '이 조합의 전체 미리보기가 불가능합니다. 유효한 모자 선택은 저장할 수 있습니다.', ja: 'この組み合わせを完全には表示できません。有効な帽子の選択は保存できます。' },
  previewPartial: { 'zh-CN': '部分预览', 'zh-TW': '部分預覽', en: 'Partial preview', ko: '일부 미리보기', ja: '一部のプレビュー' },
  rotateHint: { 'zh-CN': '拖动查看不同角度', 'zh-TW': '拖曳查看不同角度', en: 'Drag to view another angle', ko: '끌어서 다른 각도 보기', ja: 'ドラッグして角度を変更' },
  retry: { 'zh-CN': '重试', 'zh-TW': '重試', en: 'Retry', ko: '다시 시도', ja: '再試行' },
  noHats: { 'zh-CN': '没有匹配的帽子', 'zh-TW': '沒有符合的帽子', en: 'No matching hats', ko: '일치하는 모자가 없습니다', ja: '一致する帽子がありません' },
  currentUnknownHat: { 'zh-CN': '未识别的帽子：{name}', 'zh-TW': '未辨識的帽子：{name}', en: 'Unrecognized hat: {name}', ko: '알 수 없는 모자: {name}', ja: '認識できない帽子：{name}' },
}
