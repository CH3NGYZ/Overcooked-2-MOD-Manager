import type { DomainMessages } from '../types.ts'

/** 共用依赖表的关系、发布线、版本要求与满足状态。同一 key 的五语保持集中。 */
export const toolsUiRelations: DomainMessages = {
  // 依赖
  dependency: {
    'zh-CN': '依赖',
    'zh-TW': '相依',
    en: 'Dependency',
    ko: '의존성',
    ja: '依存ファイル',
  },
  // 依赖表：名称列（目标 MOD）
  modColumn: {
    'zh-CN': 'MOD',
    'zh-TW': 'MOD',
    en: 'MOD',
    ko: 'MOD',
    ja: 'MOD',
  },
  // 依赖表：关系（requires / conflicts / compatible）
  relation: {
    'zh-CN': '关系',
    'zh-TW': '關係',
    en: 'Relation',
    ko: '관계',
    ja: '関係',
  },
  // 依赖表：关系值 requires
  relationRequires: {
    'zh-CN': '依赖',
    'zh-TW': '相依',
    en: 'Requires',
    ko: '의존',
    ja: '依存',
  },
  // 依赖表：关系值 conflicts（与指定版本区间互斥）
  relationConflicts: {
    'zh-CN': '排斥',
    'zh-TW': '排斥',
    en: 'Conflicts',
    ko: '충돌',
    ja: '競合',
  },
  // 依赖表：关系值 compatible
  relationCompatible: {
    'zh-CN': '兼容',
    'zh-TW': '相容',
    en: 'Compatible',
    ko: '호환',
    ja: '互換',
  },
  // 依赖表：关系只对测试版成立（与另一条发布线互斥）
  testLine: {
    'zh-CN': '测试版',
    'zh-TW': '測試版',
    en: 'Beta',
    ko: '테스트 버전',
    ja: 'テスト版',
  },
  // 依赖表：关系只对正式版成立
  stableLine: {
    'zh-CN': '正式版',
    'zh-TW': '正式版',
    en: 'Stable',
    ko: '정식 버전',
    ja: '正式版',
  },
  // 依赖表：版本要求
  requirement: {
    'zh-CN': '要求',
    'zh-TW': '要求',
    en: 'Requirement',
    ko: '요구 사항',
    ja: '要件',
  },
  // 依赖表：状态列
  status: {
    'zh-CN': '状态',
    'zh-TW': '狀態',
    en: 'Status',
    ko: '상태',
    ja: '状態',
  },
  // 依赖已满足（本地装了且版本在区间内）
  satisfied: {
    'zh-CN': '已满足',
    'zh-TW': '已滿足',
    en: 'Satisfied',
    ko: '충족',
    ja: '充足',
  },
  // 依赖未装、但线上有满足区间的版本可装
  installable: {
    'zh-CN': '可安装',
    'zh-TW': '可安裝',
    en: 'Installable',
    ko: '설치 가능',
    ja: 'インストール可',
  },
  // 依赖已装但版本不满足：线上有满足区间的版本，去把它换上
  updatable: {
    'zh-CN': '可更新',
    'zh-TW': '可更新',
    en: 'Updatable',
    ko: '업데이트 가능',
    ja: '更新可能',
  },
  // conflicts 关系未命中（对方不在冲突区间内）
  noConflict: {
    'zh-CN': '无冲突',
    'zh-TW': '無衝突',
    en: 'No conflict',
    ko: '충돌 없음',
    ja: '競合なし',
  },
  // compatible 关系满足
  compatible: {
    'zh-CN': '兼容',
    'zh-TW': '相容',
    en: 'Compatible',
    ko: '호환',
    ja: '互換',
  },
  // compatible 关系不满足
  incompatible: {
    'zh-CN': '不兼容',
    'zh-TW': '不相容',
    en: 'Incompatible',
    ko: '비호환',
    ja: '非互換',
  },
}
