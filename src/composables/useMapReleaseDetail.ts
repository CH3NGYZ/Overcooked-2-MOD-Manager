import { computed, ref, toValue, watch, type MaybeRefOrGetter } from 'vue'
import { mapDependencyRows } from '../dependencyRows'
import { pickI18n, t } from '../i18n'
import { isPrereleaseVersion, formatVersion } from '../modStatus'
import { matchedReleaseAssets } from '../repositoryAssets'
import type { InstalledMod, MapInfo, ReleaseInfo, Repository, Provider } from '../types'

import { repositoryHomepage } from '../providers'

interface UseMapReleaseDetailOptions {
  maps: MaybeRefOrGetter<readonly MapInfo[]>
  detailId: MaybeRefOrGetter<string | null>
  mods: MaybeRefOrGetter<readonly InstalledMod[]>
  repositories: MaybeRefOrGetter<readonly Repository[]>
  mapName: (map: MapInfo) => string
}

/** Map-specific projection and selection state for the shared Release detail panel. */
export function useMapReleaseDetail(options: UseMapReleaseDetailOptions) {
  const selectedTag = ref<string | null>(null)
  const selectedAssetName = ref<string | null>(null)
  const mapList = () => toValue(options.maps)
  const activeMapId = () => toValue(options.detailId)

  const map = computed(() => mapList().find((entry) => entry.id === activeMapId()) ?? null)
  const releases = computed<ReleaseInfo[]>(() => map.value?.cached_releases ?? [])
  const latestTag = computed(() => releases.value[0]?.tag_name ?? null)
  const release = computed<ReleaseInfo | null>(() => {
    const selected = selectedTag.value
    return releases.value.find((entry) => entry.tag_name === selected) ?? releases.value[0] ?? null
  })
  const versions = computed(() => releases.value.map((entry) => ({
    label: formatVersion(entry.tag_name || entry.version || ''),
    value: entry.tag_name,
  })))
  const dependencies = computed(() => mapDependencyRows(map.value, {
    mods: toValue(options.mods),
    repositories: toValue(options.repositories),
    translate: t,
    pickName: pickI18n,
    version: release.value?.tag_name ?? null,
  }))
  const assets = computed(() => {
    const current = release.value
    if (!current) return []
    const matched = matchedReleaseAssets(current, map.value?.asset_rules ?? [])
    return matched.length ? matched : current.assets.filter((asset) => !!asset.download_url)
  })
  const asset = computed(() =>
    assets.value.find((entry) => entry.name === selectedAssetName.value)
    ?? assets.value[0]
    ?? null,
  )
  const releaseTag = computed(() => release.value?.tag_name || release.value?.name || '—')
  const prerelease = computed(() =>
    release.value
      ? isPrereleaseVersion(release.value.tag_name || release.value.version)
      : false,
  )
  const links = computed(() => {
    const current = map.value
    if (!current) return []
    const source = repositoryHomepage(current.provider, current.username, current.repository)
    return [
      { label: t('levelSet.sourcePage'), url: source },
      { label: t('levelSet.releasePage'), url: `${source}/releases/latest` },
    ]
  })
  const note = computed(() =>
    map.value ? pickI18n(map.value.note_i18n, map.value.note ?? '') : '',
  )
  const name = computed(() => (map.value ? options.mapName(map.value) : ''))
  const provider = computed<Provider>(() => map.value?.provider ?? 'github')
  const owner = computed(() => map.value?.username ?? '')
  const repository = computed(() => map.value?.repository ?? '')
  const bindingName = computed(() => map.value?.level_set_name ?? '')
  const author = computed(() => map.value?.author ?? '')
  const bindingValue = computed(() => {
    if (!bindingName.value) return ''
    return author.value ? `${bindingName.value} · ${author.value}` : bindingName.value
  })
  const primaryAction = computed<'install' | 'refresh'>(() =>
    releases.value.length ? 'install' : 'refresh',
  )

  watch(
    [activeMapId, latestTag],
    () => { selectedTag.value = latestTag.value },
    { immediate: true },
  )
  watch(
    [() => map.value?.id, () => release.value?.tag_name],
    () => { selectedAssetName.value = null },
  )

  return {
    detailMap: map,
    detailRelease: release,
    detailVersions: versions,
    detailDependencies: dependencies,
    detailAssets: assets,
    detailAsset: asset,
    detailSelectedTag: selectedTag,
    detailSelectedAsset: selectedAssetName,
    detailReleaseTag: releaseTag,
    detailPrerelease: prerelease,
    detailLinks: links,
    detailNote: note,
    detailName: name,
    detailProvider: provider,
    detailOwner: owner,
    detailRepository: repository,
    detailBindingValue: bindingValue,
    detailPrimaryAction: primaryAction,
  }
}
