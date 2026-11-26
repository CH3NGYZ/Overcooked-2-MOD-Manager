const EDGE_MAX_AGE = 300
const ERROR_CODES = Object.freeze({
  400: 'invalid_request', 403: 'forbidden', 404: 'not_found', 405: 'method_not_allowed',
  409: 'conflict', 503: 'unavailable',
})

export function response(body, status, contentType, headers = {}) {
  return new Response(body, { status, headers: {
    'Cache-Control': 'no-store',
    'Content-Type': contentType,
    'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
    'Referrer-Policy': 'no-referrer',
    'X-Content-Type-Options': 'nosniff',
    ...headers,
  } })
}

export function json(value, { head = false, publicSnapshot = false, status = 200, headers } = {}) {
  return response(head ? null : JSON.stringify(value), status, 'application/json; charset=utf-8', {
    ...headers,
    ...(publicSnapshot && status === 200 ? { 'Cloudflare-CDN-Cache-Control': `public, s-maxage=${EDGE_MAX_AGE}` } : {}),
  })
}

export function error(status, head = false, headers) {
  return json({ error: ERROR_CODES[status] }, { status, head, headers })
}

export function assetResponse(asset, method) {
  if (!asset) return error(404, method === 'HEAD')
  if (method !== 'GET' && method !== 'HEAD') return error(405, false, { Allow: 'GET, HEAD' })
  return response(method === 'HEAD' ? null : asset.body, 200, asset.type)
}
