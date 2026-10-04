import type {
  CatalogueRef,
  EditContext,
  Fact,
  ReaderDocument,
  ReaderUnit,
} from './reader-ipc';
import { savedUnitRecord } from './reader-test-record';

export function knownFact<T>(value: T): Fact<T> {
  return { value: { state: 'known', value } };
}
function unknown<T>(): Fact<T> {
  return { value: { state: 'unknown' } };
}
function absent<T>(): Fact<T> {
  return { value: { state: 'absent' } };
}
function reference(id: string, label: string): Fact<CatalogueRef> {
  return knownFact({
    id,
    label: { state: 'known', value: label },
    description: { state: 'unknown' },
    asset_key: { state: 'unknown' },
  });
}

export function draftUnit(): ReaderUnit {
  return {
    key: 0,
    name: knownFact({ text: 'Ramza', origin: 'saved_rename' }),
    persistent_identity: unknown(),
    kind: knownFact('unique_human'),
    membership: knownFact('party'),
    sprite: unknown(),
    portrait: unknown(),
    stored: {
      level: knownFact(37),
      experience: knownFact(73),
      brave: knownFact(70),
      faith: knownFact(60),
      sex: knownFact('Male'),
      birthday: unknown(),
      zodiac: unknown(),
      statuses: unknown(),
      bases: {
        hp: knownFact(8_847_360),
        mp: knownFact(1_687_552),
        speed: knownFact(147_456),
        physical_attack: knownFact(229_376),
        magical_attack: knownFact(180_224),
      },
    },
    effective: {
      breakdown: {
        hp: {
          base: knownFact(540),
          equipment_bonus: knownFact(0),
          job_multiplier: knownFact(100),
          maximum: 999,
        },
        physical_attack: {
          base: knownFact(14),
          equipment_bonus: knownFact(0),
          job_multiplier: knownFact(100),
          maximum: 99,
        },
        mp: {
          base: knownFact(103),
          equipment_bonus: knownFact(0),
          job_multiplier: knownFact(100),
          maximum: 999,
        },
        speed: {
          base: knownFact(9),
          equipment_bonus: knownFact(0),
          job_multiplier: knownFact(100),
          maximum: 50,
        },
        magical_attack: {
          base: knownFact(11),
          equipment_bonus: knownFact(0),
          job_multiplier: knownFact(100),
          maximum: 99,
        },
      },
      hp: knownFact(540),
      mp: knownFact(103),
      speed: knownFact(9),
      physical_attack: knownFact(14),
      magical_attack: knownFact(11),
      movement_tiles: knownFact(4),
      jump_tiles: knownFact(3),
      evasion: knownFact([]),
    },
    growth: unknown(),
    current_job: reference('job:74', 'Squire'),
    jobs: unknown(),
    learned_abilities: unknown(),
    saved_ability_flags: unknown(),
    abilities: {
      primary_command: unknown(),
      secondary_command: absent(),
      reaction: absent(),
      support: absent(),
      movement: absent(),
    },
    equipment: {
      head: absent(),
      body: absent(),
      accessory: absent(),
      right_weapon: absent(),
      right_shield: absent(),
      left_weapon: absent(),
      left_shield: absent(),
    },
    combat_sets: unknown(),
    selected_combat_set: unknown(),
    guidance: {
      target_job: unknown(),
      samurai_prerequisites: unknown(),
      steel_cost: unknown(),
      job_eligible: unknown(),
      ability_purchasable: unknown(),
    },
    saved: {
      ...savedUnitRecord(),
      level: 37,
      exp: 73,
      start_bcp: 70,
      start_faith: 60,
      hp_max_base: 8_847_360,
      wt_base: 147_456,
    },
  };
}

export function draftDocument(unit = draftUnit()): ReaderDocument {
  return {
    schema: 'reader_v2',
    profile: 'english_steam_enhanced_manual',
    identity: {
      session: 'synthetic',
      snapshot_generation: 1,
      resource_generation: 1,
      resource_token: { state: 'known', value: 'a'.repeat(64) },
      manual_slot: 0,
    },
    roster: knownFact([unit]),
    gil: knownFact(100),
    inventory: knownFact([]),
    progress: {
      title: unknown(),
      saved_at_unix_seconds: unknown(),
      hero_name: unknown(),
      location: unknown(),
      difficulty: unknown(),
      difficulty_code: unknown(),
      chapter: unknown(),
      ramza_level: unknown(),
      story: unknown(),
      play_time_seconds: unknown(),
      next_event_id: unknown(),
      unnamed_event_values: unknown(),
      errands: unknown(),
      events: unknown(),
      recruitment: unknown(),
    },
  };
}

export function draftContext(): EditContext {
  return {
    snapshotGeneration: 1,
    manualSlotId: 0,
    gil: 100,
    backupAvailable: false,
    genericCreation: { targetPosition: null, donors: [] },
    storyAddition: { targetPosition: null, donors: [] },
    guestAddition: { targetPosition: null, donors: [] },
    gearOptions: [
      {
        unitPosition: 0,
        slots: [
          {
            slot: 'body',
            currentKey: '',
            choices: [{ key: 'item:150', label: 'Test armour' }],
          },
          {
            slot: 'accessory',
            currentKey: '',
            choices: [{ key: 'item:213', label: 'Speed boots' }],
          },
        ],
      },
    ],
  };
}
