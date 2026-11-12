import { computed } from 'vue'
import { t } from '../i18n'
import type { ChefEditorSnapshot, ChefItem } from '../diychef/types'
import { DEMO_ID_PREFIX, demoActive } from './demo'

const SESSION_ID = `${DEMO_ID_PREFIX}diychef.session`
export const diyChefDemoIds = {
  blackCat: 'native:Chef_Black_cat', fox: 'custom:00-Fox', racoon: 'native:Chef_Racoon',
} as const

/** Keep recognizing an outgoing demo component after the shared tutorial flag clears. */
export function isDiyChefDemoSession(sessionId: string): boolean {
  return sessionId === SESSION_ID
}

/** These are bundled renders of real models. A missing combination never starts a live render. */
export function diyChefDemoImage(sessionId: string, chefId: string, hatId: string | null): string | null {
  if (!isDiyChefDemoSession(sessionId) || !hatId) return null
  if (chefId === diyChefDemoIds.fox && hatId === 'hat:Wizardhat') return '/tutorial/diychef/fox-wizardhat.png'
  if (hatId !== 'hat:' && hatId !== 'hat:None') return null
  const image = {
    [diyChefDemoIds.blackCat]: 'black-cat', [diyChefDemoIds.fox]: 'fox', [diyChefDemoIds.racoon]: 'racoon',
  }[chefId]
  return image ? `/tutorial/diychef/${image}.png` : null
}

function buildSnapshot(): ChefEditorSnapshot {
  const chef = (id: string, source: 'native' | 'custom', config: string, label: string): ChefItem => ({
    id, source_kind: source, display_name: label, config_name: config, head_name: config,
    custom_id: source === 'custom' ? 100 : null, resource_state: source === 'custom' ? 'valid' : 'known_name', preview_state: 'ready',
  })
  return {
    session_id: SESSION_ID,
    target: { id: `${DEMO_ID_PREFIX}diychef.target`, guid: 'dev.gua.overcooked.diychef', version: '2.2.0', display_path: '', enabled: true, generation: 0, compatibility: 'fastinit_2_2', supports_custom_default: true },
    source: { state: 'present', filename: 'prefer.txt', revision: SESSION_ID, encoding: 'utf8', newline: 'crlf' },
    catalog_revision: SESSION_ID,
    chefs: [
      chef(diyChefDemoIds.blackCat, 'native', 'Chef_Black_cat', t('tutorial.diychef.demo.blackCat')),
      chef(diyChefDemoIds.fox, 'custom', '00-Fox', t('tutorial.diychef.demo.fox')),
      chef(diyChefDemoIds.racoon, 'native', 'Chef_Racoon', t('tutorial.diychef.demo.racoon')),
    ],
    hats: [
      { id: 'hat:', kind: 'auto', config_value: '', display_name: t('diychef.hatAuto'), resource_state: 'known_name', preview_state: 'ready' },
      { id: 'hat:None', kind: 'none', config_value: 'None', display_name: t('diychef.hatNone'), resource_state: 'known_name', preview_state: 'ready' },
      { id: 'hat:Wizardhat', kind: 'custom', config_value: 'Wizardhat', display_name: 'Wizardhat', resource_state: 'valid', preview_state: 'ready' },
    ],
    document_rows: [],
    selection: {
      entries: [diyChefDemoIds.blackCat, diyChefDemoIds.fox].map((id, index) => ({
        entry_id: `${DEMO_ID_PREFIX}diychef.row.${index}`, row_id: null,
        chef_ref: { kind: 'known', chef_id: id }, hat_ref: { kind: 'known', hat_id: 'hat:' },
      })),
      hat_bindings: Object.fromEntries([diyChefDemoIds.blackCat, diyChefDemoIds.fox].map(id => [id, { kind: 'known' as const, hat_id: 'hat:' }])),
      lobby_switch: true,
    },
    diagnostics: [],
  }
}

/** A fresh editor-only snapshot for each run; never enters the dashboard or IPC. */
export const diyChefDemoSnapshot = computed(() => demoActive.value ? buildSnapshot() : null)
