import type { DomainMessages } from './types'

export const tutorialDiyChef: DomainMessages = {
  'diychef.default.title': {
    'zh-CN': '默认开屏厨师', 'zh-TW': '預設開場廚師', en: 'Your opening chef', ko: '시작 화면 셰프', ja: '起動画面のシェフ',
  },
  'diychef.default.body': {
    'zh-CN': '已启用区域最前面的卡片就是默认开屏厨师，拖到最前面即可更换。DIYChef 2.2 及以上支持自定义厨师作为默认；旧版需要游戏自带厨师。所有版本都必须至少启用一位游戏自带厨师作为稳定回退。',
    'zh-TW': '已啟用區域最前面的卡片就是預設開場廚師，拖到最前面即可更換。DIYChef 2.2 以上支援自訂廚師作為預設；舊版需要遊戲內建廚師。所有版本都必須至少啟用一位遊戲內建廚師作為穩定備援。',
    en: 'The first card under Enabled is your opening chef. Drag a chef to the front to change it. DIYChef 2.2 or later supports custom opening chefs; older versions require a built-in chef. Every version needs at least one enabled game chef as a stable fallback.',
    ko: '활성 영역의 첫 카드가 시작 화면 셰프입니다. 원하는 셰프를 맨 앞으로 끌어 바꿀 수 있습니다. DIYChef 2.2 이상은 사용자 지정 셰프를 지원하며 이전 버전은 게임 기본 셰프가 필요합니다. 모든 버전에서 안정적인 대체를 위해 게임 기본 셰프를 한 명 이상 활성화해야 합니다.',
    ja: '有効エリアの先頭カードが起動画面のシェフです。先頭にドラッグすると変更できます。DIYChef 2.2以降はカスタムシェフにも対応し、旧版ではゲーム標準のシェフが必要です。全バージョンで安定した代替用にゲーム内蔵シェフを1人以上有効にしてください。',
  },
  'diychef.drag.title': {
    'zh-CN': '拖动排序与启停', 'zh-TW': '拖曳排序與啟停', en: 'Drag to arrange and enable', ko: '드래그로 정렬과 활성 전환', ja: 'ドラッグで並べ替えと有効化',
  },
  'diychef.drag.body': {
    'zh-CN': '右上复选框用于多选，也可从空白处框选，按 Ctrl/Cmd 可加选。拖动选中卡片可成组排序；拖入已启用区会启用，明确拖入未启用区才会停用。标题旁可全部启用或全部禁用；没有其他合法默认时保留当前开屏厨师，帽子搭配始终保留。',
    'zh-TW': '右上核取方塊用於多選，也可從空白處框選，按 Ctrl/Cmd 可加選。拖曳已選卡片可整組排序；拖入已啟用區會啟用，明確拖入未啟用區才會停用。標題旁可全部啟用或全部停用；沒有其他合法預設時保留目前開屏廚師，帽子搭配始終保留。',
    en: 'Use the top-right checkboxes or drag a rectangle from empty space to select chefs; hold Ctrl/Cmd to add. Drag selected cards as a group to arrange or enable them. Drop inside Not enabled to disable them. Section buttons apply to every chef. If no replacement opening chef qualifies, the current one stays enabled. Hats are always kept.',
    ko: '오른쪽 위 체크박스나 빈 공간에서 드래그한 사각형으로 선택하고 Ctrl/Cmd로 추가하세요. 선택 카드를 함께 끌어 정렬하거나 활성화합니다. 미사용 영역 안에 놓아야 해제됩니다. 제목 버튼은 모든 셰프에 적용됩니다. 대신할 시작 셰프가 없으면 현재 셰프를 유지하며 모자 설정은 항상 보존됩니다.',
    ja: '右上のチェックボックス、または空白からの範囲選択で複数選び、Ctrl/Cmdで追加できます。選択カードをまとめて並べ替え、有効化できます。無効エリア内に置いた場合だけ無効になります。見出しのボタンは全シェフが対象です。代わりの起動時シェフがいなければ現在のシェフを残し、帽子設定は常に保持します。',
  },
  'diychef.hats.title': {
    'zh-CN': '点击卡片换帽子', 'zh-TW': '點擊卡片換帽子', en: 'Click a card to change hats', ko: '카드를 눌러 모자 변경', ja: 'カードを押して帽子を変更',
  },
  'diychef.hats.body': {
    'zh-CN': '点击厨师卡片打开换帽窗口。右侧选择帽子，左侧查看效果，确认后应用；也可以跟随游戏，或选择不戴帽子。',
    'zh-TW': '點擊廚師卡片開啟換帽視窗。右側選擇帽子，左側查看效果，確認後套用；也可以跟隨遊戲，或選擇不戴帽子。',
    en: 'Click a chef card to open the hat window. Choose a hat on the right, preview it on the left, then apply. You can also follow the game or choose no hat.',
    ko: '셰프 카드를 누르면 모자 창이 열립니다. 오른쪽에서 모자를 고르고 왼쪽에서 확인한 뒤 적용하세요. 게임 설정을 따르거나 모자를 쓰지 않을 수도 있습니다.',
    ja: 'シェフのカードを押すと帽子のウィンドウが開きます。右で帽子を選び、左で確認して適用します。ゲームの設定に合わせるか、帽子なしにもできます。',
  },
  'diychef.more.title': {
    'zh-CN': '更多操作', 'zh-TW': '更多操作', en: 'More actions', ko: '추가 작업', ja: 'その他の操作',
  },
  'diychef.more.body': {
    'zh-CN': '右上角「更多操作」里可以开关街机大厅上下键换厨师，默认开启。恢复默认会只启用黑猫，并开启大厅换厨师。',
    'zh-TW': '右上角「更多操作」裡可開關街機大廳上下鍵換廚師，預設開啟。恢復預設會只啟用黑貓，並開啟大廳換廚師。',
    en: 'More actions in the top-right lets you toggle chef switching with Up/Down in the arcade lobby, which is on by default. Restoring defaults enables only the black cat and turns lobby switching on.',
    ko: '오른쪽 위 추가 작업에서 아케이드 대기실 위·아래 키 셰프 전환을 켜거나 끌 수 있으며 기본값은 켜짐입니다. 기본값으로 복원하면 검은 고양이만 활성화되고 대기실 전환도 켜집니다.',
    ja: '右上のその他の操作では、アーケードロビーで上下キーによるシェフ切り替えをオン・オフできます。初期設定はオンです。初期設定に戻すと黒猫だけが有効になり、ロビーの切り替えもオンになります。',
  },
  'diychef.demo.blackCat': { 'zh-CN': '黑猫', 'zh-TW': '黑貓', en: 'Black cat', ko: '검은 고양이', ja: '黒猫' },
  'diychef.demo.fox': { 'zh-CN': '狐狸', 'zh-TW': '狐狸', en: 'Fox', ko: '여우', ja: 'キツネ' },
  'diychef.demo.racoon': { 'zh-CN': '浣熊', 'zh-TW': '浣熊', en: 'Raccoon', ko: '너구리', ja: 'アライグマ' },
}
