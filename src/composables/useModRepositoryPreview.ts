/** Repository buttons share one backend-owned batch plan, including the root. */
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { cachedInstallRelease } from '../repositoryAssets'
import { useModChanges } from '../modChanges/store'
import type { ModRepository } from '../types'

export interface ModRepositoryPreviewOptions {
  gameBlocked?: () => boolean
  onBusyChange?: (repositoryId: string, busy: boolean) => void
  onApplied?: () => void | Promise<void>
}

/** Downloads, actual-package verification and root installation form one batch. */
export function useModRepositoryPreview(options: ModRepositoryPreviewOptions = {}) {
  const message = useNotify()
  const changes = useModChanges()

  async function previewModRepository(repo: ModRepository, tag?: string, assetName?: string): Promise<void> {
    if (options.gameBlocked?.()) return
    const raw = repo.download_mode === 'repository_file'
    const release = raw ? undefined : cachedInstallRelease(repo.cached_releases, tag, repo.pinned_version)
    if (!raw && !release) { message.error(t('source.noInstallableRelease')); return }
    options.onBusyChange?.(repo.id, true)
    try {
      const result = await changes.run({ roots: [{
        node_id: `mod:${repo.id}`,
        candidate_id: raw ? `raw:${repo.id}` : `release:${repo.id}:${release!.tag_name}`,
        enabled: true,
        ...(assetName ? { asset_name: assetName } : {}),
      }] })
      if (!result) return
      if (result.message) message.success(translateBackendMessage(result.message))
      await options.onApplied?.()
    } catch (reason) {
      message.error(backendMessageOf(reason))
    } finally {
      options.onBusyChange?.(repo.id, false)
    }
  }

  return { previewModRepository }
}
