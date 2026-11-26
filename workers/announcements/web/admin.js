import { byId, LOCALE_LABELS, NOTICE_LIMITS, noticeDate, renderNotice, requestNoticeJson, requestSnapshot } from '/notice-common.js'

const apiPath = '/api/admin/announcements'
const translatePath = '/api/admin/translate'
const editor = byId('editor')
const language = byId('language')
const languageGrid = byId('language-grid')
const previewLanguage = byId('preview-language')
const removeLanguage = byId('remove-language')
const title = byId('title')
const body = byId('body')
const newNotice = byId('new')
const reload = byId('reload')
const submit = byId('submit')
const withdraw = byId('withdraw')
const translate = byId('translate')
const confirmation = byId('confirm')
const encoder = new TextEncoder()
const available = Object.keys(LOCALE_LABELS)
const emptyText = Object.freeze({ title: '', body: '' })
const versionLabels = { source: '原文', ready: '已翻译', partial: '待补全', missing: '缺译' }

const versions = new Map(available.map(code => {
  const label = document.createElement('label')
  const radio = document.createElement('input')
  const card = document.createElement('span')
  const name = document.createElement('strong')
  const status = document.createElement('small')
  radio.type = 'radio'
  radio.name = 'editing-language'
  radio.value = code
  name.textContent = LOCALE_LABELS[code]
  name.lang = code
  card.append(name, status)
  label.append(radio, card)
  return [code, { label, radio, status }]
}))
languageGrid.replaceChildren(...Array.from(versions.values(), version => version.label))
for (const select of [language, previewLanguage]) {
  select.replaceChildren(...available.map(code => new Option(LOCALE_LABELS[code], code)))
}

let snapshot = null
let draft = null
let activeLanguage = ''
let dirty = false
let busy = false
let conflicted = false
let translationService
let translationJob = null
let translationResult = null

function updateStatus(message, error = false) {
  const status = byId('status')
  status.hidden = !message
  status.textContent = message
  status.dataset.tone = error ? 'error' : 'normal'
}

function resetDraft(announcement) {
  translationJob?.controller.abort()
  translationJob = null
  const original = announcement?.language ?? 'zh-CN'
  draft = {
    id: announcement?.id,
    published_at: announcement?.published_at,
    language: original,
    texts: new Map([
      [original, { title: announcement?.title ?? '', body: announcement?.body ?? '' }],
      ...Object.entries(announcement?.translations ?? {}),
    ]),
  }
  dirty = false
  translationResult = null
  byId('preview').open = false
  byId('source-reference').open = false
  byId('editor-title').textContent = draft.id ? '编辑公告' : '新建公告'
  submit.textContent = draft.id ? '保存修改' : '发布公告'
  withdraw.hidden = !draft.id
  editor.hidden = false
  language.value = original
  activateLanguage(original)
  renderHistory()
}

function renderHistory() {
  const history = byId('history')
  const rows = snapshot.announcements.map(announcement => {
    const row = document.createElement('li')
    const button = document.createElement('button')
    const name = document.createElement('span')
    const date = document.createElement('small')
    name.textContent = announcement.title
    name.lang = announcement.language
    date.textContent = noticeDate(announcement.published_at, 'zh-CN')
    button.type = 'button'
    button.append(name, date)
    button.setAttribute('aria-pressed', String(announcement.id === draft?.id))
    button.addEventListener('click', async () => {
      if (busy || conflicted || announcement.id === draft?.id) return
      if (!await confirmDiscard()) return
      resetDraft(announcement)
      title.focus()
      updateStatus('')
    })
    row.append(button)
    return row
  })
  history.replaceChildren(...rows)
  newNotice.hidden = false
  reload.hidden = false
  byId('count').textContent = `${rows.length} / ${NOTICE_LIMITS.maxAnnouncements}`
  byId('history-empty').hidden = rows.length !== 0
  syncControls()
}

function syncControls() {
  newNotice.disabled = busy || conflicted
  reload.disabled = busy
  for (const button of byId('history').querySelectorAll('button')) button.disabled = busy || conflicted
  byId('editor-fields').disabled = busy
  submit.disabled = busy || conflicted || !!translationJob
  withdraw.disabled = busy || conflicted
  renderTranslationControls()
}

function draftText(code) {
  const text = draft.texts.get(code) ?? emptyText
  return { title: text.title.trim(), body: text.body.trim() }
}

function completeness({ title, body }) {
  if (!title && !body) return 'missing'
  return title && body ? 'ready' : 'partial'
}

function translationTargets() {
  return available.filter(code => code !== draft.language && completeness(draftText(code)) === 'missing')
}

function draftNotice() {
  return {
    id: draft.id,
    language: draft.language,
    ...draftText(draft.language),
    translations: Object.fromEntries(Array.from(draft.texts.keys())
      .filter(code => code !== draft.language).map(code => [code, draftText(code)])),
  }
}

function showDraftArticle(article, text, code, dateLocale) {
  const ready = completeness(text) === 'ready'
  article.hidden = !ready
  if (ready) renderNotice(article, { ...text, language: code, published_at: draft.published_at }, dateLocale)
  else article.replaceChildren()
}

function renderPreview() {
  const requested = previewLanguage.value
  const requestedText = draftText(requested)
  const fallback = requested !== draft.language && completeness(requestedText) === 'missing'
  const code = fallback ? draft.language : requested
  const text = fallback ? draftText(code) : requestedText
  const ready = completeness(text) === 'ready'
  const status = byId('preview-status')
  status.textContent = fallback ? `缺少译文；${ready ? `玩家将看到原文（${LOCALE_LABELS[code]}）。` : '请先填完原文。'}`
    : ready ? '' : '填写标题和正文后即可预览。'
  status.hidden = ready && !fallback
  showDraftArticle(byId('preview-content'), text, code, requested)
}

function renderVersions() {
  for (const [code, { label, radio, status }] of versions) {
    const version = code === draft.language ? 'source' : completeness(draftText(code))
    label.dataset.version = version
    radio.checked = code === activeLanguage
    status.textContent = versionLabels[version]
  }
  byId('editing-title').textContent = `${LOCALE_LABELS[activeLanguage]}（${activeLanguage === draft.language ? '原文' : '译文'}）`
  removeLanguage.hidden = activeLanguage === draft.language || !draft.texts.has(activeLanguage)
  const reference = byId('source-reference')
  reference.hidden = activeLanguage === draft.language
  byId('source-summary').textContent = `对照原文（${LOCALE_LABELS[draft.language]}）`
  const source = draftText(draft.language)
  byId('source-empty').hidden = completeness(source) === 'ready'
  showDraftArticle(byId('source-content'), source, draft.language, 'zh-CN')
}

function fieldError(value, limit, name) {
  if (!value.trim()) return `请填写${name}。`
  if (encoder.encode(value.trim()).byteLength > limit) return `${name}超出 ${limit} 字节。`
  return ''
}

function textError({ title, body }) {
  return fieldError(title, NOTICE_LIMITS.titleBytes, '标题') || fieldError(body, NOTICE_LIMITS.bodyBytes, '正文')
}

function renderTranslationControls() {
  const status = byId('translate-status')
  const missing = draft ? translationTargets().length : 0
  const validSource = draft && !textError(draftText(draft.language))
  translate.disabled = busy || conflicted || !translationService?.enabled || !!translationJob || !validSource || !missing
  translate.textContent = translationJob ? '正在生成译文…' : '生成缺少的译文'
  status.dataset.tone = translationResult?.error ? 'error' : 'normal'
  status.textContent = conflicted ? '重新读取最新公告后可继续生成译文。'
    : translationJob ? '可以继续编辑。生成结果不会覆盖已有译文，也不会自动发布。'
    : translationResult?.message ?? (translationService === undefined ? '正在检查翻译服务…'
      : translationService === null ? '未能检查翻译服务，请重新读取。'
      : !translationService.enabled ? '未配置翻译服务。请在后台 Worker 设置 API 地址和密钥。'
      : !validSource ? '填完原文的标题和正文后可生成译文。'
      : !missing ? '已无空白语言。未完成的译文可手动补全。'
      : `将原文发送给翻译服务（${translationService.model}），补齐 ${missing} 种空白语言。请审核后再保存。`)
}

async function loadTranslationService() {
  try {
    translationService = await requestNoticeJson(translatePath)
  } catch {
    translationService = null
  }
  renderTranslationControls()
}

function markTranslationEdit(code) {
  translationResult = null
  if (!translationJob) return
  if (code === translationJob.language) translationJob.sourceChanged = true
  else translationJob.targets.delete(code)
}

async function generateTranslations() {
  if (translate.disabled) return
  const request = { language: draft.language, ...draftText(draft.language), targets: translationTargets() }
  const job = { controller: new AbortController(), language: request.language, targets: new Set(request.targets), sourceChanged: false }
  translationJob = job
  translationResult = null
  syncControls()
  try {
    const { translations } = await requestNoticeJson(translatePath, request, { timeout: 90_000, signal: job.controller.signal })
    if (translationJob !== job) return
    if (job.sourceChanged) {
      translationResult = { message: '原文已修改，这次结果未填入。按最新原文重新生成即可。' }
      return
    }
    let count = 0
    for (const code of job.targets) {
      const text = translations?.[code]
      if (completeness(draftText(code)) !== 'missing' || typeof text?.title !== 'string'
        || typeof text?.body !== 'string' || textError(text)) continue
      draft.texts.set(code, text)
      count++
    }
    if (count) {
      const preview = previewLanguage.value
      dirty = true
      activateLanguage(activeLanguage)
      previewLanguage.value = preview
      renderPreview()
    }
    translationResult = { message: count ? `已填入 ${count} 种译文。请审核后再保存。` : '没有填入新译文，已保留手动编辑的内容。' }
  } catch (error) {
    if (translationJob !== job) return
    const messages = {
      400: '无法生成译文。请检查原文标题、正文和字节数。',
      403: '没有生成译文的权限，请重新登录。',
      503: '翻译服务不可用，请检查后台 Worker 配置。',
      504: '翻译服务响应超时，请重试。',
    }
    translationResult = { error: true, message: messages[error.status] ?? '未能生成译文，已保留现有文案。请稍后重试。' }
  } finally {
    if (translationJob === job) {
      translationJob = null
      syncControls()
    }
  }
}

function refreshFieldStatus() {
  const fields = [[title, '标题', 'title-count', NOTICE_LIMITS.titleBytes], [body, '正文', 'body-count', NOTICE_LIMITS.bodyBytes]]
  for (const [field, label, id, limit] of fields) {
    const bytes = encoder.encode(field.value.trim()).byteLength
    const count = byId(id)
    count.textContent = `${bytes} / ${limit} 字节`
    count.dataset.over = String(bytes > limit)
    field.setCustomValidity(fieldError(field.value, limit, label))
  }
}

function activateLanguage(code) {
  activeLanguage = code
  const text = draft.texts.get(code) ?? emptyText
  title.value = text.title
  body.value = text.body
  title.lang = body.lang = code
  refreshFieldStatus()
  previewLanguage.value = code
  renderVersions()
  renderPreview()
}

function confirmAction(heading, message, buttonLabel) {
  byId('confirm-title').textContent = heading
  byId('confirm-body').textContent = message
  byId('confirm-action').textContent = buttonLabel
  confirmation.returnValue = ''
  confirmation.showModal()
  return new Promise(resolve => confirmation.addEventListener('close', () => resolve(confirmation.returnValue === 'confirm'), { once: true }))
}

function confirmDiscard() {
  return !dirty || confirmAction('放弃未保存的修改？', '当前草稿不会发布。', '放弃修改')
}

async function load() {
  if (busy) return
  busy = true
  syncControls()
  updateStatus('正在读取公告…')
  try {
    snapshot = await requestSnapshot(apiPath)
    conflicted = false
    resetDraft()
    updateStatus('')
    void loadTranslationService()
  } catch {
    reload.hidden = false
    updateStatus('暂时未能读取公告。请检查网络或登录后重新读取。', true)
  } finally {
    busy = false
    syncControls()
  }
}

async function applyChange(action, fields) {
  const mutation = { revision: snapshot.revision, action, ...fields }
  busy = true
  syncControls()
  updateStatus('正在保存…')
  try {
    snapshot = await requestSnapshot(apiPath, mutation)
    const saved = action === 'publish' ? snapshot.announcements[0] : snapshot.announcements.find(item => item.id === fields.id)
    resetDraft(saved)
    const result = { publish: '已发布公告。', edit: '已保存修改。', withdraw: '已撤下公告。' }
    updateStatus(result[action])
  } catch (error) {
    if (error.status === 409 || error.status === 404) {
      conflicted = true
      updateStatus('公告已被其他会话更改。草稿仍在；可先复制文案，再重新读取最新公告。', true)
    } else if (error.status === 400) {
      updateStatus('公告内容不符合要求。请检查各语言的标题和正文。', true)
    } else {
      updateStatus('未能确认保存结果，草稿仍在。重试前可先重新读取确认。', true)
    }
  } finally {
    busy = false
    syncControls()
  }
}

language.addEventListener('change', () => {
  markTranslationEdit(draft.language)
  draft.language = language.value
  dirty = true
  activateLanguage(draft.language)
  renderTranslationControls()
})
languageGrid.addEventListener('change', event => activateLanguage(event.target.value))
removeLanguage.addEventListener('click', async () => {
  const code = activeLanguage
  if (!await confirmAction('移除此译文？', `保存后，玩家读取${LOCALE_LABELS[code]}公告时将改为看到原文。`, '移除译文')) return
  markTranslationEdit(code)
  draft.texts.delete(code)
  dirty = true
  activateLanguage(code)
  renderTranslationControls()
  versions.get(code).radio.focus()
})
function recordText() {
  markTranslationEdit(activeLanguage)
  const text = { title: title.value, body: body.value }
  if (!text.title.trim() && !text.body.trim()) draft.texts.delete(activeLanguage)
  else draft.texts.set(activeLanguage, text)
  dirty = true
  refreshFieldStatus()
  renderVersions()
  renderPreview()
  renderTranslationControls()
}
title.addEventListener('input', recordText)
body.addEventListener('input', recordText)
previewLanguage.addEventListener('change', renderPreview)
translate.addEventListener('click', generateTranslations)

newNotice.addEventListener('click', async () => {
  if (!await confirmDiscard()) return
  resetDraft()
  title.focus()
  updateStatus('')
})
reload.addEventListener('click', async () => {
  if (await confirmDiscard()) await load()
})
withdraw.addEventListener('click', async () => {
  if (busy || conflicted || !draft.id) return
  if (await confirmAction('撤下这条公告？', '撤下后，玩家将在公开缓存更新后看不到这条公告。未保存的修改也会丢弃。', '撤下公告')) {
    await applyChange('withdraw', { id: draft.id })
  }
})
editor.addEventListener('submit', async event => {
  event.preventDefault()
  if (busy || conflicted || translationJob) return
  for (const code of new Set([draft.language, ...draft.texts.keys()])) {
    const text = draftText(code)
    if (textError(text)) {
      activateLanguage(code)
      editor.reportValidity()
      return
    }
  }
  const overflow = !draft.id && snapshot.announcements.length >= NOTICE_LIMITS.maxAnnouncements
  const label = draft.id ? '保存修改' : '发布公告'
  const explain = overflow ? '发布后，最早的一条公告将移出列表。' : '玩家可通过公开页面查看公告，或在启动管理器时获取最新公告。'
  if (await confirmAction(`确认${label}？`, explain, label)) await applyChange(draft.id ? 'edit' : 'publish', draftNotice())
})

await load()
