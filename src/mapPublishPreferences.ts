import type { PublishProvider } from './types'

type ProviderStorage = Pick<Storage, 'getItem' | 'setItem'>
const PROVIDER_KEY = 'oc2-map-publish-provider'
const DEFAULT_PROVIDER: PublishProvider = 'github'

function localPreferenceStorage(): ProviderStorage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}

/** 渠道属于本机界面偏好；身份和 Token 继续由原授权链管理。 */
export function readPublishProvider(storage: ProviderStorage | null = localPreferenceStorage()): PublishProvider {
  try {
    const value = storage?.getItem(PROVIDER_KEY)
    return value === 'github' || value === 'gitcode' ? value : DEFAULT_PROVIDER
  } catch {
    return DEFAULT_PROVIDER
  }
}

export function savePublishProvider(provider: PublishProvider, storage: ProviderStorage | null = localPreferenceStorage()): void {
  try {
    storage?.setItem(PROVIDER_KEY, provider)
  } catch {
    // 存储不可用不影响本次渠道切换和发布。
  }
}
