import page from './web/admin.html'
import stylesheet from './web/notice.css'
import common from './web/common.js'
import script from './web/admin.js'
import { noticeAssets, serveAdmin } from './handlers.mjs'

const assets = noticeAssets(page, stylesheet, common, '/notice-admin.js', script)

export default { fetch: (request, env, ctx) => serveAdmin(request, env, ctx, assets) }
