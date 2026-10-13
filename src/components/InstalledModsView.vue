<script setup lang="ts">
import { computed, ref, toRefs, type PropType } from 'vue'
import { ChevronRight, FileCog, FolderOpen, Pencil, Plus, RefreshCw, Trash2, X } from 'lucide-vue-next'
import { t } from '../i18n'
import { useListNavigationFocus } from '../composables/useListNavigationFocus'
import type { InstalledMod } from '../types'

const props = defineProps({
  focusModFiles: { type: Array as PropType<string[]>, default: () => [] },
  openModConfig: null, openModRepository: null,
  config: null, busy: null, saveHideUnidentifiedMods: null, openModDirectory: null, openModFileLocation: null, openModRecycleBin: null, gameBlocked: null,
  installLocalMod: null, rescanInstalledMods: null, mappedReposAvailable: null, checkModUpdates: null,
  installedMods: null, isFirstModInPackage: null, packageIsCollapsed: null, modDisplayIndex: null,
  packageEnabledForMod: null, packageMixedForMod: null, packageModCountForMod: null, packageNameForMod: null,
  toggleModPackage: null, toggleModPackageDetails: null, packageIdForMod: null, modDetailsExpanded: null,
  isLastModInPackage: null, packageChildIndex: null, modEnabledForView: null, isModToggling: null,
  isPackageToggling: null, toggleMod: null, toggleModDetails: null,
  installedModStatus: { type: Function as PropType<(mod: InstalledMod) => string>, required: true },
  installedModStatusLabel: null, installedModStatusClass: null, editModNote: null, uninstallMod: null, uninstallModPackage: null,
  displayModPath: null, modUninstallPending: null, cancelModUninstall: null,
  formatModPackageDeleteWarning: null, confirmModUninstall: null,
})
const dynamicProps = props as Record<string, any>
const emit = defineEmits<{ 'focus-finished': [] }>()
const installedList = ref<HTMLElement | null>(null)
const installedGroups = computed(() => {
  const groups: { id: string; packageId?: string; mods: { mod: InstalledMod; index: number }[] }[] = []
  const statusesByGroup = new Map<string, Map<string, InstalledMod[]>>()
  for (const [index, mod] of (dynamicProps.installedMods as InstalledMod[]).entries()) {
    const packageId = dynamicProps.packageIdForMod(mod) as string | undefined
    const id = packageId ? `package:${packageId}` : `mod:${mod.dll_file}`
    const previous = groups[groups.length - 1]
    if (previous?.id === id) previous.mods.push({ mod, index })
    else groups.push({ id, packageId, mods: [{ mod, index }] })
    const statuses = statusesByGroup.get(id) ?? new Map<string, InstalledMod[]>()
    const status = props.installedModStatus(mod)
    const members = statuses.get(status)
    if (members) members.push(mod)
    else statuses.set(status, [mod])
    statusesByGroup.set(id, statuses)
  }
  return groups.map((group) => ({
    ...group,
    statuses: [...statusesByGroup.get(group.id)!].map(([status, mods]) => ({ status, mods })),
  }))
})
const focusGroupIds = computed(() => {
  const files = new Set(props.focusModFiles)
  return installedGroups.value.filter((group) => group.mods.some(({ mod }) => files.has(mod.dll_file)))
    .map((group) => group.id)
})
const focusedMods = useListNavigationFocus({
  list: installedList,
  targetIds: () => focusGroupIds.value,
  targetAttribute: 'data-mod-focus-target',
  stickyHeadingSelector: ':scope > .table-head',
  onFinished: () => emit('focus-finished'),
})
const { config, busy, saveHideUnidentifiedMods, openModDirectory, openModFileLocation, openModRecycleBin, gameBlocked, installLocalMod, rescanInstalledMods, mappedReposAvailable, checkModUpdates, installedMods, isFirstModInPackage, packageIsCollapsed, modDisplayIndex, packageEnabledForMod, packageMixedForMod, packageModCountForMod, packageNameForMod, toggleModPackage, toggleModPackageDetails, packageIdForMod, modDetailsExpanded, isLastModInPackage, packageChildIndex, modEnabledForView, isModToggling, isPackageToggling, toggleMod, toggleModDetails, installedModStatusLabel, installedModStatusClass, editModNote, uninstallMod, uninstallModPackage, displayModPath, modUninstallPending, cancelModUninstall, formatModPackageDeleteWarning, confirmModUninstall } = toRefs(dynamicProps)
</script>

<template>
<section class="view-content">
        <div ref="tutorialInstalledActions" class="page-intro">
          <div>
            <p>{{ t('这里只显示当前 BepInEx/plugins 中已经安装的插件。') }}</p>
            <label class="check-control mod-filter-control mod-switch-control">
              <input class="mod-switch-input" type="checkbox"
                :checked="config?.hide_unidentified_mods ?? true" :disabled="!!busy"
                @change="saveHideUnidentifiedMods" />
              <span class="mod-switch" :class="{ 'mod-switch-on': config?.hide_unidentified_mods ?? true }" aria-hidden="true"><span class="mod-switch-thumb"></span></span>
              <span>{{ t('隐藏未识别 BepInEx GUID 的插件') }}</span>
            </label>
          </div>
          <div class="button-row"><button class="icon-button" data-tutorial="mod-recycle-bin"
              :disabled="!!busy" :title="t('回收站')" :aria-label="t('回收站')" @click="openModRecycleBin">
              <Trash2 :size="17" />
            </button><button class="icon-button" data-tutorial="mod-open-directory" :disabled="!!busy"
              :title="t('打开目录')" :aria-label="t('打开目录')" @click="openModDirectory">
              <FolderOpen :size="16" />
            </button><button class="icon-button" data-tutorial="mod-install-manual" :disabled="gameBlocked || !!busy"
              :title="t('手动安装模组')" :aria-label="t('手动安装模组')" @click="installLocalMod">
              <Plus :size="16" />
            </button><button class="icon-button" data-tutorial="mod-rescan" :disabled="!!busy"
              :title="t('扫描本地 MOD')" :aria-label="t('扫描本地 MOD')" @click="rescanInstalledMods">
              <RefreshCw :size="16" />
            </button><button class="primary-button" data-tutorial="mod-check-updates" :disabled="!mappedReposAvailable || !!busy"
              @click="checkModUpdates">
              <RefreshCw :size="16" />{{ t('检查模组更新') }}
            </button></div>
        </div>
        <div ref="installedList" class="panel table-panel">
          <div class="table-head"><span>{{ t('序号') }}</span><span>{{ t('状态') }}</span><span>{{ t('模组') }}</span><span>{{ t('操作') }}</span></div>
          <div v-for="group in installedGroups" :key="group.id" :data-mod-focus-target="group.id" tabindex="-1"
            :class="['mod-list-group', { 'mod-package-group': !!group.packageId, 'mod-focus': focusedMods.includes(group.id) }]">
          <template v-for="{ mod, index } in group.mods" :key="mod.dll_file">
            <div v-if="isFirstModInPackage(mod, index)" :class="['mod-package-heading', { 'mod-package-collapsed': packageIsCollapsed(mod) }]">
              <span class="mod-index">{{ modDisplayIndex(mod, index) }}</span>
              <button class="mod-switch" :class="{ 'mod-switch-on': packageEnabledForMod(mod), 'mod-switch-mixed': packageMixedForMod(mod) }" role="switch"
                :aria-checked="packageEnabledForMod(mod)" :title="packageEnabledForMod(mod) ? t('禁用 MOD 包') : t('启用 MOD 包')"
                :aria-label="packageEnabledForMod(mod) ? t('禁用 MOD 包') : t('启用 MOD 包')" :disabled="gameBlocked || !!busy || isPackageToggling(mod)"
                @click="toggleModPackage(mod)">
                <span class="mod-switch-thumb" aria-hidden="true"></span>
              </button>
              <div class="mod-package-cell">
                <div class="mod-package-title">
                  <button class="mod-details-toggle" :title="packageIsCollapsed(mod) ? t('展开') : t('折叠')" :aria-expanded="!packageIsCollapsed(mod)"
                    @click="toggleModPackageDetails(mod)"><ChevronRight :class="{ 'mod-details-expanded': !packageIsCollapsed(mod) }" :size="15" /></button>
                  <div class="mod-heading">
                    <b>{{ packageNameForMod(mod) }}</b>
                    <span class="badge muted">{{ packageModCountForMod(mod) }} {{ t('个子 MOD') }}</span>
                    <template v-for="summary in group.statuses" :key="summary.status">
                      <button v-if="['有更新', '最新版'].includes(summary.status)" type="button" class="badge mod-package-status mod-update-link"
                        :class="installedModStatusClass(summary.mods[0])"
                        @click="dynamicProps.openModRepository(mod, summary.status)">{{ summary.mods.length }} {{ t(summary.status) }}</button>
                      <span v-else class="badge mod-package-status" :class="installedModStatusClass(summary.mods[0])"
                        :title="summary.status === '重复' ? t('检测到多个相同 GUID 和版本的 MOD，管理器会随机加载一个 MOD，建议删除重复 MOD') : undefined">
                        <RefreshCw v-if="summary.status === '检查中'" class="spin-icon" :size="11" />{{ summary.mods.length }} {{ t(summary.status) }}
                      </span>
                    </template>
                  </div>
                </div>
              </div>
              <div class="mod-controls">
                <span class="mod-actions"><button class="icon-button small danger-icon" :title="t('卸载 MOD 包')"
                    :disabled="gameBlocked || !!busy" @click="uninstallModPackage(mod)">
                    <Trash2 :size="14" />
                  </button></span>
              </div>
            </div>
            <Transition name="mod-package-expand">
              <div v-if="!packageIdForMod(mod) || !packageIsCollapsed(mod)"
                :data-mod-file="mod.dll_file"
                :class="['mod-row-shell', { 'mod-package-shell': !!packageIdForMod(mod), 'has-mod-details': modDetailsExpanded[mod.dll_file] }]">
                <div :class="['table-row', { 'mod-package-child': !!packageIdForMod(mod), 'mod-package-child-first': isFirstModInPackage(mod, index), 'mod-package-child-last': isLastModInPackage(mod, index) }]">
            <span class="mod-index">{{ packageIdForMod(mod) ? packageChildIndex(mod) : modDisplayIndex(mod, index) }}</span>
            <button class="mod-switch" :class="{ 'mod-switch-on': modEnabledForView(mod) }" role="switch"
              :aria-checked="modEnabledForView(mod)" :title="modEnabledForView(mod) ? t('禁用 MOD') : t('启用 MOD')"
              :aria-label="modEnabledForView(mod) ? t('禁用 MOD') : t('启用 MOD')" :disabled="gameBlocked || !!busy || isModToggling(mod)"
              @click="toggleMod(mod)">
              <span class="mod-switch-thumb" aria-hidden="true"></span>
            </button>
            <div class="mod-cell">
              <div class="mod-details">
                <div class="mod-details-layout">
                  <button class="mod-details-toggle" data-tutorial="mod-details-toggle" :title="modDetailsExpanded[mod.dll_file] ? t('折叠') : t('展开')"
                    @click="toggleModDetails(mod.dll_file)"><ChevronRight :class="{ 'mod-details-expanded': modDetailsExpanded[mod.dll_file] }" :size="15" /></button>
                  <div class="mod-details-content">
                    <div class="mod-heading"><b>{{ mod.name }}</b>
                      <span class="mod-version mono">{{ mod.version || t('未知版本') }}</span><button v-if="['有更新', '最新版'].includes(props.installedModStatus(mod))" type="button"
                        class="badge mod-update-link" :class="installedModStatusClass(mod)" @click="dynamicProps.openModRepository(mod)">{{ installedModStatusLabel(mod) }}</button><span v-else class="badge"
                        :title="mod.duplicate_version ? t('检测到多个相同 GUID 和版本的 MOD，管理器会随机加载一个 MOD，建议删除重复 MOD') : undefined"
                        :class="installedModStatusClass(mod)"><RefreshCw v-if="installedModStatusLabel(mod) === t('检查中')" class="spin-icon" :size="11" />{{ installedModStatusLabel(mod) }}</span>
                        </div>
                    </div>
                  </div>
                </div>
              </div>
                  <div class="mod-controls">
              <span class="mod-actions" data-tutorial="mod-actions"><button v-if="mod.has_config && mod.guid" class="icon-button small" data-tutorial="mod-open-config"
                  :title="t('打开配置')" :aria-label="t('打开配置')" :disabled="!!busy"
                  @click="dynamicProps.openModConfig(mod)">
                  <FileCog :size="14" />
                </button><button class="icon-button small" data-tutorial="mod-open-file-location"
                  :title="t('打开目录')" :aria-label="t('打开目录')" :disabled="!!busy || isModToggling(mod)"
                  @click="openModFileLocation(mod)">
                  <FolderOpen :size="14" />
                </button><button class="icon-button small" :title="t('编辑 MOD 备注')"
                  @click="editModNote(mod)">
                  <Pencil :size="14" />
                </button><button class="icon-button small danger-icon" :title="t('删除 MOD')"
                  :disabled="gameBlocked || !!busy" @click="uninstallMod(mod)">
                  <Trash2 :size="14" />
                </button></span>
                  </div>
                </div>
                <Transition name="mod-details-expand">
                  <dl v-if="modDetailsExpanded[mod.dll_file]" class="mod-detail-table mod-detail-panel">
                    <div v-if="mod.repository_binding" class="mod-detail-field"><dt>{{ t('仓库 Owner') }}</dt><dd>{{ mod.repository_binding.provider === 'github' ? 'GitHub' : 'Gitee' }} / {{ mod.repository_binding.owner }}</dd></div>
                    <div v-if="mod.repository_binding" class="mod-detail-field"><dt>{{ t('仓库 Repository') }}</dt><dd>{{ mod.repository_binding.repository }}</dd></div>
                    <div v-if="mod.repository_binding" class="mod-detail-field mod-detail-wide"><dt>{{ t('已安装仓库 Tag') }}</dt><dd>{{ mod.repository_binding.tag }}</dd></div>
                    <div v-if="mod.guid" class="mod-detail-field mod-detail-wide"><dt>{{ t('BepInEx.GUID') }}</dt><dd class="mod-guid">{{ mod.guid }}</dd></div>
                    <div v-if="mod.product_name" class="mod-detail-field"><dt>{{ t('DLL.产品名称') }}</dt><dd>{{ mod.product_name }}</dd></div>
                    <div v-if="mod.original_filename" class="mod-detail-field"><dt>{{ t('DLL.原始文件名') }}</dt><dd>{{ mod.original_filename }}</dd></div>
                    <div v-if="mod.title" class="mod-detail-field"><dt>{{ t('DLL.程序集标题') }}</dt><dd>{{ mod.title }}</dd></div>
                    <div v-if="mod.company" class="mod-detail-field"><dt>{{ t('DLL.开发者信息') }}</dt><dd>{{ mod.company }}</dd></div>
                    <div v-if="mod.copyright" class="mod-detail-field"><dt>{{ t('DLL.版权信息') }}</dt><dd>{{ mod.copyright }}</dd></div>
                    <div v-if="mod.description" class="mod-detail-field mod-detail-wide"><dt>{{ t('DLL.文件描述') }}</dt><dd>{{ mod.description }}</dd></div>
                    <div v-if="mod.dll_file" class="mod-detail-field mod-detail-wide"><dt>{{ t('DLL.插件路径') }}</dt><dd class="mod-path">{{ displayModPath(mod) }}</dd></div>
                    <div v-if="mod.note" class="mod-detail-field mod-detail-wide mod-note"><dt>{{ t('备注') }}</dt><dd>{{ mod.note }}</dd></div>
                  </dl>
                </Transition>
              </div>
            </Transition>
          </template>
          </div>
          <div v-if="!installedMods.length" class="empty-state">{{ t(config?.hide_unidentified_mods ? '没有扫描到可识别的 BepInEx MOD' : '没有扫描到 BepInEx/plugins/**/*.dll') }}</div>
        </div>
        <Transition name="modal-fade">
          <div v-if="modUninstallPending && !(modUninstallPending.kind === 'mod' && modUninstallPending.phase === 'package')" class="modal-backdrop mod-uninstall-backdrop" @click.self="cancelModUninstall">
            <div class="modal source-manager-modal mod-uninstall-modal unmanaged-mod-submenu mod-uninstall-submenu" role="alertdialog" aria-live="polite">
              <div class="modal-head">
                <div>
                  <h3>{{ modUninstallPending.kind === 'package' ? t('卸载 MOD 包') : modUninstallPending.phase === 'package' ? t('确认卸载单个 MOD') : modUninstallPending.phase === 'unmanaged' ? t('确认删除未托管 MOD') : t('确定删除 MOD') }}</h3>
                  <p class="muted-text">{{ modUninstallPending.kind === 'package' ? modUninstallPending.packageName : modUninstallPending.mod?.name }}</p>
                </div><button class="icon-button" :title="t('关闭')" @click="cancelModUninstall">
                  <X :size="17" />
                </button>
              </div>
              <p v-if="modUninstallPending.kind === 'package'" class="warning unmanaged-mod-warning">{{ t('卸载 MOD 包会将安装清单记录的文件移入回收站，可随时恢复。') }}</p>
              <p v-else-if="modUninstallPending.phase === 'package'" class="warning unmanaged-mod-warning">{{ formatModPackageDeleteWarning(modUninstallPending) }}</p>
              <p v-else-if="modUninstallPending.phase === 'unmanaged'" class="warning unmanaged-mod-warning">{{ t('该 MOD 不是通过本管理器安装的，仅将 DLL 移入回收站；其他相关文件需手动处理。是否继续？') }}</p>
              <p v-else class="muted-text">{{ t('删除 MOD 会将对应文件移入回收站，可以从回收站恢复。') }}</p>
              <div class="modal-actions"><button class="secondary-button" :disabled="!!busy" @click="cancelModUninstall">{{ t('否') }}</button><button class="danger-button" :disabled="!!busy" @click="confirmModUninstall">
                  <Trash2 :size="15" />{{ t('是') }}
                </button></div>
            </div>
          </div>
        </Transition>
        <Transition name="modal-fade">
          <div v-if="modUninstallPending?.kind === 'mod' && modUninstallPending.phase === 'package'" class="modal-backdrop mod-uninstall-backdrop" @click.self="cancelModUninstall">
            <div class="modal source-manager-modal mod-uninstall-modal" role="alertdialog" aria-live="polite">
              <div class="modal-head">
                <div>
                  <h3>{{ t('确认卸载单个 MOD') }}</h3>
                  <p class="muted-text">{{ modUninstallPending.mod?.name }}</p>
                </div><button class="icon-button" :title="t('关闭')" @click="cancelModUninstall"><X :size="17" /></button>
              </div>
              <p class="warning unmanaged-mod-warning">{{ formatModPackageDeleteWarning(modUninstallPending) }}</p>
              <div class="modal-actions"><button class="secondary-button" :disabled="!!busy" @click="cancelModUninstall">{{ t('否') }}</button><button class="danger-button" :disabled="!!busy" @click="confirmModUninstall">
                <Trash2 :size="15" />{{ t('是') }}
              </button></div>
            </div>
          </div>
        </Transition>
      </section>
</template>
