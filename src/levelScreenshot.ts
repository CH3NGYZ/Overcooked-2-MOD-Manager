/** Adapt only the disposable screenshot DOM; the interactive preview stays intact. */
export function prepareLevelScreenshot(snapshot: HTMLElement, title: string) {
  snapshot.style.padding = '20px'
  snapshot.style.boxSizing = 'border-box'
  snapshot.style.fontSize = '16px'
  const set = (selector: string, style: Partial<CSSStyleDeclaration>) => {
    snapshot.querySelectorAll<HTMLElement>(selector).forEach(node => Object.assign(node.style, style))
  }
  const heading = snapshot.querySelector<HTMLElement>('.level-name')
  if (heading) heading.textContent = title
  set('.level-name', { fontSize: '48px', whiteSpace: 'normal', overflow: 'visible' })
  set('.level-heading', { marginBottom: '16px' })
  set('.information-layout', { display: 'grid', gridTemplateColumns: 'minmax(0, 1fr)', gap: '12px', fontSize: '32px' })
  const table = snapshot.querySelector<HTMLTableElement>('.level-information-table')
  const thumbnail = snapshot.querySelector<HTMLTableCellElement>('.thumbnail-cell')
  if (table && thumbnail) {
    // Large labels use one label/value pair per row instead of squeezing four columns.
    for (const row of [...table.rows]) {
      const cells = [...row.cells].filter(cell => cell !== thumbnail)
      if (cells.length !== 4) continue
      const next = table.insertRow(row.rowIndex + 1)
      next.append(cells[2], cells[3])
    }
    const firstRow = table.insertRow(0)
    firstRow.append(thumbnail)
    thumbnail.rowSpan = 1
    thumbnail.colSpan = 2
    table.querySelector('colgroup')?.remove()
  }
  set('.level-information-table', { width: '100%', flex: 'none' })
  set('.thumbnail-cell', { padding: '12px', border: '0' })
  const image = snapshot.querySelector<HTMLImageElement>('.thumbnail-trigger img')
  const ratio = image?.naturalWidth && image.naturalHeight ? image.naturalWidth / image.naturalHeight : 524 / 308
  set('.thumbnail-trigger', { width: `${Math.min(688, 300 * ratio)}px`, height: `${Math.min(300, 688 / ratio)}px`, maxWidth: '100%' })
  set('th,td', { padding: '16px 18px', lineHeight: '1.5', verticalAlign: 'middle' })
  set('th', { width: '40%', whiteSpace: 'normal', overflowWrap: 'normal' })
  snapshot.querySelector('.bundle-column')?.remove()
  set('.recipe-toolbar', { fontSize: '14px', marginTop: '16px' })
  snapshot.querySelector('.recipe-toolbar button')?.remove()
  set('.recipe-grid', { gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: '12px' })
  set('.recipe-number,.recipe-score', { fontSize: '14px' })
}
