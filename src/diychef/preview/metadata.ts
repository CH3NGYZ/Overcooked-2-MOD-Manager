import { enqueuePreviewMetadata } from './queue'

interface Read {
  promise: Promise<unknown>
  users: Set<symbol>
  abort: AbortController
  started: boolean
}
const pending = new Map<string, Read>()
const aborted = () => new DOMException('Aborted', 'AbortError')

/** Share only in-flight reads, never settled fingerprints. Cancelling the last
 * offscreen consumer removes queued work; an invoke already running keeps its
 * lane and identity until completion, so scrolling back cannot duplicate it. */
export function readPreviewMetadata<T>(key: string, run: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) return Promise.reject(aborted())
  let current = pending.get(key)
  if (!current) {
    const read: Read = { promise: Promise.resolve(), users: new Set(), abort: new AbortController(), started: false }
    current = read
    pending.set(key, read)
    read.promise = Promise.resolve().then(() => enqueuePreviewMetadata(() => {
      read.started = true
      return run()
    }, read.abort.signal)).finally(() => {
      if (pending.get(key) === read) pending.delete(key)
    })
  }
  const read = current, user = Symbol()
  read.users.add(user)
  return new Promise<T>((resolve, reject) => {
    let settled = false
    const finish = () => {
      if (settled) return false
      settled = true
      signal?.removeEventListener('abort', cancel)
      read.users.delete(user)
      if (!read.users.size && !read.started) {
        if (pending.get(key) === read) pending.delete(key)
        read.abort.abort()
      }
      return true
    }
    const cancel = () => { if (finish()) reject(aborted()) }
    signal?.addEventListener('abort', cancel, { once: true })
    read.promise.then(value => { if (finish()) resolve(value as T) }, error => { if (finish()) reject(error) })
    if (signal?.aborted) cancel()
  })
}
