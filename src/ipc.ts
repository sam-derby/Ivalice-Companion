import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

export type ValueState<T> =
  | { state: 'known'; value: T }
  | { state: 'unknown' }
  | { state: 'absent' }
  | { state: 'unsupported' };

export type IpcErrorCategory =
  | 'input'
  | 'selection'
  | 'settings'
  | 'snapshot'
  | 'resource'
  | 'unsupported'
  | 'corrupt'
  | 'limit'
  | 'cancelled'
  | 'internal';

export interface IpcError {
  category: IpcErrorCategory;
  code: string;
  retryable: boolean;
  attempts: number | null;
  last_transient: 'source_busy' | 'source_changed' | null;
}

export type SaveSelectionStatus =
  | { state: 'no_selection' }
  | { state: 'selected' }
  | { state: 'unavailable'; error: IpcError };

export interface SnapshotProvenance {
  byte_length: number;
}

export interface SaveProvenance {
  snapshot: SnapshotProvenance;
  writer_build: ValueState<string>;
}

export interface ContainerMetadata {
  embedded_payload_version: number;
  format_discriminator: number;
  payload_byte_length: number;
  stored_adler_status: 'matched' | 'mismatched';
}

export interface ManualSlot {
  id: number;
}

export interface SteelCostProgress {
  cost_jp: number;
  shortfall_jp: number;
  state: 'enough' | 'shortfall';
}

export interface SamuraiPrerequisiteLevels {
  squire: number;
  knight: number;
  archer: number;
  monk: number;
  thief: number;
  dragoon: number;
}

export interface LevelRequirementProgress {
  current_level: number;
  required_level: number;
  remaining_levels: number;
  state: 'met' | 'missing';
}

export interface SamuraiPrerequisiteProgress {
  squire: LevelRequirementProgress;
  knight: LevelRequirementProgress;
  archer: LevelRequirementProgress;
  monk: LevelRequirementProgress;
  thief: LevelRequirementProgress;
  dragoon: LevelRequirementProgress;
  all_level_requirements_met: boolean;
}

export interface NormalizedSave {
  schema: 'v6';
  provenance: SaveProvenance;
  container: ValueState<ContainerMetadata>;
  selected_manual_slot: ValueState<ManualSlot>;
}

export function getSaveSelection(): Promise<SaveSelectionStatus> {
  return invoke<SaveSelectionStatus>('get_save_selection');
}

export async function chooseSave(): Promise<SaveSelectionStatus | null> {
  const path = await open({
    title: 'Choose an Enhanced save',
    directory: false,
    multiple: false,
    filters: [{ name: 'Enhanced save', extensions: ['png'] }],
  });
  if (path === null) {
    return null;
  }
  return invoke<SaveSelectionStatus>('set_save_selection', { path });
}

export async function chooseImportFile(): Promise<string | null> {
  return open({
    title: 'Choose a save to import from',
    directory: false,
    multiple: false,
    filters: [{ name: 'Enhanced save', extensions: ['png'] }],
  });
}

/** The system dialog confirms overwrites. */
export async function chooseExportFile(slot: number): Promise<string | null> {
  return save({
    title: `Export slot ${String(slot + 1)}`,
    defaultPath: `slot-${String(slot + 1)}-enhanced.png`,
    filters: [{ name: 'Enhanced save', extensions: ['png'] }],
  });
}
