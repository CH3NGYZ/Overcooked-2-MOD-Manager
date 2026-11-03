import type { DomainMessages } from './types'

/**
 * 内置教程：工具页与收款二维码（`tutorial.tools.*` / `tutorial.sponsor.*`）。
 *
 * 工具页是「维护动作」的集合：日志、回收站、缓存、高级。这一组不需要操作，
 * 正文只讲清两件事 —— 这里能干什么、什么时候会用到它。教程**不会**替你点
 * 「清空日志」「清除缓存」这类按钮，也不该点（见 `.tmp/tutorial-contract.md` 第 7 节）。
 *
 * 收款码单独用一个 `sponsor` 前缀：它是整份教程的最后一步，与工具页其余
 * 四步不是同一类内容（那四步讲功能，这一步只是道谢），文案也要更轻。
 *
 * 语言表里写的是**域内 key**：`tools.logs.title` 经 `locale/index.ts` 的
 * `{ domain: 'tutorial', messages: tutorialTools }` 拼成
 * `tutorial.tools.logs.title`。段名与步骤 id 对齐（`tools-logs` → `tools.logs.*`）。
 */
export const tutorialTools: DomainMessages = {
  'tools.logs.title': {
    'zh-CN': '日志与诊断',
    'zh-TW': '日誌與診斷',
    en: 'Logs and diagnostics',
    ko: '로그와 진단',
    ja: 'ログと診断',
  },
  'tools.logs.body': {
    'zh-CN': '出了问题先看这里。后端、界面、崩溃三份日志分开记，打开后能按等级筛选；反馈问题时把报错行复制出来就行。',
    'zh-TW': '出了問題先看這裡。後端、介面、崩潰三份日誌分開記，開啟後能依等級篩選；回報問題時把錯誤行複製出來就行。',
    en: 'When something breaks, start here. Backend, UI and crash logs are kept separately with a level filter; copy the failing lines into your report.',
    ko: '문제가 생기면 먼저 여기를 보세요. 백엔드, 화면, 크래시 로그가 따로 기록되고 단계별로 걸러 볼 수 있으니, 오류 줄을 복사해 알려 주세요.',
    ja: '不具合が出たらまずここ。バックエンド・画面・クラッシュのログが別々に残り、レベルで絞り込めます。報告にはエラー行をコピーしてください。',
  },
  'tools.recycle.title': {
    'zh-CN': '回收站',
    'zh-TW': '資源回收筒',
    en: 'Recycle bin',
    ko: '휴지통',
    ja: 'ゴミ箱',
  },
  'tools.recycle.body': {
    'zh-CN': '删掉的东西默认先进这里，可以在设置里关掉这层保护。误删了就回来点恢复；确认不要了再彻底删除，那一步无法撤销。',
    'zh-TW': '刪掉的東西預設先進這裡，可以在設定裡關掉這層保護。誤刪了就回來點復原；確定不要了再徹底刪除，那一步無法復原。',
    en: 'Deleted items land here first, unless you turn that off in Settings. Restore a mistake here; permanent deletion cannot be undone.',
    ko: '삭제한 항목은 먼저 여기로 옵니다(설정에서 이 보호 기능을 끌 수 있습니다). 실수로 지운 것은 여기서 복원하고, 완전 삭제는 되돌릴 수 없습니다.',
    ja: '削除したものはまずここに入ります（設定でこの保護を切れます）。誤って消したらここで復元、完全削除は元に戻せません。',
  },
  'tools.cache.title': {
    'zh-CN': '缓存清理',
    'zh-TW': '快取清理',
    en: 'Cache cleanup',
    ko: '캐시 정리',
    ja: 'キャッシュ整理',
  },
  'tools.cache.body': {
    'zh-CN': '头像和地图信息都会缓存，平时不用管。头像不更新或地图信息读错时，清一次缓存，再打开对应页面。',
    'zh-TW': '頭像和地圖資訊都會快取，平時不用管。頭像不更新或地圖資訊讀錯時，清一次快取，再開啟對應頁面。',
    en: 'Avatars and map info are cached, so usually nothing to do here. Stale avatar or wrong map info? Clear it, then reopen that page.',
    ko: '아바타와 지도 정보는 캐시됩니다. 보통은 건드릴 필요 없고, 아바타가 갱신되지 않거나 지도 정보가 잘못 읽히면 캐시를 지운 뒤 해당 페이지를 다시 여세요.',
    ja: 'アバターとマップ情報はキャッシュされます。普段は何もしなくて大丈夫。アバターが更新されない、マップ情報が正しく読めないときは、キャッシュを消してからそのページを開き直してください。',
  },
  'tools.advanced.title': {
    'zh-CN': '高级工具',
    'zh-TW': '進階工具',
    en: 'Advanced tools',
    ko: '고급 도구',
    ja: '詳細設定',
  },
  'tools.advanced.body': {
    'zh-CN': '开发者工具打开调试面板，报问题时能截图控制台；窗口跑出屏幕或布局乱了，用重置窗口状态拉回来。想再看教程，点右边按钮。',
    'zh-TW': '開發者工具開啟偵錯面板，回報問題時能截圖主控台；視窗跑出螢幕或版面亂了，用重設視窗狀態拉回來。想再看教學，點右邊按鈕。',
    en: 'Open the debug panel here for reports; reset window state when the window goes off-screen. Replay the tour with the button on the right.',
    ko: '문제 보고용 디버그 패널을 열거나, 창이 화면 밖으로 나갔을 때 창 상태를 초기화합니다. 오른쪽 버튼으로 투어를 다시 볼 수 있습니다.',
    ja: '問題報告用のデバッグパネルを開いたり、ウィンドウが画面外に出たら状態をリセットできます。右のボタンでツアーを最初から見られます。',
  },
  'sponsor.title': {
    'zh-CN': '支持开发者',
    'zh-TW': '支持開發者',
    en: 'Support the developer',
    ko: '개발자 후원',
    ja: '開発者を支援',
  },
  'sponsor.body': {
    'zh-CN': '管理器一直免费，服务器、签名和更新都靠这点赞助撑着。扫不扫都随你 —— 用着顺手就是最好的支持。',
    'zh-TW': '管理器一直免費，伺服器、簽章和更新都靠這點贊助撐著。掃不掃都隨你 —— 用著順手就是最好的支持。',
    en: 'The manager stays free; servers, code signing and updates are funded by these tips. Scan it or not — using it is already support.',
    ko: '관리자는 계속 무료입니다. 서버, 서명, 업데이트는 이런 후원으로 유지됩니다. 사용해 주시는 것만으로도 큰 힘이 됩니다.',
    ja: 'マネージャーはこれからも無料です。サーバー・署名・更新はこの支援で成り立っています。使ってくださるだけでも支えになります。',
  },
}
