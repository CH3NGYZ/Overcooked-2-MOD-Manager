/** Mirrors models/diychef.rs. Opaque IDs never contain caller supplied paths. */
import type { InstalledMod } from '../types'
export interface LocalModSnapshot { generation: number; mods: InstalledMod[]; diy_level_target?: string | null }
export interface ChefTarget { id: string; guid: string; version: string | null; display_path: string; enabled: boolean; generation: number; compatibility: string; supports_custom_default?: boolean }
export interface ChefSource { state: string; filename: string; revision: string; encoding: string; newline: string }
export interface ChefItem { id: string; source_kind: string; display_name: string; config_name: string; head_name: string; custom_id: number | null; resource_state: string; preview_state: string }
export interface ChefHat { id: string; kind: string; config_value: string; display_name: string; resource_state: string; preview_state: string }
export type ChefReference = { kind: 'known'; chef_id: string } | { kind: 'unresolved'; raw_key: string; row_id: string }
export type HatReference = { kind: 'known'; hat_id: string } | { kind: 'unresolved'; raw_value: string; row_id: string }
export interface ChefEntry { entry_id: string; row_id: string | null; chef_ref: ChefReference; hat_ref: HatReference }
export interface ChefSelection { entries: ChefEntry[]; lobby_switch: boolean }
export interface ChefDocumentRow { row_id: string; category: string; raw_display: string; duplicate_group_id: string | null }
export interface ChefDiagnostic { id: string; severity: string; code: string; chef_id: string | null; hat_id: string | null; row_id: string | null; save_blocking: boolean }
export interface ChefChange { id: string; before: string; after: string }
export interface ChefValidation { diagnostics: ChefDiagnostic[]; changes: ChefChange[]; save_blocking: boolean }
export interface ChefDraft { base_revision: string; catalog_revision: string; target_generation: number; entries: ChefEntry[]; lobby_switch: boolean; removed_problem_row_ids: string[]; acknowledged_change_ids: string[]; reset_intent: string | null }
export interface ChefEditorSnapshot { session_id: string; target: ChefTarget; source: ChefSource; catalog_revision: string; chefs: ChefItem[]; hats: ChefHat[]; document_rows: ChefDocumentRow[]; selection: ChefSelection; diagnostics: ChefDiagnostic[] }
export interface ChefEditorOpen { status: 'ready' | 'not_installed' | 'select_target'; candidates: ChefTarget[]; snapshot: ChefEditorSnapshot | null }
