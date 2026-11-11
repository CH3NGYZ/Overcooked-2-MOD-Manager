type Job = {
  run: () => Promise<unknown>
  resolve: (value: unknown) => void
  reject: (error: unknown) => void
  detach: () => void
}
const jobs: Job[] = []
const preparations: Job[] = []
const metadata: Job[] = []
let running = false
let preparing = 0
let reading = 0
let reservations = 0
let configuredConcurrency = 0

/** Zero follows all logical cores; running work keeps its permit after a change. */
export function configurePreviewConcurrency(value: number) {
  configuredConcurrency = Number.isFinite(value) && value > 0 ? Math.max(1, Math.floor(value)) : 0
  pumpPreparations()
}

function preparationLimit() {
  const cores = globalThis.navigator?.hardwareConcurrency ?? 1
  const available = Number.isFinite(cores) ? Math.max(1, Math.floor(cores)) : 1
  return Math.min(configuredConcurrency || available, available)
}

/** Metadata and small PNG reads never queue behind the GPU. Their own bound
 * also covers an invoke that cannot be cancelled after Rust has started it. */
export function enqueuePreviewMetadata<T>(run: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  return enqueue(metadata, run, pumpMetadata, signal)
}

function pumpMetadata() {
  const limit = Math.min(4, globalThis.navigator?.hardwareConcurrency || 1)
  while (reading < limit && metadata.length) {
    const next = metadata.shift()!
    next.detach()
    reading += 1
    void (async () => {
      try { next.resolve(await next.run()) }
      catch (error) { next.reject(error) }
      finally { reading -= 1; pumpMetadata() }
    })()
  }
}

export function previewQueueStats() {
  return { reading, metadataQueued: metadata.length, preparing, preparationQueued: preparations.length,
    capturing: Number(running), captureQueued: jobs.length }
}

function enqueue<T>(queue: Job[], run: () => Promise<T>, start: () => void, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) return Promise.reject(new DOMException('Aborted', 'AbortError'))
  return new Promise<T>((resolve, reject) => {
    const cancel = () => {
      const index = queue.indexOf(job)
      if (index < 0) return
      queue.splice(index, 1)
      job.detach()
      reject(new DOMException('Aborted', 'AbortError'))
    }
    const job: Job = { run, resolve: value => resolve(value as T), reject,
      detach: () => signal?.removeEventListener('abort', cancel) }
    queue.push(job)
    signal?.addEventListener('abort', cancel, { once: true })
    start()
  })
}

/** The permit covers preparation, waiting for GPU, capture and disposal. */
export function enqueuePreviewPreparation<T>(run: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  return enqueue(preparations, run, pumpPreparations, signal)
}

function pumpPreparations() {
  while (!reservations && preparing < preparationLimit() && preparations.length) {
    const next = preparations.shift()!
    next.detach()
    preparing += 1
    void (async () => {
      try { next.resolve(await next.run()) }
      catch (error) { next.reject(error) }
      finally { preparing -= 1; pumpPreparations() }
    })()
  }
}

/** One serial lane owns the sole WebGL renderer; dialog reservations pause list work. */
export function enqueuePreview<T>(run: () => Promise<T>, signal?: AbortSignal): Promise<T> {
  return enqueue(jobs, run, () => { void pump() }, signal)
}
async function pump() {
  if (running || reservations) return
  const next = jobs.shift()
  if (!next) return
  next.detach()
  running = true
  try { next.resolve(await next.run()) }
  catch (error) { next.reject(error) }
  finally { running = false; void pump() }
}
export async function reservePreviewRenderer() {
  reservations += 1
  // Current list capture completes before a dialog takes its canvas.
  while (running) await new Promise<void>(resolve => setTimeout(resolve, 16))
  let released = false
  return () => {
    if (released) return
    released = true
    reservations -= 1
    void pump()
    pumpPreparations()
  }
}
