<script setup lang="ts">
import { computed, type DeepReadonly } from 'vue'
import { NTag } from 'naive-ui'
import { t } from '../i18n'
import { translateBackendMessage } from '../locale/backend'
import { changeReasonKeys } from './messages'
import type { ChangePlan } from './types'

const { plan } = defineProps<{ plan: DeepReadonly<ChangePlan> }>()
const labels = computed(() => new Map(plan.nodes.map(node => [node.node_id, node.label])))
const changedNodes = computed(() => plan.nodes.filter(node => node.members?.length
  && plan.actions.some(action => action.node_id === node.node_id)))
function reasonText(reason: string) { return changeReasonKeys[reason] ? t(changeReasonKeys[reason]!) : translateBackendMessage(reason) }
function memberLabel(nodeId: string, memberKey: string) {
  return plan.nodes.find(node => node.node_id === nodeId)?.members?.find(member => member.key === memberKey)?.dll_file ?? memberKey
}
</script>

<template>
  <details class="plan-details">
    <summary>{{ t('modChanges.details') }}</summary>
    <div class="details-content">
      <div v-for="root in plan.roots" :key="root.node_id" class="plan-root">
        <strong>{{ root.label }}</strong><NTag v-if="root.tag" size="small">{{ root.tag }}</NTag>
      </div>
      <article v-for="(action, index) in plan.actions" :key="`${action.node_id}:${index}`">
        <strong>{{ labels.get(action.node_id) || t('modChanges.unnamedItem') }}</strong>
        <span v-if="action.member_key"> · {{ memberLabel(action.node_id, action.member_key) }}</span>
        <p v-if="action.reason">{{ reasonText(action.reason) }}</p>
        <p v-if="action.caused_by.length">{{ t('modChanges.causedBy', { names: action.caused_by.map(id => labels.get(id) || t('modChanges.unnamedItem')).join(', ') }) }}</p>
      </article>
      <section v-if="changedNodes.length" class="plan-package-members">
        <h4>{{ t('modChanges.members') }}</h4>
        <article v-for="node in changedNodes" :key="node.node_id">
          <strong>{{ node.label }}</strong>
          <ul>
            <li v-for="member in node.members" :key="member.key">
              <NTag :type="member.enabled ? 'success' : 'default'" size="small">{{ t(member.enabled ? 'modChanges.action.enable' : 'modChanges.action.disable') }}</NTag>
              <span>{{ member.dll_file }}</span>
            </li>
          </ul>
        </article>
      </section>
      <ul v-if="plan.impacts.length">
        <li v-for="(impact, index) in plan.impacts" :key="index">{{ translateBackendMessage(impact) }}</li>
      </ul>
    </div>
  </details>
</template>

<style scoped>
.plan-details { color: var(--oc2-text-3); font-size: 12px; }
.plan-details summary { cursor: pointer; width: fit-content; border-radius: 4px; }
.plan-details summary:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 4px; }
.details-content { display: flex; flex-direction: column; gap: 12px; padding-top: 12px; overflow-wrap: anywhere; }
.plan-root { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.details-content p { margin: 4px 0 0; }
.details-content h4 { margin: 0 0 8px; }
.details-content ul { padding-left: 18px; margin: 8px 0 0; }
.plan-package-members li { margin-bottom: 6px; }
.plan-package-members li > span { margin-left: 8px; }
</style>
