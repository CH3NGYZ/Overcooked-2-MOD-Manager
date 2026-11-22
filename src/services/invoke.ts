import { invoke as invokeNative, type InvokeArgs } from '@tauri-apps/api/core'
import type { UpdateStage } from '../appUpdateState.ts'
import { isErrorTag } from '../locale/backend.ts'

type FileDropResult = 'update_gate' | 'no_consumer' | 'no_path' | 'busy'
  | 'unsupported_type' | 'dispatched' | 'listener_failed'

export function recordFileDrop(result: FileDropResult, updateStage: UpdateStage, paths?: readonly string[]) {
  const suffix = paths?.[0]?.match(/\.([a-z\d]{1,12})$/i)?.[1].toLowerCase()
  const fileType = suffix === 'zip' || suffix === 'dll' ? suffix : paths?.length ? 'other' : 'none'
  void Promise.resolve()
    .then(() => invokeNative<void>('record_file_drop', { result, updateStage, fileType, count: paths?.length ?? 0 }))
    .catch(() => undefined)
}

function knownTag(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined
  const tag = value.split(/[?:：]/, 1)[0]
  return tag.length <= 96 && isErrorTag(tag) ? tag : undefined
}

function reportFailure(command: string, start: number, rejected: boolean, reason: unknown) {
  const elapsedMs = Math.min(0xffffffff, Math.round(performance.now() - start))
  const tag = knownTag(reason instanceof Error ? reason.message : reason)
  void Promise.resolve()
    .then(() => invokeNative<void>('record_ipc_failure', { command, elapsedMs, rejected, tag }))
    .catch(() => undefined)
}

export async function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  const start = performance.now()
  try {
    const result = await invokeNative<T>(command, args)
    if (result && typeof result === 'object' && 'ok' in result && result.ok === false) {
      reportFailure(command, start, false, 'message' in result ? result.message : undefined)
    }
    return result
  } catch (reason) {
    reportFailure(command, start, true, reason)
    throw reason
  }
}
