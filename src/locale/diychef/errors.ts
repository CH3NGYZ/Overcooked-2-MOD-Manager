import type { DomainMessages } from '../types.ts'

export const diychefErrors: DomainMessages = {
  lastChef: { 'zh-CN': '至少保留一位可用厨师', 'zh-TW': '至少保留一位可用廚師', en: 'Keep at least one usable chef', ko: '사용 가능한 셰프를 한 명 이상 남겨 주세요', ja: '利用可能なシェフを1人以上残してください' },
  unavailable: { 'zh-CN': '这位厨师的资源不可用', 'zh-TW': '這位廚師的資源不可用', en: 'This chef’s resources are unavailable', ko: '이 셰프의 리소스를 사용할 수 없습니다', ja: 'このシェフのリソースは利用できません' },
  gameRunning: { 'zh-CN': '游戏运行中，可以继续编辑；退出游戏后才能保存。', 'zh-TW': '遊戲執行中，可以繼續編輯；退出遊戲後才能儲存。', en: 'The game is running. You can edit, but close the game before saving.', ko: '게임 실행 중에는 편집할 수 있지만 저장하려면 게임을 종료해야 합니다.', ja: 'ゲーム実行中も編集できますが、保存するにはゲームを終了してください。' },
  readOnly: { 'zh-CN': '此版本的配置格式尚未确认，当前仅供查看。', 'zh-TW': '此版本的配置格式尚未確認，目前僅供檢視。', en: 'This version’s format has not been verified. Viewing only.', ko: '이 버전의 설정 형식은 확인되지 않았습니다. 보기만 가능합니다.', ja: 'このバージョンの設定形式は未確認のため閲覧のみ可能です。' },
  missingConfig: { 'zh-CN': '配置尚未创建，退出游戏后将初始化为黑猫。', 'zh-TW': '配置尚未建立，退出遊戲後將初始化為黑貓。', en: 'No configuration exists yet. It will be initialized with the black cat when the game closes.', ko: '설정이 아직 없습니다. 게임 종료 후 검은 고양이로 초기화됩니다.', ja: '設定はまだありません。ゲーム終了後に黒猫で初期化します。' },
  problems: { 'zh-CN': '配置中有 {count} 项需要注意', 'zh-TW': '配置中有 {count} 項需要注意', en: '{count} configuration records need attention', ko: '설정 기록 {count}개를 확인해 주세요', ja: '設定の{count}項目を確認してください' },
  duplicateRow: { 'zh-CN': '重复记录', 'zh-TW': '重複記錄', en: 'Duplicate record', ko: '중복 기록', ja: '重複行' },
  unresolvedRow: { 'zh-CN': '资源缺失或名称未识别', 'zh-TW': '資源缺少或名稱未辨識', en: 'Missing resources or unrecognized name', ko: '리소스 누락 또는 알 수 없는 이름', ja: 'リソースがないか名前を認識できません' },
  removeProblem: { 'zh-CN': '删除这条记录', 'zh-TW': '刪除這筆記錄', en: 'Remove this record', ko: '이 기록 삭제', ja: 'この行を削除' },
  validationBlocked: { 'zh-CN': '请先处理阻止保存的配置问题', 'zh-TW': '請先處理阻止儲存的配置問題', en: 'Resolve the blocking configuration issues before saving', ko: '저장을 막는 설정 문제를 먼저 해결해 주세요', ja: '保存を妨げている設定の問題を先に解決してください' },
  catalogUnavailable: { 'zh-CN': '厨师目录不完整，请重新读取', 'zh-TW': '廚師目錄不完整，請重新讀取', en: 'The chef catalogue is incomplete. Reload it.', ko: '셰프 목록이 불완전합니다. 다시 읽어 주세요.', ja: 'シェフの一覧が不完全です。再読み込みしてください。' },
  staleTarget: { 'zh-CN': '本地 MOD 已变化，请先重新读取；当前草稿已保留。', 'zh-TW': '本機 MOD 已變更，請先重新讀取；目前草稿已保留。', en: 'Local MODs changed. Reload before saving; your draft has been kept.', ko: '로컬 MOD가 변경되었습니다. 초안은 유지되며 저장 전에 다시 읽어야 합니다.', ja: 'ローカルMODが変わりました。下書きは保持しています。保存前に再読み込みしてください。' },
}
