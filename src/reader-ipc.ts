import { invoke } from '@tauri-apps/api/core';
import type {
  IpcError,
  NormalizedSave,
  SamuraiPrerequisiteProgress,
  SteelCostProgress,
  ValueState,
} from './ipc';

export interface Fact<T> {
  value: ValueState<T>;
}

export interface CatalogueRef {
  id: string;
  label: ValueState<string>;
  description: ValueState<string>;
  asset_key: ValueState<string>;
}

export interface ReaderIdentity {
  session: string;
  snapshot_generation: number;
  resource_generation: number;
  resource_token: ValueState<string>;
  manual_slot: number;
}

export interface ReaderUnitName {
  text: string;
  origin: 'saved_rename' | 'localized_catalogue' | 'upstream_fallback';
}

export interface StoredBases {
  hp: Fact<number>;
  mp: Fact<number>;
  speed: Fact<number>;
  physical_attack: Fact<number>;
  magical_attack: Fact<number>;
}

export interface StoredStats {
  level: Fact<number>;
  experience: Fact<number>;
  brave: Fact<number>;
  faith: Fact<number>;
  sex: Fact<string>;
  birthday: Fact<string>;
  zodiac: Fact<string>;
  statuses: Fact<CatalogueRef[]>;
  bases: StoredBases;
}

export interface EffectiveStats extends StoredBases {
  breakdown?: Partial<
    Record<
      BaseStatKind,
      {
        base: Fact<number>;
        equipment_bonus: Fact<number>;
        job_multiplier?: Fact<number>;
        maximum: number;
      }
    >
  >;
  movement_tiles: Fact<number>;
  jump_tiles: Fact<number>;
  evasion: Fact<
    {
      source:
        | 'character'
        | 'accessory'
        | 'right_shield'
        | 'left_shield'
        | 'right_weapon'
        | 'left_weapon';
      physical_basis_points: Fact<number>;
      magical_basis_points: Fact<number>;
    }[]
  >;
}

export interface GrowthCoefficients {
  hp: number;
  mp: number;
  speed: number;
  physical_attack: number;
  magical_attack: number;
}

export interface AbilityLoadout {
  primary_command: Fact<CatalogueRef>;
  secondary_command: Fact<CatalogueRef>;
  reaction: Fact<CatalogueRef>;
  support: Fact<CatalogueRef>;
  movement: Fact<CatalogueRef>;
}

export interface CurrentEquipment {
  head: Fact<CatalogueRef>;
  body: Fact<CatalogueRef>;
  accessory: Fact<CatalogueRef>;
  right_weapon: Fact<CatalogueRef>;
  right_shield: Fact<CatalogueRef>;
  left_weapon: Fact<CatalogueRef>;
  left_shield: Fact<CatalogueRef>;
}

export interface CombatSet {
  key: number;
  assigned: Fact<boolean>;
  name: Fact<string>;
  job: Fact<CatalogueRef>;
  head: Fact<CatalogueRef>;
  body: Fact<CatalogueRef>;
  accessory: Fact<CatalogueRef>;
  right_hand: Fact<CatalogueRef>;
  left_hand: Fact<CatalogueRef>;
  abilities: AbilityLoadout;
  double_hand: Fact<boolean>;
}

export interface ReaderUnit {
  key: number;
  persistent_identity: Fact<string>;
  name: Fact<ReaderUnitName>;
  kind: Fact<'regular_human' | 'unique_human' | 'monster'>;
  membership: Fact<'party' | 'guest' | 'inactive'>;
  sprite: Fact<string>;
  portrait: Fact<string>;
  stored: StoredStats;
  effective: EffectiveStats;
  growth: Fact<GrowthCoefficients>;
  current_job: Fact<CatalogueRef>;
  jobs: Fact<
    {
      slot: number;
      job: Fact<CatalogueRef>;
      level: Fact<number>;
      current_jp: Fact<number>;
      total_jp: Fact<number>;
    }[]
  >;
  learned_abilities: Fact<
    {
      ability: Fact<CatalogueRef>;
      learned: Fact<boolean>;
      jp_cost: Fact<number>;
    }[]
  >;
  saved_ability_flags: Fact<
    { slot: number; active_positions: number[]; passive_positions: number[] }[]
  >;
  abilities: AbilityLoadout;
  equipment: CurrentEquipment;
  combat_sets: Fact<CombatSet[]>;
  selected_combat_set: Fact<number>;
  guidance: {
    target_job: Fact<CatalogueRef>;
    samurai_prerequisites: Fact<SamuraiPrerequisiteProgress>;
    steel_cost: Fact<SteelCostProgress>;
    job_eligible: Fact<boolean>;
    ability_purchasable: Fact<boolean>;
  };
  saved: SavedUnitRecord;
}

export interface SavedCombatSet {
  name_raw: number[];
  name_padding: number[];
  equipment: number[];
  skillsets: number[];
  abilities: number[];
  job: number;
  is_double_hand: boolean;
}

export interface SavedUnitRecord {
  character: number;
  unit_index: number;
  job: number;
  union: number;
  sex: number;
  birthday: number;
  zodiac_sign: number;
  secondary_action: number;
  reaction_ability: number;
  support_ability: number;
  movement_ability: number;
  equip_items: number[];
  exp: number;
  level: number;
  start_bcp: number;
  start_faith: number;
  hp_max_base: number;
  mp_max_base: number;
  wt_base: number;
  at_base: number;
  mat_base: number;
  unlocked_jobs: number;
  ability_flags: number[][];
  job_levels_raw: number[];
  job_levels: number[];
  job_points: number[];
  total_job_points: number[];
  nickname_raw: number[];
  custom_job_name_raw: number[];
  unit_name_trailing: number[];
  name_no: number;
  in_trip: number;
  parasite: number;
  egg_color: number;
  psp_killed_num: number;
  unit_order_id: number;
  unit_starting_team: number;
  unit_join_id: number;
  current_combat_set: number;
  combat_sets: SavedCombatSet[];
  pad: number;
  chara_name_key: number;
  pad2: number[];
}

export interface ProgressEntry {
  subject: Fact<CatalogueRef>;
  saved_state: Fact<string>;
}

export interface SavedProgress {
  title: Fact<string>;
  saved_at_unix_seconds: Fact<number>;
  hero_name: Fact<string>;
  location: Fact<CatalogueRef>;
  difficulty: Fact<CatalogueRef>;
  difficulty_code: Fact<number>;
  chapter: Fact<string>;
  ramza_level: Fact<number>;
  story: Fact<ProgressEntry[]>;
  play_time_seconds: Fact<number>;
  next_event_id: Fact<number>;
  unnamed_event_values: Fact<number>;
  errands: Fact<ProgressEntry[]>;
  events: Fact<ProgressEntry[]>;
  recruitment: Fact<ProgressEntry[]>;
}

export interface ReaderDocument {
  item_details?: Record<string, string[]>;
  schema: 'reader_v2';
  profile: 'english_steam_enhanced_manual';
  identity: ReaderIdentity;
  roster: Fact<ReaderUnit[]>;
  inventory: Fact<
    {
      key: number;
      item: Fact<CatalogueRef>;
      category: Fact<string>;
      quantity: Fact<number>;
    }[]
  >;
  gil: Fact<number>;
  progress: SavedProgress;
}

export interface LoadReaderRequest {
  requestId: number;
  manualSlotId: number | null;
}

export interface LoadReaderResponse {
  requestId: number;
  occupiedManualSlots: number[];
  slotSummaries: SlotSummary[];
  save: NormalizedSave | null;
  reader: ReaderDocument | null;
  readerError: IpcError | null;
  editContext?: EditContext | null;
}

export type ZodiacSign =
  | 'aries'
  | 'taurus'
  | 'gemini'
  | 'cancer'
  | 'leo'
  | 'virgo'
  | 'libra'
  | 'scorpio'
  | 'sagittarius'
  | 'capricorn'
  | 'aquarius'
  | 'pisces';

export type UnitSex = 'male' | 'female';
export type GearSlot =
  'right_hand' | 'left_hand' | 'head' | 'body' | 'accessory';
export type GearSource = 'held' | 'create_and_equip';

export interface EditContext {
  gearOptions?: {
    unitPosition: number;
    slots: {
      slot: GearSlot;
      currentKey: string;
      choices: { key: string; label: string }[];
    }[];
  }[];
  zodiacOptions?: {
    unitPosition: number;
    current: ZodiacSign;
    choices: { key: ZodiacSign; label: string }[];
  }[];
  sexOptions?: {
    unitPosition: number;
    current: UnitSex;
    choices: { key: UnitSex; label: string }[];
    unavailableReason: string | null;
  }[];
  abilityDescriptions?: Record<string, string>;
  snapshotGeneration: number;
  manualSlotId: number;
  gil: number;
  backupAvailable?: boolean;
  jobOptions?: {
    unitPosition: number;
    jobSlots: number[];
    lockedJobs?: {
      jobSlot: number;
      requires: { jobSlot: number; level: number; currentLevel: number }[];
    }[];
  }[];
  abilityOptions?: {
    unitPosition: number;
    jobSlot: number;
    commandKey: string;
    commandLabel: string;
    members: {
      key: string;
      label: string;
      learned: boolean;
      equipped: boolean;
      kind: 'action' | 'reaction' | 'support' | 'movement';
    }[];
  }[];
  loadoutOptions?: {
    unitPosition: number;
    secondaryChoices: { key: string; label: string }[];
    reactionChoices: { key: string; label: string }[];
    supportChoices: { key: string; label: string }[];
    movementChoices: { key: string; label: string }[];
    slots: {
      combatSet: number | null;
      slot: 'secondary_command' | 'reaction' | 'support' | 'movement';
      currentKey: string;
    }[];
  }[];
  jobLevelTotalJp?: number[];
  genericCreation: {
    targetPosition: number | null;
    donors: {
      sourceSlot: number;
      sourcePosition: number;
      unitPosition: number;
      label: string;
      level: number;
      sex: string;
    }[];
  };
  storyAddition: EditContext['genericCreation'];
  guestAddition: EditContext['genericCreation'];
  creatureAddition?: {
    targetPosition: number | null;
    monsters: NonNullable<EditContext['namedAddition']>['characters'];
    enemies: NonNullable<EditContext['namedAddition']>['characters'];
  };
  namedAddition?: EditContext['genericCreation'] & {
    characters: {
      key: string;
      label: string;
      available: boolean;
      sex?: 'Male' | 'Female';
      unavailableReason?: string;
    }[];
  };
}

export type BaseStatKind =
  'hp' | 'mp' | 'speed' | 'physical_attack' | 'magical_attack';
export const statFields = [
  ['hp', 'HP'],
  ['mp', 'MP'],
  ['physical_attack', 'PA'],
  ['magical_attack', 'MA'],
  ['speed', 'Speed'],
] as const;

export function previewDraft(request: {
  transaction: SaveTransactionRequest;
  solve?: { unitPosition: number; stat: BaseStatKind; value: number };
}): Promise<{ reader: ReaderDocument; solvedBase: number | null }> {
  return invoke('preview_draft', { request });
}

export function previewBaseStat(request: {
  stat: BaseStatKind;
  previousBase: number;
  value: number | null;
  jobMultiplier: number;
  equipmentBonus: number | null;
}): Promise<{ storedBase: number; base: Fact<number>; total: Fact<number> }> {
  return invoke('preview_base_stat', { request });
}

export interface SaveTransactionRequest {
  snapshotGeneration: number;
  manualSlotId: number;
  operations: (
    | {
        kind: 'create_creature';
        formKey: string;
        unitPosition: number;
        name: string;
      }
    | { kind: 'gil'; value: string }
    | { kind: 'inventory_quantity'; itemPosition: number; quantity: string }
    | {
        kind: 'character_level' | 'experience';
        unitPosition: number;
        value: string;
      }
    | {
        kind: 'base_stat';
        unitPosition: number;
        stat: BaseStatKind;
        value: number;
      }
    | { kind: 'bravery'; unitPosition: number; value: string }
    | { kind: 'faith'; unitPosition: number; value: string }
    | { kind: 'zodiac'; unitPosition: number; sign: ZodiacSign }
    | { kind: 'sex'; unitPosition: number; sex: UnitSex }
    | {
        kind: 'gear';
        unitPosition: number;
        slot: GearSlot;
        itemKey: string;
        source: GearSource | null;
      }
    | {
        kind: 'job_progress';
        unitPosition: number;
        jobSlot: number;
        level: string;
        currentJp: string;
        totalJp: string;
      }
    | {
        kind: 'learned_ability';
        unitPosition: number;
        jobSlot: number;
        abilityKey: string;
        learned: boolean;
      }
    | {
        kind: 'equipped_slot';
        unitPosition: number;
        combatSet: number | null;
        slot: 'secondary_command' | 'reaction' | 'support' | 'movement';
        valueKey: string;
      }
    | {
        kind: 'create_generic_from_slot';
        sourceSlot: number;
        sourcePosition: number;
        unitPosition: number;
        name: string;
      }
    | {
        kind: 'add_story_from_slot';
        sourceSlot: number;
        sourcePosition: number;
        unitPosition: number;
      }
    | {
        kind: 'add_guest_from_slot';
        sourceSlot: number;
        sourcePosition: number;
        unitPosition: number;
      }
    | {
        kind: 'add_named_from_unit';
        sourceSlot: number;
        sourcePosition: number;
        unitPosition: number;
        characterKey: string;
      }
  )[];
}

export function saveTransaction(
  request: SaveTransactionRequest,
): Promise<{ gil: number; backupCreated: boolean }> {
  return invoke<{ gil: number; backupCreated: boolean }>('save_transaction', {
    request,
  });
}

export function restoreLastBackup(request: {
  snapshotGeneration: number;
  manualSlotId: number;
}): Promise<null> {
  return invoke<null>('restore_last_backup', { request });
}

export interface SlotSummary {
  manualSlot: number;
  title: string | null;
  savedAtUnixSeconds: number | null;
  playTimeSeconds: number | null;
}

export function loadReader(
  request: LoadReaderRequest,
): Promise<LoadReaderResponse> {
  return invoke<LoadReaderResponse>('load_reader', { request });
}

export function previewJobProgress(request: {
  snapshotGeneration: number;
  manualSlotId: number;
  unitPosition: number;
  levels: { jobSlot: number; level: number }[];
}): Promise<{
  jobOptions: NonNullable<EditContext['jobOptions']>[number];
  invalidatedSlots: number[];
}> {
  return invoke('preview_job_progress', { request });
}

export function previewCharacter(
  request: SaveTransactionRequest,
): Promise<LoadReaderResponse> {
  return invoke('preview_character', { request });
}
