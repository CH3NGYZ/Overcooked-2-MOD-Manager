import type { DomainMessages } from '../types.ts'

/** 配置编辑、API 凭据、地区与赞助说明。同一 key 的五语保持集中。 */
export const toolsUiSettings: DomainMessages = {
  // 支持开发
  supportDevelopment: {
    'zh-CN': '支持开发',
    'zh-TW': '支持開發',
    en: 'Support development',
    ko: '개발 후원',
    ja: '開発を支援',
  },
  // 觉得好用，请作者喝杯咖啡
  buyTheAuthorACoffee: {
    'zh-CN': '觉得好用，请作者喝杯咖啡',
    'zh-TW': '覺得好用，請作者喝杯咖啡',
    en: 'Buy the author a coffee',
    ko: '유용했다면 커피 한 잔 부탁드립니다',
    ja: '役に立ったらコーヒー一杯お願いします',
  },
  // 所有功能永久免费，赞助用于服务器与开发动力。感谢支持！
  allFeaturesStayFreeSponsorshipsFundServersAndDevelopmentThankYou: {
    'zh-CN': '所有功能永久免费，赞助用于服务器与开发动力。感谢支持！',
    'zh-TW': '所有功能永久免費，贊助用於伺服器與開發動力。感謝支持！',
    en: 'All features stay free; sponsorships fund servers and development. Thank you!',
    ko: '모든 기능은 무료이며 후원은 서버와 개발에 사용됩니다. 감사합니다!',
    ja: 'すべての機能は無料です。支援はサーバーと開発に使われます。ありがとうございます！',
  },
  // 配置编辑
  editConfig: {
    'zh-CN': '配置编辑',
    'zh-TW': '設定編輯',
    en: 'Edit config',
    ko: '설정 편집',
    ja: '設定を編集',
  },
  // API 凭据
  apiCredentials: {
    'zh-CN': 'API 凭据',
    'zh-TW': 'API 憑證',
    en: 'API credentials',
    ko: 'API 자격 증명',
    ja: 'API 認証情報',
  },
  // 访问令牌
  accessTokens: {
    'zh-CN': '访问令牌',
    'zh-TW': '存取權杖',
    en: 'Access tokens',
    ko: '액세스 토큰',
    ja: 'アクセストークン',
  },
  // 配置后 API 请求直接使用 Token，避免匿名限流
  withATokenApiRequestsUseItDirectlyInsteadOfTheAnonymousQuota: {
    'zh-CN': '配置后 API 请求直接使用 Token，避免匿名限流',
    'zh-TW': '設定後 API 請求直接使用 Token，避免匿名限流',
    en: 'With a token, API requests use it directly instead of the anonymous quota',
    ko: '토큰을 설정하면 API 요청이 익명 대신 토큰을 사용합니다',
    ja: 'トークンを設定すると API リクエストは匿名ではなくトークンを使用します',
  },
  // 使用 Authorization: Bearer 请求头
  sentAsAnAuthorizationBearerHeader: {
    'zh-CN': '使用 Authorization: Bearer 请求头',
    'zh-TW': '使用 Authorization: Bearer 請求頭',
    en: 'Sent as an Authorization: Bearer header',
    ko: 'Authorization: Bearer 헤더로 전송됩니다',
    ja: 'Authorization: Bearer ヘッダーで送信します',
  },
  // 使用 access_token 请求头
  sentAsAnAccessTokenHeader: {
    'zh-CN': '使用 access_token 请求头',
    'zh-TW': '使用 access_token 請求頭',
    en: 'Sent as an access_token header',
    ko: 'access_token 헤더로 전송됩니다',
    ja: 'access_token ヘッダーで送信します',
  },
  // 已配置
  configured: {
    'zh-CN': '已配置',
    'zh-TW': '已設定',
    en: 'Configured',
    ko: '설정됨',
    ja: '設定済み',
  },
  // 已保存，留空则不修改
  savedLeaveBlankToKeep: {
    'zh-CN': '已保存，留空则不修改',
    'zh-TW': '已儲存，留空則不修改',
    en: 'Saved — leave blank to keep',
    ko: '저장됨 — 비워 두면 유지',
    ja: '保存済み — 空欄なら変更しません',
  },
  // 粘贴 Gitee 私人令牌
  pasteYourGiteePrivateToken: {
    'zh-CN': '粘贴 Gitee 私人令牌',
    'zh-TW': '貼上 Gitee 私人權杖',
    en: 'Paste your Gitee private token',
    ko: 'Gitee 개인 토큰을 붙여넣으세요',
    ja: 'Gitee のプライベートトークンを貼り付け',
  },
  // 清除全部
  clearAll: {
    'zh-CN': '清除全部',
    'zh-TW': '清除全部',
    en: 'Clear all',
    ko: '모두 지우기',
    ja: 'すべて削除',
  },
  // 已清除
  cleared: {
    'zh-CN': '已清除',
    'zh-TW': '已清除',
    en: 'Cleared',
    ko: '지워짐',
    ja: '削除しました',
  },
  // 配置 Token 后，API 请求直接使用 Token；Token 自身被限流时才回退到代理。
  withTokensSetRequestsUseThemDirectlyAndOnlyFallBackToProxiesWhenTheTokenItselfIsRateLimited: {
    'zh-CN': '配置 Token 后，API 请求直接使用 Token；Token 自身被限流时才回退到代理。',
    'zh-TW': '設定 Token 後，API 請求直接使用 Token；Token 自身被限流時才回退到代理。',
    en: 'With tokens set, requests use them directly and only fall back to proxies when the token itself is rate limited.',
    ko: '토큰을 설정하면 요청에 직접 사용하고, 토큰이 제한될 때만 프록시로 전환합니다.',
    ja: 'トークンを設定すると直接使用し、トークンが制限された場合のみプロキシに切り替えます。',
  },
  // 未保存
  unsaved: {
    'zh-CN': '未保存',
    'zh-TW': '未儲存',
    en: 'Unsaved',
    ko: '저장 안 됨',
    ja: '未保存',
  },
  // 有未保存的修改，已取消
  unsavedChangesWereDiscarded: {
    'zh-CN': '有未保存的修改，已取消',
    'zh-TW': '有未儲存的修改，已取消',
    en: 'Unsaved changes were discarded',
    ko: '저장하지 않은 변경 사항이 있습니다',
    ja: '未保存の変更があります',
  },
  // 中国大陆地区
  mainlandChina: {
    'zh-CN': '中国大陆地区',
    'zh-TW': '中國大陸地區',
    en: 'Mainland China',
    ko: '중국 본토',
    ja: '中国本土',
  },
  // 国际地区
  international: {
    'zh-CN': '国际地区',
    'zh-TW': '國際地區',
    en: 'International',
    ko: '해외',
    ja: '海外',
  },
}
