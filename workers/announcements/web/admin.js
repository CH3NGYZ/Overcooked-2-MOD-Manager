import { byId, LOCALE_LABELS, NOTICE_LIMITS, noticeDate, renderNotice, requestSnapshot } from '/notice-common.js'

const apiPath = '/api/admin/announcements'
const editor = byId('editor')
const language = byId('language')
const editingLanguage = byId('editing-language')
const addLanguage = byId('add-language')
const previewLanguage = byId('preview-language')
const removeLanguage = byId('remove-language')
const title = byId('title')
const body = byId('body')
const newNotice = byId('new')
const reload = byId('reload')
const submit = byId('submit')
const withdraw = byId('withdraw')
const confirmation = byId('confirm')
const encoder = new TextEncoder()
const available = Object.keys(LOCALE_LABELS)

let snapshot = null
let draft = null
let activeLanguage = ''
let dirty = false
let busy = false
let conflicted = false

function updateStatus(message, error = false) {
  const status = byId('status')
  status.hidden = !message
  status.textContent = message
  status.dataset.tone = error ? 'error' : 'normal'
}

function setOptions(select, codes, current, add = false) {
  const options = codes.map(code => new Option(LOCALE_LABELS[code], code, false, code === current))
  if (add) options.unshift(new Option('选择语言', '', false, true))
  select.replaceChildren(...options)
}

function resetDraft(announcement) {
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
  activeLanguage = original
  dirty = false
  byId('preview').open = false
  refreshEditor(original)
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
  submit.disabled = busy || conflicted
  withdraw.disabled = busy || conflicted
}

function draftNotice() {
  const texts = Array.from(draft.texts, ([code, text]) => [code, { title: text.title.trim(), body: text.body.trim() }])
  const original = texts.find(([code]) => code === draft.language)[1]
  return {
    id: draft.id, published_at: draft.published_at, language: draft.language, ...original,
    translations: Object.fromEntries(texts.filter(([code]) => code !== draft.language)),
  }
}

function renderPreview() {
  const article = byId('preview-content')
  const notice = draftNotice()
  const text = notice.translations[previewLanguage.value] ?? notice
  const ready = !!(text.title && text.body)
  article.hidden = !ready
  byId('preview-empty').hidden = ready
  if (ready) renderNotice(article, notice, previewLanguage.value)
  else article.replaceChildren()
}

function fieldError(value, limit, name) {
  if (!value.trim()) return `请填写${name}。`
  if (encoder.encode(value.trim()).byteLength > limit) return `${name}超出 ${limit} 字节。`
  return ''
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

function showActiveText() {
  const text = draft.texts.get(activeLanguage)
  title.value = text.title
  body.value = text.body
  editingLanguage.value = activeLanguage
  removeLanguage.hidden = activeLanguage === draft.language
  refreshFieldStatus()
}

function refreshEditor(preview = previewLanguage.value) {
  const populated = available.filter(code => draft.texts.has(code))
  setOptions(language, available, draft.language)
  setOptions(editingLanguage, populated, activeLanguage)
  setOptions(addLanguage, available.filter(code => !draft.texts.has(code)), '', true)
  addLanguage.disabled = populated.length === available.length
  setOptions(previewLanguage, populated, populated.includes(preview) ? preview : draft.language)
  byId('editor-title').textContent = draft.id ? '编辑公告' : '新建公告'
  submit.textContent = draft.id ? '保存修改' : '发布公告'
  withdraw.hidden = !draft.id
  editor.hidden = false
  showActiveText()
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
    resetDraft()
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
  const newOriginal = language.value
  if (!draft.texts.has(newOriginal)) {
    draft.texts.set(newOriginal, draft.texts.get(draft.language))
    draft.texts.delete(draft.language)
  }
  draft.language = newOriginal
  activeLanguage = newOriginal
  dirty = true
  refreshEditor(newOriginal)
})
editingLanguage.addEventListener('change', () => {
  activeLanguage = editingLanguage.value
  showActiveText()
})
addLanguage.addEventListener('change', () => {
  if (!addLanguage.value) return
  activeLanguage = addLanguage.value
  draft.texts.set(activeLanguage, { title: '', body: '' })
  dirty = true
  refreshEditor(activeLanguage)
  title.focus()
})
removeLanguage.addEventListener('click', async () => {
  if (!await confirmAction('移除此译文？', `编辑草稿中的${LOCALE_LABELS[activeLanguage]}译文将被移除。`, '移除译文')) return
  draft.texts.delete(activeLanguage)
  activeLanguage = draft.language
  dirty = true
  refreshEditor(activeLanguage)
  editingLanguage.focus()
})
function recordText() {
  draft.texts.set(activeLanguage, { title: title.value, body: body.value })
  dirty = true
  refreshFieldStatus()
  renderPreview()
}
title.addEventListener('input', recordText)
body.addEventListener('input', recordText)
previewLanguage.addEventListener('change', renderPreview)

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
  if (busy || conflicted) return
  for (const [code, text] of draft.texts) {
    if (fieldError(text.title, NOTICE_LIMITS.titleBytes, '标题') || fieldError(text.body, NOTICE_LIMITS.bodyBytes, '正文')) {
      activeLanguage = code
      showActiveText()
      editor.reportValidity()
      return
    }
  }
  const { published_at: _published, ...fields } = draftNotice()
  const overflow = !draft.id && snapshot.announcements.length >= NOTICE_LIMITS.maxAnnouncements
  const label = draft.id ? '保存修改' : '发布公告'
  const explain = overflow ? '发布后，最早的一条公告将移出列表。' : '玩家可通过公开页面查看公告，或在启动管理器时获取最新公告。'
  if (await confirmAction(`确认${label}？`, explain, label)) await applyChange(draft.id ? 'edit' : 'publish', fields)
})

await load()
