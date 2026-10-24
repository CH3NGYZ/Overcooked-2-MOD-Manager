import { api } from '../services/tauri'

function formatFrontendLogValue(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}${value.stack ? `\n${value.stack}` : ''}`
  if (typeof value === 'string') return value
  try { return JSON.stringify(value) ?? String(value) } catch { return String(value) }
}

function redactFrontendLog(value: string): string {
  return value.replace(
    /(authorization|private[-_]?token|access[-_]?token|api[-_]?token|github[-_]?token|gitee[-_]?token)(\s*[:=]\s*(?:bearer\s+)?)[^\s,&}"']+/gi,
    '$1$2[REDACTED_SECRET]'
  )
}

/**
 * Mirror console output and global errors into the shared frontend log file,
 * with credentials redacted before anything leaves the page.
 */
export function installFrontendLogging(): () => void {
  const methods = ['debug', 'info', 'log', 'warn', 'error'] as const
  const originals = new Map<string, (...args: unknown[]) => void>()
  const consoleTarget = console as unknown as Record<string, (...args: unknown[]) => void>
  for (const method of methods) {
    const original = consoleTarget[method].bind(console)
    originals.set(method, original)
    consoleTarget[method] = (...args: unknown[]) => {
      original(...args)
      const message = redactFrontendLog(args.map(formatFrontendLogValue).join(' '))
      void api.writeFrontendLog(method, message).catch(() => undefined)
    }
  }
  const onError = (event: ErrorEvent) => {
    const message = redactFrontendLog(`${event.message} at ${event.filename}:${event.lineno}:${event.colno}`)
    void api.writeFrontendLog('error', message).catch(() => undefined)
  }
  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    const message = redactFrontendLog(`Unhandled promise rejection: ${formatFrontendLogValue(event.reason)}`)
    void api.writeFrontendLog('error', message).catch(() => undefined)
  }
  window.addEventListener('error', onError)
  window.addEventListener('unhandledrejection', onUnhandledRejection)
  return () => {
    for (const method of methods) {
      const original = originals.get(method)
      if (original) consoleTarget[method] = original
    }
    window.removeEventListener('error', onError)
    window.removeEventListener('unhandledrejection', onUnhandledRejection)
  }
}
