import type { DomainMessages } from './types'

/**
 * rename 域的界面文案。
 *
 * 迁移自旧 `src/i18n.ts` 的中文 key 表。key 是从英文译文派生的
 * camelCase 名，每条上方保留中文原文注释，便于和旧代码、截图对照。
 */
export const renameUi: DomainMessages = {
  // 输入名字
  enterName: {
    'zh-CN': '输入名字',
    'zh-TW': '輸入名字',
    en: 'Enter name',
    ko: '이름 입력',
    ja: '名前を入力',
  },
  // 支持任意字符，例如中文、英文与符号
  anyCharactersEGChineseEnglishSymbols: {
    'zh-CN': '支持任意字符，例如中文、英文与符号',
    'zh-TW': '支援任意字元，例如中文、英文與符號',
    en: 'Any characters, e.g. Chinese, English, symbols',
    ko: '모든 문자 사용 가능 (예: 중국어, 영어, 기호)',
    ja: '任意の文字（中文・英字・記号）に対応',
  },
  // 在这里输入要修改的名字
  enterTheNameYouWantToRestyle: {
    'zh-CN': '在这里输入要修改的名字',
    'zh-TW': '在這裡輸入要修改的名字',
    en: 'Enter the name you want to restyle',
    ko: '꾸밀 이름을 입력하세요',
    ja: '装飾したい名前を入力',
  },
  // 请输入名字
  pleaseEnterAName: {
    'zh-CN': '请输入名字',
    'zh-TW': '請輸入名字',
    en: 'Please enter a name',
    ko: '이름을 입력하세요',
    ja: '名前を入力してください',
  },
  // 改多彩名
  colorfulName: {
    'zh-CN': '改多彩名',
    'zh-TW': '改多彩名',
    en: 'Colorful name',
    ko: '다채로운 이름',
    ja: 'カラフル名',
  },
  // 开始自定义
  startCustomizing: {
    'zh-CN': '开始自定义',
    'zh-TW': '開始自訂',
    en: 'Start customizing',
    ko: '커스터마이즈 시작',
    ja: 'カスタマイズ開始',
  },
  // 重新输入
  startOver: {
    'zh-CN': '重新输入',
    'zh-TW': '重新輸入',
    en: 'Start over',
    ko: '다시 입력',
    ja: '入力し直す',
  },
  // 选择要修改的文字
  selectTextToEdit: {
    'zh-CN': '选择要修改的文字',
    'zh-TW': '選擇要修改的文字',
    en: 'Select text to edit',
    ko: '수정할 텍스트 선택',
    ja: '編集する文字を選択',
  },
  // 拖动选中预览里的字符，再应用颜色或字号
  dragSelectCharactersInThePreviewThenApplyColorOrSize: {
    'zh-CN': '拖动选中预览里的字符，再应用颜色或字号',
    'zh-TW': '拖曳選取預覽裡的字符，再套用顏色或字號',
    en: 'Drag-select characters in the preview, then apply color or size',
    ko: '미리보기에서 글자를 드래그해 선택한 뒤 색상이나 크기를 적용하세요',
    ja: 'プレビュー内の文字をドラッグ選択して色やサイズを適用します',
  },
  // 请先选择要修改的文字
  selectSomeTextFirst: {
    'zh-CN': '请先选择要修改的文字',
    'zh-TW': '請先選擇要修改的文字',
    en: 'Select some text first',
    ko: '먼저 텍스트를 선택하세요',
    ja: '先に文字を選択してください',
  },
  // 选择颜色
  chooseColor: {
    'zh-CN': '选择颜色',
    'zh-TW': '選擇顏色',
    en: 'Choose color',
    ko: '색상 선택',
    ja: '色を選択',
  },
  // 更多颜色
  moreColors: {
    'zh-CN': '更多颜色',
    'zh-TW': '更多顏色',
    en: 'More colors',
    ko: '추가 색상',
    ja: 'その他の色',
  },
  // 字号增量
  sizeDelta: {
    'zh-CN': '字号增量',
    'zh-TW': '字號增量',
    en: 'Size delta',
    ko: '글자 크기 증감',
    ja: 'サイズ増分',
  },
  // 重置样式
  resetStyle: {
    'zh-CN': '重置样式',
    'zh-TW': '重設樣式',
    en: 'Reset style',
    ko: '스타일 초기화',
    ja: 'スタイルをリセット',
  },
  // 生成的名字代码
  generatedNameCode: {
    'zh-CN': '生成的名字代码',
    'zh-TW': '生成的名字代碼',
    en: 'Generated name code',
    ko: '생성된 이름 코드',
    ja: '生成された名前コード',
  },
  // 名字代码超过限制
  nameCodeIsOverTheLimit: {
    'zh-CN': '名字代码超过限制',
    'zh-TW': '名字代碼超過限制',
    en: 'Name code is over the limit',
    ko: '이름 코드가 제한을 초과했습니다',
    ja: '名前コードが上限を超えています',
  },
  // 超出 238 字节限制
  exceedsThe238ByteLimit: {
    'zh-CN': '超出 238 字节限制',
    'zh-TW': '超出 238 位元組限制',
    en: 'Exceeds the 238-byte limit',
    ko: '238바이트 제한 초과',
    ja: '238 バイト上限を超過',
  },
  // 正在推送到 街机MOD
  pushingToHostutilities: {
    'zh-CN': '正在推送到 街机MOD',
    'zh-TW': '正在推送到 主機實用工具',
    en: 'Pushing to HostUtilities',
    ko: '호스트 유틸리티로 보내는 중',
    ja: 'ホストユーティリティへ送信中',
  },
  // 全部等级
  allLevels: {
    'zh-CN': '全部等级',
    'zh-TW': '全部等級',
    en: 'All levels',
    ko: '모든 레벨',
    ja: 'すべてのレベル',
  },
  // {count} 行
  lines: {
    'zh-CN': '{count} 行',
    'zh-TW': '{count} 行',
    en: '{count} lines',
    ko: '{count}줄',
    ja: '{count} 行',
  },
  // 复制代码
  copyCode: {
    'zh-CN': '复制代码',
    'zh-TW': '複製代碼',
    en: 'Copy code',
    ko: '코드 복사',
    ja: 'コードをコピー',
  },
  // 已复制，可粘贴到街机MOD 的多彩名字菜单
  copiedPasteItIntoHostutilitiesColorfulNameMenu: {
    'zh-CN': '已复制，可粘贴到街机MOD 的多彩名字菜单',
    'zh-TW': '已複製，可貼到主機實用工具的多彩名字選單',
    en: 'Copied. Paste it into HostUtilities\\\' colorful-name menu.',
    ko: '복사되었습니다. 호스트 유틸리티의 다채로운 이름 메뉴에 붙여넣으세요',
    ja: 'コピーしました。ホストユーティリティのカラフルネームメニューに貼り付けられます',
  },
  // 推送到 街机MOD
  pushToHostutilities: {
    'zh-CN': '推送到 街机MOD',
    'zh-TW': '推送到 主機實用工具',
    en: 'Push To HostUtilities',
    ko: '호스트 유틸리티로 보내기',
    ja: 'ホストユーティリティへ送信',
  },
  // 已推送到 街机MOD
  pushedToHostutilities: {
    'zh-CN': '已推送到 街机MOD',
    'zh-TW': '已推送到 主機實用工具',
    en: 'Pushed to HostUtilities',
    ko: '호스트 유틸리티로 보냈습니다',
    ja: 'ホストユーティリティに送信しました',
  },
  // 复制后粘贴到游戏内街机MOD 的多彩名字菜单，或直接推送
  copyAndPasteIntoHostutilitiesColorfulNameMenuInGameOrPushDirectly: {
    'zh-CN': '复制后粘贴到游戏内街机MOD 的多彩名字菜单，或直接推送',
    'zh-TW': '複製後貼到遊戲內主機實用工具的多彩名字選單，或直接推送',
    en: 'Copy and paste into HostUtilities\\\' colorful-name menu in game, or push directly',
    ko: '복사해 게임 내 호스트 유틸리티의 다채로운 이름 메뉴에 붙여넣거나 바로 보내세요',
    ja: 'コピーしてゲーム内ホストユーティリティのカラフルネームメニューに貼り付けるか、直接送信します',
  },
  // {count} 个
  text: {
    'zh-CN': '{count} 个',
    'zh-TW': '{count} 個',
    en: '{count}',
    ko: '{count}개',
    ja: '{count} 件',
  },
}
