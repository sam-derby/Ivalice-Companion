import { statFields } from './reader-ipc';
import type {
  BaseStatKind,
  EditContext,
  ReaderDocument,
  ReaderUnit,
  SaveTransactionRequest,
} from './reader-ipc';

export interface JobDraft {
  level: string;
  currentJp: string;
  totalJp: string;
}

export interface UnitDraft {
  level?: string;
  exp?: string;
  statBases?: Partial<Record<BaseStatKind, number>>;
  zodiac?: string;
  sex?: string;
  bravery?: string;
  faith?: string;
  jobs: Record<number, JobDraft>;
  abilities: Record<string, boolean>;
  loadout: Record<string, string>;
  gear: Record<
    string,
    { itemKey: string; source: 'held' | 'create_and_equip' | null }
  >;
}

export interface PendingAddition {
  operation: Extract<
    SaveTransactionRequest['operations'][number],
    {
      kind:
        'create_generic_from_slot' | 'add_named_from_unit' | 'create_creature';
    }
  >;
  label: string;
}

export interface EditorDraft {
  creature?: { formKey: string; name: string } | null;
  additions?: PendingAddition[];
  gil: string;
  storyStep?: string;
  calendar?: { month: number; day: number };
  /** Unlocked state by achievement save index. */
  achievements?: Record<number, boolean>;
  inventory: Record<number, string>;
  units: Record<number, UnitDraft>;
  generic: { sourceSlot: number; sourcePosition: number; name: string } | null;
  story: { sourceSlot: number; sourcePosition: number } | null;
  guest: { sourceSlot: number; sourcePosition: number } | null;
  named?: {
    sourceSlot: number;
    sourcePosition: number;
    characterKey: string;
  } | null;
}

export interface ReviewChange {
  group: string;
  label: string;
  before: string;
  after: string;
  summary?: string;
}

export interface DraftReview {
  operations: SaveTransactionRequest['operations'];
  changes: ReviewChange[];
  errors: string[];
  heldAfter: Record<number, number>;
}

export function emptyUnitDraft(): UnitDraft {
  return { jobs: {}, abilities: {}, loadout: {}, gear: {} };
}

export function abilityDraftKey(jobSlot: number, abilityKey: string): string {
  return `${String(jobSlot)}:${abilityKey}`;
}

export function loadoutDraftKey(
  combatSet: number | null,
  slot: string,
): string {
  return `${combatSet === null ? 'active' : String(combatSet)}:${slot}`;
}

function unitLabel(unit: ReaderUnit): string {
  const name = unit.name.value;
  return name.state === 'known' ? name.value.text : 'Party member';
}

function jobLabel(unit: ReaderUnit, slot: number): string {
  const jobs = unit.jobs.value;
  const job =
    jobs.state === 'known'
      ? jobs.value.find((row) => row.slot === slot)
      : undefined;
  const value = job?.job.value;
  return value?.state === 'known' && value.value.label.state === 'known'
    ? value.value.label.value
    : 'Job';
}

function add(
  review: DraftReview,
  group: string,
  label: string,
  before: number | string | boolean,
  after: number | string | boolean,
) {
  review.changes.push({
    group,
    label,
    before: String(before),
    after: String(after),
  });
}

export function storyStepLabel(context: EditContext, progress: number): string {
  const choice = context.storyChoices?.find(
    (option) => option.progress === progress,
  );
  return [String(progress), choice?.chapter, choice?.objective]
    .filter((part) => part)
    .join(' · ');
}

export function storyGuestsLabel(
  context: EditContext,
  progress: number,
): string {
  const guests =
    context.storyChoices?.find((option) => option.progress === progress)
      ?.guests ?? [];
  return guests.length > 0 ? guests.join(', ') : 'No guests';
}

export function storyPartyLabel(
  context: EditContext,
  progress: number,
): string | null {
  const choice = context.storyChoices?.find(
    (option) => option.progress === progress,
  );
  const parts = [
    choice?.joins?.length ? `Joins: ${choice.joins.join(', ')}` : null,
    choice?.leaves?.length ? `Leaves: ${choice.leaves.join(', ')}` : null,
  ].filter((part) => part !== null);
  return parts.length > 0 ? parts.join(' · ') : null;
}

export function calendarLabel(date: { month: number; day: number }): string {
  return `Month ${String(date.month)}, day ${String(date.day)}`;
}

/** The game's "Awarded for …" description as a short sentence. */
export function achievementLabel(description: string): string {
  const text = description.replace(/^Awarded for /, '');
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function collectExtras(
  context: EditContext,
  draft: EditorDraft,
  review: DraftReview,
) {
  const saved = context.calendar;
  const date = draft.calendar;
  if (saved && date && (date.month !== saved.month || date.day !== saved.day)) {
    const length = saved.monthLengths[date.month - 1];
    if (length === undefined || date.day < 1 || date.day > length) {
      review.errors.push('Choose a valid in-game date.');
    } else {
      review.operations.push({
        kind: 'calendar_date',
        month: date.month,
        day: date.day,
      });
      add(review, 'Game', 'Date', calendarLabel(saved), calendarLabel(date));
    }
  }
  for (const achievement of context.achievements ?? []) {
    const unlocked = draft.achievements?.[achievement.index];
    if (unlocked !== undefined && unlocked !== achievement.unlocked) {
      review.operations.push({
        kind: 'achievement',
        index: achievement.index,
        unlocked,
      });
      add(
        review,
        'Achievements',
        achievementLabel(achievement.description),
        achievement.unlocked ? 'Unlocked' : 'Locked',
        unlocked ? 'Unlocked' : 'Locked',
      );
    }
  }
}

export function collectDraft(
  reader: ReaderDocument | null,
  context: EditContext,
  draft: EditorDraft,
  projected?: ReaderDocument | null,
): DraftReview {
  const review: DraftReview = {
    operations: [],
    changes: [],
    errors: [],
    heldAfter: {},
  };
  if (draft.gil !== String(context.gil)) {
    if (!/^\d{1,10}$/.test(draft.gil) || Number(draft.gil) > 4_294_967_295) {
      review.errors.push('Gil must be a whole number from 0 to 4,294,967,295.');
    } else {
      review.operations.push({ kind: 'gil', value: draft.gil });
      add(review, 'Game', 'Gil', context.gil, draft.gil);
    }
  }
  if (
    draft.storyStep !== undefined &&
    context.storyStep !== undefined &&
    draft.storyStep !== String(context.storyStep)
  ) {
    const choice = context.storyChoices?.find(
      (option) => String(option.progress) === draft.storyStep,
    );
    if (!choice) {
      review.errors.push('Choose a story step from the list.');
    } else {
      review.operations.push({ kind: 'story_step', progress: draft.storyStep });
      add(
        review,
        'Game',
        'Story step',
        storyStepLabel(context, context.storyStep),
        storyStepLabel(context, choice.progress),
      );
      add(
        review,
        'Game',
        'Guests',
        'Current guests',
        storyGuestsLabel(context, choice.progress),
      );
      const party = storyPartyLabel(context, choice.progress);
      if (party) {
        add(review, 'Game', 'Party', 'Current party', party);
      }
    }
  }
  collectExtras(context, draft, review);

  const holdings = reader?.inventory.value;
  const originalHeld = new Map<number, { quantity: number; label: string }>();
  if (holdings?.state === 'known') {
    for (const holding of holdings.value) {
      const quantity = holding.quantity.value;
      const item = holding.item.value;
      if (
        quantity.state === 'known' &&
        item.state === 'known' &&
        item.value.id === `item:${String(holding.key)}` &&
        item.value.label.state === 'known'
      ) {
        originalHeld.set(holding.key, {
          quantity: quantity.value,
          label: item.value.label.value,
        });
      }
    }
  }
  const finalHeld = new Map<number, number>(
    [...originalHeld].map(([id, entry]) => [id, entry.quantity]),
  );
  const touchedHeld = new Set<number>();
  for (const [key, value] of Object.entries(draft.inventory)) {
    const position = Number(key);
    const holding =
      holdings?.state === 'known'
        ? holdings.value.find((row) => row.key === position)
        : undefined;
    const original = holding?.quantity.value;
    const item = holding?.item.value;
    if (
      original?.state !== 'known' ||
      item?.state !== 'known' ||
      item.value.id !== `item:${key}` ||
      item.value.label.state !== 'known'
    ) {
      review.errors.push(`Inventory position ${key} is unavailable.`);
      continue;
    }
    if (value === String(original.value)) continue;
    if (!/^(?:\d|[1-9]\d)$/.test(value)) {
      review.errors.push(
        `${item.value.label.value} needs a quantity from 0 to 99.`,
      );
      continue;
    }
    review.operations.push({
      kind: 'inventory_quantity',
      itemPosition: position,
      quantity: value,
    });
    finalHeld.set(position, Number(value));
    touchedHeld.add(position);
  }

  const roster = reader?.roster.value;
  for (const [positionText, unitDraft] of Object.entries(draft.units)) {
    const position = Number(positionText);
    const unit =
      roster?.state === 'known'
        ? roster.value.find((row) => row.key === position)
        : undefined;
    if (!unit) {
      review.errors.push(`Party member ${positionText} is unavailable.`);
      continue;
    }
    const group = unitLabel(unit);
    for (const [field, label, value, original, minimum] of [
      ['character_level', 'Level', unitDraft.level, unit.saved.level, 1],
      ['experience', 'EXP', unitDraft.exp, unit.saved.exp, 0],
    ] as const) {
      if (value === undefined || value === String(original)) continue;
      if (
        !/^\d{1,2}$/.test(value) ||
        Number(value) < minimum ||
        Number(value) > 99
      ) {
        review.errors.push(
          `${group}: ${label} must be from ${String(minimum)} to 99.`,
        );
      } else {
        review.operations.push({ kind: field, unitPosition: position, value });
        add(review, group, label, original, value);
      }
    }
    const projectedRoster = projected?.roster.value;
    const projectedUnit =
      projectedRoster?.state === 'known'
        ? projectedRoster.value.find((row) => row.key === position)
        : undefined;
    for (const [stat, label] of statFields) {
      const base = unitDraft.statBases?.[stat];
      const originalBase = unit.stored.bases[stat].value;
      if (
        base === undefined ||
        (originalBase.state === 'known' && base === originalBase.value)
      )
        continue;
      if (
        !Number.isSafeInteger(base) ||
        base < 0 ||
        base > 0xffffff ||
        originalBase.state !== 'known'
      ) {
        review.errors.push(`${group}: ${label} edit is unavailable.`);
        continue;
      }
      review.operations.push({
        kind: 'base_stat',
        unitPosition: position,
        stat,
        value: base,
      });
      const before = unit.effective[stat].value;
      const after = projectedUnit?.effective[stat].value;
      add(
        review,
        group,
        label,
        before.state === 'known' ? before.value : 'Unknown',
        after?.state === 'known'
          ? after.value
          : after
            ? 'Unknown'
            : 'Updating…',
      );
    }
    const gearOptions = context.gearOptions?.find(
      (option) => option.unitPosition === position,
    );
    for (const [slotKey, choice] of Object.entries(unitDraft.gear)) {
      const slot = gearOptions?.slots.find((entry) => entry.slot === slotKey);
      if (
        !slot ||
        (choice.itemKey !== '' &&
          !slot.choices.some((entry) => entry.key === choice.itemKey)) ||
        (choice.itemKey === ''
          ? choice.source !== null
          : choice.source === null)
      ) {
        review.errors.push(
          `${group}: ${slotKey.replace('_', ' ')} equipment choice is unavailable.`,
        );
        continue;
      }
      if (choice.itemKey === slot.currentKey) continue;
      const label = (key: string) =>
        key === ''
          ? 'Empty'
          : (slot.choices.find((entry) => entry.key === key)?.label ?? key);
      const slotLabel = slotKey.replace('_', ' ');
      review.operations.push({
        kind: 'gear',
        unitPosition: position,
        slot: slot.slot,
        itemKey: choice.itemKey,
        source: choice.source,
      });
      add(
        review,
        group,
        slotLabel,
        label(slot.currentKey),
        `${label(choice.itemKey)}${choice.source === 'create_and_equip' ? ' (Create and equip)' : ''}`,
      );
      for (const [key, delta] of [
        [slot.currentKey, 1],
        [choice.itemKey, choice.source === 'held' ? -1 : 1],
      ] as const) {
        if (key === '') continue;
        const match = /^item:(\d+)$/.exec(key);
        const id = match ? Number(match[1]) : NaN;
        if (!originalHeld.has(id)) {
          review.errors.push(
            `${group}: inventory for ${label(key)} is unavailable.`,
          );
          continue;
        }
        finalHeld.set(id, (finalHeld.get(id) ?? 0) + delta);
        touchedHeld.add(id);
      }
    }
    const zodiac = context.zodiacOptions?.find(
      (option) => option.unitPosition === position,
    );
    if (
      unitDraft.zodiac !== undefined &&
      unitDraft.zodiac !== zodiac?.current
    ) {
      const choice = zodiac?.choices.find(
        (option) => option.key === unitDraft.zodiac,
      );
      const original = zodiac?.choices.find(
        (option) => option.key === zodiac.current,
      );
      if (!choice || !original)
        review.errors.push(`${group}: Zodiac editing is unavailable.`);
      else {
        review.operations.push({
          kind: 'zodiac',
          unitPosition: position,
          sign: choice.key,
        });
        add(review, group, 'Zodiac', original.label, choice.label);
      }
    }
    const sex = context.sexOptions?.find(
      (option) => option.unitPosition === position,
    );
    if (unitDraft.sex !== undefined && unitDraft.sex !== sex?.current) {
      const choice = sex?.choices.find(
        (option) => option.key === unitDraft.sex,
      );
      const original = sex?.choices.find(
        (option) => option.key === sex.current,
      );
      if (!choice || !original)
        review.errors.push(`${group}: Sex editing is unavailable.`);
      else {
        review.operations.push({
          kind: 'sex',
          unitPosition: position,
          sex: choice.key,
        });
        add(review, group, 'Sex', original.label, choice.label);
      }
    }
    for (const [field, value, original] of [
      ['bravery', unitDraft.bravery, unit.saved.start_bcp],
      ['faith', unitDraft.faith, unit.saved.start_faith],
    ] as const) {
      if (value === undefined || value === String(original)) continue;
      if (!/^\d{1,3}$/.test(value) || Number(value) > 100) {
        review.errors.push(`${group}: ${field} must be from 0 to 100.`);
        continue;
      }
      review.operations.push({ kind: field, unitPosition: position, value });
      add(
        review,
        group,
        field === 'bravery' ? 'Bravery' : 'Faith',
        original,
        value,
      );
    }

    for (const [jobSlotText, values] of Object.entries(unitDraft.jobs)) {
      const jobSlot = Number(jobSlotText);
      const jobs = unit.jobs.value;
      const job =
        jobs.state === 'known'
          ? jobs.value.find((row) => row.slot === jobSlot)
          : undefined;
      const allowed = context.jobOptions?.some(
        (row) =>
          row.unitPosition === position && row.jobSlots.includes(jobSlot),
      );
      const level = job?.level.value;
      const current = job?.current_jp.value;
      const total = job?.total_jp.value;
      if (
        !allowed ||
        level?.state !== 'known' ||
        current?.state !== 'known' ||
        total?.state !== 'known'
      ) {
        review.errors.push(
          `${group}: ${jobLabel(unit, jobSlot)} progress is unavailable.`,
        );
        continue;
      }
      if (
        values.level === String(level.value) &&
        values.currentJp === String(current.value) &&
        values.totalJp === String(total.value)
      )
        continue;
      const nextLevel = Number(values.level);
      const nextTotal = Number(values.totalJp);
      const thresholds = context.jobLevelTotalJp;
      const minimum = thresholds?.[nextLevel];
      const maximum = thresholds?.[nextLevel + 1];
      if (
        !/^\d$/.test(values.level) ||
        nextLevel > 8 ||
        !/^\d{1,5}$/.test(values.currentJp) ||
        Number(values.currentJp) > 65535 ||
        !/^\d{1,5}$/.test(values.totalJp) ||
        nextTotal > 65535 ||
        minimum === undefined ||
        nextTotal < minimum ||
        (nextLevel < 8 && (maximum === undefined || nextTotal >= maximum))
      ) {
        review.errors.push(
          `${group}: ${jobLabel(unit, jobSlot)} needs a valid level, JP and Job EXP.`,
        );
        continue;
      }
      review.operations.push({
        kind: 'job_progress',
        unitPosition: position,
        jobSlot,
        level: values.level,
        currentJp: values.currentJp,
        totalJp: values.totalJp,
      });
      if (values.level !== String(level.value)) {
        add(
          review,
          group,
          `${jobLabel(unit, jobSlot)} level`,
          level.value,
          values.level,
        );
      }
      if (values.currentJp !== String(current.value)) {
        add(
          review,
          group,
          `${jobLabel(unit, jobSlot)} spendable JP`,
          current.value,
          values.currentJp,
        );
      }
      if (values.totalJp !== String(total.value)) {
        add(
          review,
          group,
          `${jobLabel(unit, jobSlot)} Job EXP`,
          total.value,
          values.totalJp,
        );
      }
    }

    const abilityOptions =
      context.abilityOptions?.filter((row) => row.unitPosition === position) ??
      [];
    for (const option of abilityOptions) {
      for (const ability of option.members) {
        const value =
          unitDraft.abilities[abilityDraftKey(option.jobSlot, ability.key)];
        if (value === undefined || value === ability.learned) continue;
        review.operations.push({
          kind: 'learned_ability',
          unitPosition: position,
          jobSlot: option.jobSlot,
          abilityKey: ability.key,
          learned: value,
        });
        add(
          review,
          group,
          `${jobLabel(unit, option.jobSlot)} · ${ability.label}`,
          ability.learned ? 'Learned' : 'Unlearned',
          value ? 'Learned' : 'Unlearned',
        );
      }
    }

    const loadout = context.loadoutOptions?.find(
      (row) => row.unitPosition === position,
    );
    if (loadout) {
      for (const slot of loadout.slots) {
        const key = loadoutDraftKey(slot.combatSet, slot.slot);
        const value = unitDraft.loadout[key];
        if (value === undefined || value === slot.currentKey) continue;
        const choices =
          slot.slot === 'secondary_command'
            ? loadout.secondaryChoices
            : slot.slot === 'reaction'
              ? loadout.reactionChoices
              : slot.slot === 'support'
                ? loadout.supportChoices
                : loadout.movementChoices;
        const label = (choice: string) =>
          choice === ''
            ? 'None'
            : (choices.find((row) => row.key === choice)?.label ??
              'Saved selection (unverified)');
        review.operations.push({
          kind: 'equipped_slot',
          unitPosition: position,
          combatSet: slot.combatSet,
          slot: slot.slot,
          valueKey: value,
        });
        add(
          review,
          group,
          `${slot.combatSet === null ? 'Active' : `Set ${String(slot.combatSet + 1)}`} ${slot.slot.replace('_', ' ')}`,
          label(slot.currentKey),
          label(value),
        );
      }
      for (const option of abilityOptions) {
        for (const ability of option.members) {
          const learned =
            unitDraft.abilities[abilityDraftKey(option.jobSlot, ability.key)] ??
            ability.learned;
          if (
            !learned &&
            loadout.slots.some(
              (slot) =>
                (unitDraft.loadout[
                  loadoutDraftKey(slot.combatSet, slot.slot)
                ] ?? slot.currentKey) === ability.key,
            )
          ) {
            review.errors.push(
              `${group}: replace or unequip ${ability.label} before unlearning it.`,
            );
          }
        }
      }
    }
  }
  for (const id of touchedHeld) {
    const original = originalHeld.get(id);
    const final = finalHeld.get(id);
    if (!original || final === undefined) continue;
    if (final < 0 || final > 99) {
      review.errors.push(
        `${original.label}: final held quantity must be from 0 to 99.`,
      );
    } else {
      review.heldAfter[id] = final;
      if (final !== original.quantity) {
        add(review, 'Held inventory', original.label, original.quantity, final);
      }
    }
  }
  const generic = draft.generic;
  if (generic) {
    const creation = context.genericCreation;
    const donor = creation.donors.find(
      (option) =>
        option.sourceSlot === generic.sourceSlot &&
        option.sourcePosition === generic.sourcePosition &&
        option.unitPosition === creation.targetPosition,
    );
    if (creation.targetPosition === null || !donor) {
      review.errors.push(
        'This save has no compatible generic template or open party position.',
      );
    } else if (
      !/^[\x20-\x7e]{1,15}$/.test(generic.name) ||
      generic.name.trim() === ''
    ) {
      review.errors.push('Name must be 1–15 printable ASCII characters.');
    } else {
      review.operations.unshift({
        kind: 'create_generic_from_slot',
        sourceSlot: donor.sourceSlot,
        sourcePosition: donor.sourcePosition,
        unitPosition: donor.unitPosition,
        name: generic.name,
      });
      review.changes.unshift({
        group: 'Party',
        label: 'Added',
        before: '',
        after: '',
        summary: `${generic.name} added`,
      });
    }
  }
  if (draft.creature) {
    const addition = context.creatureAddition;
    const choice = [
      ...(addition?.monsters ?? []),
      ...(addition?.enemies ?? []),
    ].find((option) => option.key === draft.creature?.formKey);
    const name = draft.creature.name;
    if (addition?.targetPosition == null || !choice?.available) {
      review.errors.push('This creature cannot be added to the current party.');
    } else if (
      name.length > 15 ||
      (name.length > 0 && (!/^[ -~]+$/.test(name) || !name.trim()))
    ) {
      review.errors.push(
        'Use up to 15 plain English letters, numbers or punctuation for the creature name.',
      );
    } else {
      review.operations.unshift({
        kind: 'create_creature',
        formKey: choice.key,
        unitPosition: addition.targetPosition,
        name,
      });
      review.changes.unshift({
        group: 'Party',
        label: 'Added',
        before: '',
        after: '',
        summary: `${name || choice.label} added`,
      });
    }
  }
  const story = draft.story;
  const named = draft.named;
  if (named) {
    const addition = context.namedAddition;
    const donor = addition?.donors.find(
      (option) =>
        option.sourceSlot === named.sourceSlot &&
        option.sourcePosition === named.sourcePosition &&
        option.unitPosition === addition.targetPosition,
    );
    const character = addition?.characters.find(
      (option) => option.key === named.characterKey,
    );
    if (
      addition?.targetPosition === null ||
      !donor ||
      !character ||
      !character.sex ||
      donor.sex !== character.sex
    ) {
      review.errors.push(
        'This save has no compatible base record or open party position.',
      );
    } else if (!character.available) {
      review.errors.push(
        character.unavailableReason
          ? `${character.label}: ${character.unavailableReason}.`
          : 'This exact character variant is already in the permanent party.',
      );
    } else {
      review.operations.unshift({
        kind: 'add_named_from_unit',
        sourceSlot: donor.sourceSlot,
        sourcePosition: donor.sourcePosition,
        unitPosition: donor.unitPosition,
        characterKey: character.key,
      });
      review.changes.unshift({
        group: 'Party',
        label: 'Added',
        before: '',
        after: '',
        summary: `${character.label.replace(/ · variant \d+$/, '')} added`,
      });
    }
  }
  if (story) {
    const addition = context.storyAddition;
    const donor = addition.donors.find(
      (option) =>
        option.sourceSlot === story.sourceSlot &&
        option.sourcePosition === story.sourcePosition &&
        option.unitPosition === addition.targetPosition,
    );
    if (addition.targetPosition === null || !donor) {
      review.errors.push(
        'This save has no compatible story template or open party position.',
      );
    } else if (review.operations.length > 0) {
      review.errors.push(
        'Save the other staged edits before adding a character.',
      );
    } else {
      review.operations.push({
        kind: 'add_story_from_slot',
        sourceSlot: donor.sourceSlot,
        sourcePosition: donor.sourcePosition,
        unitPosition: donor.unitPosition,
      });
      add(
        review,
        'Story addition',
        `Party position ${String(donor.unitPosition + 1)}`,
        'Available',
        donor.label,
      );
      add(
        review,
        'Story addition',
        'Equipment and progress',
        'None',
        `Copied from save slot ${String(donor.sourceSlot + 1)}, member ${String(donor.sourcePosition + 1)}`,
      );
      add(
        review,
        'Story addition',
        'Story compatibility',
        'Normal recruitment',
        'Experimental; story events are unchanged',
      );
    }
  }
  const guest = draft.guest;
  if (guest) {
    const addition = context.guestAddition;
    const donor = addition.donors.find(
      (option) =>
        option.sourceSlot === guest.sourceSlot &&
        option.sourcePosition === guest.sourcePosition &&
        option.unitPosition === addition.targetPosition,
    );
    if (addition.targetPosition === null || !donor) {
      review.errors.push(
        'This save has no compatible guest template or open party position.',
      );
    } else if (review.operations.length > 0) {
      review.errors.push(
        'Save the other staged edits before adding a character.',
      );
    } else {
      review.operations.push({
        kind: 'add_guest_from_slot',
        sourceSlot: donor.sourceSlot,
        sourcePosition: donor.sourcePosition,
        unitPosition: donor.unitPosition,
      });
      add(
        review,
        'Guest addition',
        `Party position ${String(donor.unitPosition + 1)}`,
        'Available',
        donor.label,
      );
      add(
        review,
        'Guest addition',
        'Equipment and progress',
        'None',
        `Copied from save slot ${String(donor.sourceSlot + 1)}, guest ${String(donor.sourcePosition - 49)}`,
      );
      add(
        review,
        'Guest addition',
        'Story compatibility',
        'None',
        'Experimental; story events are unchanged',
      );
    }
  }
  const additions = draft.additions ?? [];
  review.operations.unshift(...additions.map((addition) => addition.operation));
  review.changes.unshift(
    ...additions.map((addition) => ({
      group: 'Party',
      label: 'Added',
      before: '',
      after: '',
      summary: `${addition.label} added`,
    })),
  );
  return review;
}
