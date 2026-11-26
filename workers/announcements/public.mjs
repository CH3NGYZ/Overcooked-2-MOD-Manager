import page from './web/public.html'
import stylesheet from './web/notice.css'
import common from './web/common.js'
import script from './web/public.js'
import { noticeAssets, servePublic } from './handlers.mjs'

const assets = noticeAssets(page, stylesheet, common, '/notice-public.js', script)

export default { fetch: (request, env) => servePublic(request, env, assets) }
