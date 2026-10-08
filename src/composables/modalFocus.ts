// Keep keyboard navigation within the active dialog. The caller focuses the
// initial control and restores its trigger when closing.
export function trapModalFocus(event: KeyboardEvent) {
  const dialog = event.currentTarget as HTMLElement
  const controls = [...dialog.querySelectorAll<HTMLElement>(
    'button, input, select, textarea, a[href], [tabindex]',
  )].filter((element) => !element.matches(':disabled, [tabindex="-1"]') && element.getClientRects().length)
  const first = controls[0]
  const last = controls.at(-1)
  if (!first || !last) {
    event.preventDefault()
    return
  }
  if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && (document.activeElement === last || document.activeElement === dialog)) {
    event.preventDefault()
    first.focus()
  }
}
