/** Adapt only the disposable screenshot DOM; the interactive preview stays intact. */
export function prepareLevelScreenshot(snapshot: HTMLElement) {
  snapshot.style.padding = '20px'
  snapshot.style.boxSizing = 'border-box'
  snapshot.style.fontSize = '16px'
  const set = (selector: string, style: Partial<CSSStyleDeclaration>) => {
    snapshot.querySelectorAll<HTMLElement>(selector).forEach(node => Object.assign(node.style, style))
  }
  set('.level-name', { fontSize: '24px', whiteSpace: 'normal', overflow: 'visible' })
  set('.level-heading', { marginBottom: '16px' })
  set('.information-layout', { display: 'grid', gridTemplateColumns: 'minmax(0, 1fr)', gap: '12px', fontSize: '16px' })
  const table = snapshot.querySelector<HTMLTableElement>('.level-information-table')
  const thumbnail = snapshot.querySelector<HTMLTableCellElement>('.thumbnail-cell')
  if (table && thumbnail) {
    const firstRow = table.insertRow(0)
    firstRow.append(thumbnail)
    thumbnail.rowSpan = 1
    thumbnail.colSpan = 4
    table.querySelector('colgroup')?.remove()
  }
  set('.level-information-table', { width: '100%', flex: 'none' })
  set('.thumbnail-cell', { padding: '12px', border: '0' })
  const image = snapshot.querySelector<HTMLImageElement>('.thumbnail-trigger img')
  const ratio = image?.naturalWidth && image.naturalHeight ? image.naturalWidth / image.naturalHeight : 524 / 308
  set('.thumbnail-trigger', { width: `${Math.min(688, 300 * ratio)}px`, height: `${Math.min(300, 688 / ratio)}px`, maxWidth: '100%' })
  set('th,td', { padding: '9px 12px' })
  set('.bundle-column', { flex: 'none', width: '100%', minWidth: '0', boxSizing: 'border-box' })
  const bundle = snapshot.querySelector('.bundle-column')
  if (bundle) {
    const block = document.createElement('div')
    block.className = bundle.className
    for (const attribute of bundle.attributes) block.setAttribute(attribute.name, attribute.value)
    block.append(...bundle.childNodes)
    bundle.replaceWith(block)
  }
  set('.bundle-list', { maxHeight: 'none', overflow: 'visible', lineHeight: '24px' })
  snapshot.querySelectorAll('.bundle-toggle').forEach(node => node.remove())
  set('.recipe-toolbar', { fontSize: '14px', marginTop: '16px' })
  snapshot.querySelector('.recipe-toolbar button')?.remove()
  set('.recipe-grid', { gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: '12px' })
  set('.recipe-number,.recipe-score', { fontSize: '14px' })
}
