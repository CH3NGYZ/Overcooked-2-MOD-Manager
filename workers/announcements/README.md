# 公告服务运维

| 部署名 | 域名 | 用途 |
|---|---|---|
| `oc2modmanager-notice` | `oc2modmanager-notice.ch3ng.top` | 公开公告页、按语言读取的公告 API |
| `oc2modmanager-notice-admin` | `oc2modmanager-notice-admin.ch3ng.top` | Access 保护的编辑页、发布和翻译 API |

两个 Worker 共用一个 D1 `oc2modmanager-notice`，绑定名均为 `DB`。源码和迁移随管理器仓库维护；编辑数据写 D1，不修改 Git，不需要部署 Pages，也不调用 GitHub 或 `OC2-GH-API`。公开读成功才缓存，后台不缓存。后台资源由 Worker 直接返回，不要改用会丢失 `ctx.access` 的 Static Assets 路由器或服务绑定转发。

## 日常发公告

1. 登录[公告后台](https://oc2modmanager-notice-admin.ch3ng.top/)，选原文语言，填标题和正文。
2. 用语言卡片手动补充译文，或在翻译服务配置后点「生成缺少的译文」。已有译文和只填了一部分的译文都不会被覆盖。生成结果只填草稿，逐种检查后再保存。
3. 展开「预览公告」切换语言；缺译时预览原文。确认「发布公告」或「保存修改」才写入 D1。编辑已有公告不改变 ID 和发布时间，不会重复提醒玩家。要撤下则选择对应公告并确认。

同一条记录保存 `id`、`published_at`、原文 `language/title/body` 和按语言码索引的 `translations`，不需要 D1 迁移。管理器使用当前界面语言请求 `GET /api/announcements?lang=zh-CN`，未指定语言时默认请求英语。每条结果只返回 `id`、`published_at`、实际展示的 `language/title/body`；没有对应译文就返回原文。普通读取按语言分别缓存 300 秒，管理器点「重新获取」用 `?lang=zh-CN&fresh=1` 直接读 D1，不读写边缘缓存。

## 配置自动翻译

在 **Cloudflare → Workers & Pages → oc2modmanager-notice-admin → Settings → Variables and Secrets** 添加以下绑定；不要加到公开 Worker，不要把密钥写入源码、URL 或聊天。

| 名称 | 类型 | 值 |
|---|---|---|
| `NOTICE_TRANSLATION_ENDPOINT` | 文本 | 供应商完整 HTTPS Chat Completions URL，通常以 `/v1/chat/completions` 结尾；不是只填 `/v1`，也不是 `/v1/responses` |
| `NOTICE_TRANSLATION_API_KEY` | Secret | 该端点的 Bearer API 密钥 |
| `NOTICE_TRANSLATION_MODEL` | 文本，可省略 | 默认 `gpt-6.2-sol`；端点必须实际支持该模型 ID，可改为供应商接受的名称 |

[OpenCode Go](https://opencode.ai/docs/go/#where-can-i-use-it) 的文本接口面向编程客户端，不用于本后台；检测到 Go 端点时停用自动生成，不向供应商发请求，也不自动切换计费。请选择允许公告翻译的通用 Chat Completions API。[OpenCode Console 按量接口](https://opencode.ai/docs/console/api/inference/)与 Go 套餐独立计费，地址为 `https://opencode.ai/inference/openai/v1/chat/completions`（无 `/go`），可用文档列出的 `glm-5.3`；使用前需确认工作区密钥、余额和用途符合供应商要求。

应用后台 Worker 设置后重新读取编辑器。端点／密钥未配时仍可手动编辑。点击「生成缺少的译文」时，后台一次发送原文标题、正文和所有空白目标语言，最多生成 11 种译文（另 1 种为原文）。提示词按本次目标语言生成 JSON 示例，要求用实际译文替换空值。例如只缺 `en`、`zh-TW` 时，模型应返回：

```json
{"translations":{"en":{"title":"Notice","body":"The update is available."},"zh-TW":{"title":"公告","body":"更新已推出。"}}}
```

后台从 `choices[0].message.content` 解析 JSON，校验语言码集合、标题和正文。多出、缺少语言或照抄空值都会拒绝，不改变草稿。不会随输入自动请求，也不会自动重试、写入公告或发布。调用可能计费，限时 80 秒。

失败时编辑器会区分无法连接、上游 HTTP 错误、译文格式错误和超时。Worker 使用 `redirect: 'manual'`，不跟随上游的 `3xx`，只报告状态；遇到跳转时应直接填写供应商文档中的最终端点。Cloudflare Workers 不支持 `redirect: 'error'`，使用该选项会在发出请求前失败。实时日志中可按 `notice_translation` 查失败类型及上游 HTTP 状态，不记录原文、密钥、上游响应正文或端点。

## 本地检查

从仓库根目录执行以下命令。`--local` 迁移只初始化本机 SQLite；它不会创建或更新线上 D1。两个 `wrangler dev` 需要指向同一个 `--persist-to` 目录，才能读到同一份本机数据。

```powershell
node --test tests/announcement-worker.test.mjs
npx wrangler@4.162.0 d1 migrations apply DB --config workers/announcements/wrangler.public.jsonc --local --persist-to .tmp/notice-d1
npx wrangler@4.162.0 deploy --config workers/announcements/wrangler.public.jsonc --dry-run --outdir .tmp/notice-public-bundle
npx wrangler@4.162.0 deploy --config workers/announcements/wrangler.admin.jsonc --dry-run --outdir .tmp/notice-admin-bundle
```

在两个终端分别启动 Worker；通过 `--connect-to` 仅把请求导向回环，同时保留应用校验的 HTTPS 域名。`--insecure` 仅用于本机自签证书，线上验收不要使用。

```powershell
npx wrangler@4.162.0 dev --config workers/announcements/wrangler.public.jsonc --local-protocol https --port 5921 --persist-to .tmp/notice-d1
npx wrangler@4.162.0 dev --config workers/announcements/wrangler.admin.jsonc --local-protocol https --port 5922 --persist-to .tmp/notice-d1
curl.exe -isS --insecure --noproxy '*' --connect-to oc2modmanager-notice.ch3ng.top:443:127.0.0.1:5921 https://oc2modmanager-notice.ch3ng.top/api/announcements
curl.exe -isS --insecure --noproxy '*' --connect-to oc2modmanager-notice-admin.ch3ng.top:443:127.0.0.1:5922 https://oc2modmanager-notice-admin.ch3ng.top/
```

首个请求应返回 `200` 和 `{"revision":0,"announcements":[]}`（空库），并有 `Cloudflare-CDN-Cache-Control: public, s-maxage=300`；第二个应返回 `403`。真实边缘缓存不在本机模拟。若需模拟后台登录，按 [Wrangler 本地 Access 模拟](https://developers.cloudflare.com/workers/configuration/cloudflare-access/#test-ctxaccess-locally) 在仅本机的配置中添加 `access.dev`，并用本机测试邮箱设置 `ADMIN_EMAIL`；不要提交或部署该配置。

## 已上线环境更新代码

已上线的两个 Worker 不需要再建 D1、跑迁移或关联域名。先按本地检查验收并核对 `whoami`、现网部署、域名和两端共同的 `DB`。从本次提交导出一份部署副本，仅在副本填现有 D1 ID。确认**最近上传的版本就是当前承接 100% 流量的版本**时，使用固定 Wrangler 版本：

```powershell
npx wrangler@4.162.0 versions upload --keep-vars --strict --config <部署副本配置>
npx wrangler@4.162.0 versions view <上传返回的版本ID> --config <部署副本配置>
npx wrangler@4.162.0 versions deploy <已核对的版本ID>@100% --config <部署副本配置>
```

如果最近上传的版本与生产版不同，不要用 `--keep-vars` 从最近上传的版本继承。用 [Workers Versions API](https://developers.cloudflare.com/api/resources/workers/subresources/beta/subresources/workers/subresources/versions/methods/create/) 只上传候选，给每个 `inherit` 绑定指定当前生产版的 `version_id`。配置来源不明确时不要切流。

切流前确认候选属于对应 Worker、D1 不变，后台现有 `ADMIN_EMAIL` 和翻译设置均保留；不要复制含密钥的配置或原始输出。切流后回读 `deployments status`。仅更新代码时不要运行 `triggers deploy`、普通 `deploy` 或远程迁移。

## 首次上线顺序（仅新环境）

上线前确认 Wrangler 账号、`ch3ng.top` 所属 Zone、两个 Worker 和 D1 的归属一致。以下是待执行清单，不是上线回执。

1. `npx wrangler@4.162.0 whoami` 核对账号。确认同名 D1 不存在后，用 `npx wrangler@4.162.0 d1 create oc2modmanager-notice` 创建。两份 Wrangler 配置在本机部署副本中填同一个真实 `database_id`，其余保持原值；模板的全零 ID 不能上线。配置副本可与原文件同目录以保留相对路径，但不得提交。
2. 在部署副本上执行 `npx wrangler@4.162.0 d1 migrations apply DB --config <公开部署配置> --remote`。`--remote` 会写线上 D1；只对已核对的目标执行一次，两站之后共享该表。已有数据时先备份并核对迁移记录，不重建或清空数据库。
3. 先用 `npx wrangler@4.162.0 deploy --config <后台部署配置>` 部署后台，首次不要设置 `ADMIN_EMAIL`，此时代码拒绝访问。进入 **Workers & Pages → oc2modmanager-notice-admin → Access**，为该 Worker 启用 **All traffic**，在 Zero Trust 细化为只允许指定维护邮箱。不要使用 **Everyone**、整个邮箱域或 **Bypass**；若已有该后台域名的单独 Access 应用，先检查它不会覆盖 Worker 级规则。不要把账号级保护附加到公开入口。
4. 在 **Zero Trust → Integrations → Identity providers** 启用 **Passkeys**，在该应用的 **Login methods** 中明确选择它。首次在登录页选 **Sign in with a passkey → Don't have a passkey? Set one up**；如果只允许 Passkeys，Access 会把首次验证 PIN 发到邮箱，验证后在系统安全提示中保存通行密钥。后续可直接用设备验证。邮箱策略仍须命中，不因为持有 Passkey 就放行。通过 `npx wrangler@4.162.0 secret put ADMIN_EMAIL --config <后台部署配置>` 交互输入相同维护邮箱，不写入命令行参数、仓库或日志。无配置、不匹配或 Access 身份缺失时，Worker 仍返回 `403`。
5. 确认后台匿名拒绝、授权维护者可打开编辑器后，再执行 `npx wrangler@4.162.0 deploy --config <公开部署配置>`。复核两站都只有自定义域名；`workers_dev: false`、`preview_urls: false` 必须保持关闭，只有公开站启用 Workers Cache。

## 上线验收

- 无痕打开后台和后台 API，应由 Access 要求登录，不得返回页面内容或 JSON；非维护邮箱不得通过。用维护邮箱注册、退出并再次用 Passkey 登录，核对后台读写。Access 原生退出路径为后台域名下的 `/cdn-cgi/access/logout`，会注销同一 Zero Trust 团队中的其他 Access 会话，旧令牌约 20–30 秒后失效。
- 公告 API 以管理器的 User-Agent、无 Cookie 和无认证头，对两个不同 `lang` 的 URL 分别连续 `GET`；核对对应译文或原文。记录 `Cf-Ray` 的机房、`Cf-Cache-Status` 和 `Age`，同一机房观察 `MISS → HIT`。手动重取 URL `?lang=zh-CN&fresh=1`、后台、错误和页面资源都不应 `HIT`。线上 `Cloudflare-CDN-Cache-Control` 会被边缘消费，不一定出现在回包；浏览器收到的仍是 `Cache-Control: no-store`。公开域名不能返回后台脚本或后台 API。
- 发布、编辑和撤下会改变 `revision`。推荐在本机 D1 验证从写入到公开读的全链；生产只按确认过的内容发布，不为测试给用户推假公告。普通读取在缓存过期前可能看到旧版本，管理器可点「重新获取」立即重读。
- 已核对的客户端仍应能在公告域或网络不可用时正常启动；发布前从大陆和境外网络分别实测域名与 HTTPS。自定义域名、缓存和少量请求只能降低压力，不能保证可达或保证不受限。

## 配额与限流

- 不从 GitHub 拉公告、不占 GitHub API 额度。边缘命中不执行 Worker、不读 D1，但请求本身仍计入 Workers 用量。按当前 [Workers 限额](https://developers.cloudflare.com/workers/platform/limits/#daily-requests)，Free 为账号每日 100,000 次（UTC 零时重置）；[D1 Free](https://developers.cloudflare.com/d1/platform/pricing/#billing-metrics) 每日 500 万行读、10 万行写。投产前按账户套餐重新核对，并监控请求、命中率、D1 行数和错误率。
- 流量异常先核对 Metrics、机房和请求来源，再按 [Cloudflare Rate Limiting](https://developers.cloudflare.com/waf/rate-limiting-rules/) 对公开公告 API 配置限流（能力依套餐）。阈值以正常峰值为基准，避免按低阈值误伤共享出口；桌面请求被限流时不应要求通过浏览器挑战或密集重试。不要轮换身份或域名以绕过服务限制。
