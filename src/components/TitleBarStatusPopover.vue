<script setup lang="ts">
import { nextTick, shallowRef, useId, useTemplateRef, watch } from 'vue'
import { NPopover } from 'naive-ui'

defineOptions({ inheritAttrs: false })

const { label, title, heading, tone, rotating } = defineProps<{
  label: string
  title?: string
  heading?: string
  tone?: 'activity' | 'warning' | 'error'
  rotating?: boolean
}>()
const open = defineModel<boolean>('open', { default: false })
const button = useTemplateRef<HTMLButtonElement>('button')
const panel = useTemplateRef<HTMLElement>('panel')
const panelId = useId()
const focusedOpen = shallowRef(false)
watch(open, show => { if (!show) focusedOpen.value = false })

function hover(show: boolean) {
  if (!focusedOpen.value) open.value = show
}

function close(restoreFocus = false) {
  open.value = false
  focusedOpen.value = false
  if (restoreFocus) button.value?.focus()
}

async function click() {
  if (focusedOpen.value && open.value) {
    close(true)
    return
  }
  focusedOpen.value = true
  open.value = true
  await nextTick()
  if (open.value) panel.value?.focus({ preventScroll: true })
}

function clickOutside(event: MouseEvent) {
  if (!button.value?.contains(event.target as Node)) close()
}

function focusOut(event: FocusEvent) {
  if (event.relatedTarget instanceof Node && !panel.value?.contains(event.relatedTarget)
    && event.relatedTarget !== button.value) close()
}
</script>

<template>
  <NPopover :show="open" trigger="hover" placement="bottom" :z-index="5000" :show-arrow="false"
    :delay="0" :duration="125" @update:show="hover" @clickoutside="clickOutside">
    <template #trigger>
      <button ref="button" class="titlebar-status-button" :class="[tone, { rotating }]" type="button"
        :title="title" :aria-label="label" :aria-expanded="open" :aria-controls="open ? panelId : undefined"
        aria-haspopup="dialog" v-bind="$attrs" @click="click" @keydown.esc.stop="close(true)">
        <slot name="icon" />
        <span v-if="tone" class="status-dot" aria-hidden="true" />
      </button>
    </template>
    <section :id="panelId" ref="panel" class="titlebar-status-panel" role="dialog" :aria-label="label"
      tabindex="-1" @keydown.esc.stop="close(true)" @focusout="focusOut">
      <header v-if="heading" class="titlebar-status-heading">
        <strong>{{ heading }}</strong>
        <slot name="heading-action" />
      </header>
      <slot />
    </section>
  </NPopover>
</template>

<style scoped>
.titlebar-status-button {
  position: relative; display: grid; place-items: center; width: 32px; height: 32px;
  padding: 0; border-radius: 9px; border: 1px solid var(--oc2-rail-line);
  background: var(--oc2-card); color: var(--oc2-text-3); cursor: pointer;
  transition: color .16s, background-color .16s, border-color .16s;
}
.titlebar-status-button:is(:hover, :focus-visible, .activity) { color: var(--oc2-accent); }
.titlebar-status-button.warning { color: var(--oc2-status-warning); }
.titlebar-status-button.error { color: var(--oc2-status-error); }
.titlebar-status-button:is(.warning, .error) {
  border-color: currentColor;
  background: color-mix(in srgb, currentColor 15%, var(--oc2-card));
}
.titlebar-status-button:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.status-dot { position: absolute; right: 4px; bottom: 4px; width: 4px; height: 4px; border-radius: 50%; background: currentColor; }
.titlebar-status-button.rotating :deep(svg) { animation: status-orbit 1.6s linear infinite; }
.titlebar-status-panel { display: flex; flex-direction: column; gap: 9px; width: 272px; max-width: calc(100vw - 40px); color: var(--oc2-text-2); }
.titlebar-status-heading { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 28px; }
.titlebar-status-heading strong { min-width: 0; overflow-wrap: anywhere; }
.titlebar-status-heading :deep(button) { flex: none; }
@keyframes status-orbit { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .titlebar-status-button { transition: none; }
  .titlebar-status-button.rotating :deep(svg) { animation: none; }
}
</style>
