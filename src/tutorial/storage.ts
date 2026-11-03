/**
 * 教程进度的持久化。
 *
 * 只存一件事：这份教程**看完/跳过过没有**。教程没有「看到第几步」这种进度 ——
 * 中途关掉管理器再打开，从头再看一遍比从第 27 步接着讲更合理（用户早忘了
 * 前面讲过什么）。所以这里不做断点续播。
 *
 * 用 localStorage 而不是 `config.toml`：这是**这台机器的界面状态**，和
 * 界面缩放、主题一样属于客户端偏好；写进配置文件会跟着导入导出搬家，
 * 换台机器反而看不到教程。键名沿用 `oc2-` 前缀（`oc2-ui-zoom`、主题同族）。
 *
 * `storage` 参数可注入：Node 测试环境没有 localStorage，直接传假对象即可。
 */

export const TUTORIAL_STORAGE_KEY = 'oc2-tutorial'

/** 进度格式版本。将来步骤/文案大改、想强制所有人重看时递增即可。 */
export const TUTORIAL_PROGRESS_VERSION = 2

export interface TutorialProgress {
  version: number
  /** 看完/跳过的时间（ISO）；没有 = 还没看过。 */
  completedAt?: string
}

type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>

function defaultStorage(): StorageLike | null {
  try {
    // 隐私模式 / 测试环境里访问 localStorage 可能直接抛错
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}

export function readTutorialProgress(storage: StorageLike | null = defaultStorage()): TutorialProgress {
  if (!storage) return { version: TUTORIAL_PROGRESS_VERSION }
  try {
    const raw = storage.getItem(TUTORIAL_STORAGE_KEY)
    if (!raw) return { version: TUTORIAL_PROGRESS_VERSION }
    const parsed = JSON.parse(raw) as Partial<TutorialProgress> | null
    if (!parsed || typeof parsed !== 'object') return { version: TUTORIAL_PROGRESS_VERSION }
    // 旧版本进度一律当作「没看过」：格式换了，步骤也对不上了
    if (parsed.version !== TUTORIAL_PROGRESS_VERSION) return { version: TUTORIAL_PROGRESS_VERSION }
    return {
      version: TUTORIAL_PROGRESS_VERSION,
      completedAt: typeof parsed.completedAt === 'string' ? parsed.completedAt : undefined,
    }
  } catch {
    return { version: TUTORIAL_PROGRESS_VERSION }
  }
}

/** 是否已经看过（含中途跳过）。 */
export function isTutorialCompleted(storage: StorageLike | null = defaultStorage()): boolean {
  return Boolean(readTutorialProgress(storage).completedAt)
}

export function markTutorialCompleted(storage: StorageLike | null = defaultStorage()): void {
  if (!storage) return
  const progress: TutorialProgress = {
    version: TUTORIAL_PROGRESS_VERSION,
    completedAt: new Date().toISOString(),
  }
  try {
    storage.setItem(TUTORIAL_STORAGE_KEY, JSON.stringify(progress))
  } catch {
    // 存不下就算了：下次启动再问一遍，不影响本次使用
  }
}

/** 清掉进度（「重新播放教程」用，也方便排查问题）。 */
export function clearTutorialProgress(storage: StorageLike | null = defaultStorage()): void {
  if (!storage) return
  try {
    storage.removeItem(TUTORIAL_STORAGE_KEY)
  } catch {
    // 同上
  }
}
