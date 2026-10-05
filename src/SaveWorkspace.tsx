import { useEffect, useRef, useState } from 'react';
import type { ValueState } from './ipc';
import {
  previewCharacter,
  previewJobProgress,
  restoreLastBackup,
  saveTransaction,
  type BaseStatKind,
  type EditContext,
  type ReaderDocument,
  type ReaderUnit,
  type GearSlot,
  type GearSource,
  type SlotSummary,
} from './reader-ipc';
import {
  abilityArt,
  characterSpriteArt,
  itemArt,
  jobSpriteArt,
  monsterSpriteArt,
  portraitArt,
  useReaderArt,
  type ReaderArt,
} from './reader-art';
import {
  ArtImage,
  CatalogueDescription,
  jobName,
  unitName,
} from './ReaderRoster';
import {
  abilityDraftKey,
  collectDraft,
  emptyUnitDraft,
  loadoutDraftKey,
  type EditorDraft,
  type JobDraft,
  type PendingAddition,
  type UnitDraft,
} from './editor-draft';

import { AddCharacterForm } from './AddCharacterForm';
import { WorkspaceGame } from './WorkspaceGame';
import { WorkspaceStatus } from './WorkspaceStatus';
import { useDraftProjection } from './use-draft-projection';
import { EquippedGear, description } from './WorkspaceEquipment';
import { InfoHint, Tooltip } from './Tooltip';
import { WorkspaceQuests } from './WorkspaceQuests';
import { WorkspaceUtilities } from './WorkspaceUtilities';
import { NumericField, stagedNumber, valueText } from './workspace-fields';
import type {
  WorkspaceSection,
  UnitPanel,
  AbilityKind,
  WorkspaceView,
} from './workspace-view';

interface Props {
  reader: ReaderDocument | null;
  context: EditContext | null;
  onDirtyChange: (dirty: boolean) => void;
  onReload: (slot: number, message: string) => Promise<void>;
  manualSlotId: number;
  occupiedSlots?: number[];
  slotSummaries?: SlotSummary[];
  initialView: WorkspaceView | undefined;
  /** Section shown when this slot has no remembered view. */
  defaultSection?: WorkspaceSection;
  onViewChange: (slot: number, view: WorkspaceView) => void;
}

function namedInventory(reader: ReaderDocument | null) {
  const inventory = reader?.inventory.value;
  if (inventory?.state !== 'known') return [];
  return inventory.value
    .flatMap((holding) => {
      const item = holding.item.value;
      const quantity = holding.quantity.value;
      if (
        item.state !== 'known' ||
        item.value.label.state !== 'known' ||
        quantity.state !== 'known' ||
        item.value.id !== `item:${String(holding.key)}` ||
        [0, 254, 255].includes(holding.key)
      )
        return [];
      return [
        {
          key: holding.key,
          label: item.value.label.value,
          description:
            item.value.description.state === 'known'
              ? item.value.description.value.replace(/<\/?color(?:=\d+)?>/g, '')
              : null,
          quantity: quantity.value,
          category:
            holding.category.value.state === 'known'
              ? holding.category.value.value
              : 'Other',
        },
      ];
    })
    .sort((a, b) => a.label.localeCompare(b.label));
}

function editableUnit(unit: ReaderUnit): boolean {
  return (
    unit.membership.value.state === 'known' &&
    unit.membership.value.value === 'party' &&
    (unit.kind.value.state !== 'known' ||
      unit.kind.value.value !== 'monster') &&
    unit.name.value.state === 'known'
  );
}

function editableProgression(unit: ReaderUnit): boolean {
  return (
    unit.key < 50 &&
    unit.membership.value.state === 'known' &&
    unit.membership.value.value === 'party'
  );
}

function unitSex(unit: ReaderUnit): 'male' | 'female' | null {
  const sex = unit.stored.sex.value;
  return sex.state === 'known' && sex.value === 'Male'
    ? 'male'
    : sex.state === 'known' && sex.value === 'Female'
      ? 'female'
      : null;
}

function unitAvatar(
  art: ReaderArt | null,
  unit: ReaderUnit,
): { src: string | null; variant: 'sprite' | 'portrait' } {
  if (
    unit.kind.value.state === 'known' &&
    unit.kind.value.value === 'monster'
  ) {
    const job = unit.current_job.value;
    const src =
      job.state === 'known' ? monsterSpriteArt(art, job.value.id) : null;
    return { src, variant: src ? 'sprite' : 'portrait' };
  }
  const identity = unit.portrait.value;
  if (identity.state === 'known') {
    const sprite = characterSpriteArt(art, identity.value);
    if (sprite) return { src: sprite, variant: 'sprite' };
  }
  const job = unit.current_job.value;
  const src =
    job.state === 'known'
      ? jobSpriteArt(art, job.value.id, unitSex(unit))
      : null;
  if (src) return { src, variant: 'sprite' };
  return {
    src: identity.state === 'known' ? portraitArt(art, identity.value) : null,
    variant: 'portrait',
  };
}

function jobTitle(unit: ReaderUnit, slot: number): string {
  const jobs = unit.jobs.value;
  const row =
    jobs.state === 'known'
      ? jobs.value.find((entry) => entry.slot === slot)
      : undefined;
  const value = row?.job.value;
  return value?.state === 'known'
    ? valueText(value.value.label)
    : 'Unknown job';
}

export function SaveWorkspace({
  reader: loadedReader,
  context: loadedContext,
  onDirtyChange,
  onReload,
  manualSlotId,
  occupiedSlots = [],
  slotSummaries = [],
  initialView,
  defaultSection = 'game',
  onViewChange,
}: Props) {
  const [additionPreview, setAdditionPreview] = useState<{
    reader: ReaderDocument;
    context: EditContext;
  } | null>(null);
  const reader = additionPreview?.reader ?? loadedReader;
  const context =
    additionPreview && loadedContext
      ? {
          ...loadedContext,
          ...additionPreview.context,
          genericCreation: {
            ...additionPreview.context.genericCreation,
            donors: additionPreview.context.genericCreation.donors.filter(
              (row) =>
                loadedContext.genericCreation.donors.some(
                  (original) =>
                    original.sourceSlot === row.sourceSlot &&
                    original.sourcePosition === row.sourcePosition,
                ),
            ),
          },
          ...(additionPreview.context.namedAddition
            ? {
                namedAddition: {
                  ...additionPreview.context.namedAddition,
                  donors: additionPreview.context.namedAddition.donors.filter(
                    (row) =>
                      loadedContext.namedAddition?.donors.some(
                        (original) =>
                          original.sourceSlot === row.sourceSlot &&
                          original.sourcePosition === row.sourcePosition,
                      ),
                  ),
                },
              }
            : {}),
        }
      : loadedContext;
  const namedChoices = [...(context?.namedAddition?.characters ?? [])].sort(
    (left, right) =>
      left.label.localeCompare(right.label, 'en', { numeric: true }),
  );
  const art = useReaderArt();
  const [section, setSection] = useState<WorkspaceSection>(
    initialView?.section ?? defaultSection,
  );
  const [panel, setPanel] = useState<UnitPanel>(initialView?.panel ?? 'status');
  const roster = reader?.roster.value;
  const units =
    roster?.state === 'known'
      ? roster.value.filter(
          (row) =>
            row.membership.value.state !== 'known' ||
            row.membership.value.value !== 'inactive',
        )
      : [];
  const [selectedUnit, setSelectedUnit] = useState<number | null>(
    initialView?.selectedUnit ?? units[0]?.key ?? null,
  );
  const [selectedJobs, setSelectedJobs] = useState<Record<number, number>>(
    initialView?.selectedJobs ?? {},
  );
  const [abilityKind, setAbilityKind] = useState<AbilityKind>(
    initialView?.abilityKind ?? 'action',
  );
  const [unitQuery, setUnitQuery] = useState(initialView?.unitQuery ?? '');
  const [itemQuery, setItemQuery] = useState(initialView?.itemQuery ?? '');
  const [itemCategory, setItemCategory] = useState(
    initialView?.itemCategory ?? 'All',
  );
  const [itemView, setItemView] = useState<'held' | 'all'>(
    initialView?.itemView ?? 'held',
  );
  const [selectedItem, setSelectedItem] = useState<number | null>(
    initialView?.selectedItem ?? null,
  );
  const [draft, setDraft] = useState<EditorDraft>({
    gil: context ? String(context.gil) : '',
    inventory: {},
    units: {},
    generic: null,
    story: null,
    guest: null,
    named: null,
  });
  const [reviewRequested, setReviewOpen] = useState(false);
  const [addCharacterOpen, setAddCharacterOpen] = useState(false);
  const [addCharacterType, setAddCharacterType] = useState<
    'generic' | 'named' | 'monster' | 'enemy'
  >('generic');
  const [addCharacterPrior, setAddCharacterPrior] = useState<
    Pick<EditorDraft, 'generic' | 'named' | 'creature'>
  >({ generic: null, named: null });
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [previewOptions, setPreviewOptions] = useState<
    Record<number, NonNullable<EditContext['jobOptions']>[number]>
  >({});
  const [jobWarning, setJobWarning] = useState<string | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  const previewVersion = useRef(0);
  const [commandInfo, setCommandInfo] = useState<{
    name: string;
    members: {
      label: string;
      learned: boolean;
      description: string | undefined;
    }[];
  } | null>(null);
  const reviewButton = useRef<HTMLButtonElement>(null);
  const confirmButton = useRef<HTMLButtonElement>(null);
  const inventoryRows = namedInventory(reader).map((row) => ({
    ...row,
    stagedQuantity: stagedNumber(draft.inventory[row.key], row.quantity),
  }));
  const heldForGear: Record<number, number> = Object.fromEntries(
    inventoryRows.map((row) => [row.key, row.stagedQuantity]),
  );
  const otherHoldings =
    reader?.inventory.value.state === 'known'
      ? reader.inventory.value.value.filter(
          (holding) =>
            holding.quantity.value.state === 'known' &&
            holding.quantity.value.value > 0 &&
            !inventoryRows.some((row) => row.key === holding.key),
        )
      : [];
  const operationReview = context
    ? collectDraft(
        reader,
        {
          ...context,
          jobOptions: (context.jobOptions ?? []).map(
            (option) => previewOptions[option.unitPosition] ?? option,
          ),
        },
        draft,
      )
    : null;
  function stageBase(position: number, stat: BaseStatKind, base: number) {
    const original = units.find((unit) => unit.key === position)?.stored.bases[
      stat
    ].value;
    setDraft((current) => {
      const unit = current.units[position] ?? emptyUnitDraft();
      return {
        ...current,
        units: {
          ...current.units,
          [position]: {
            ...unit,
            statBases: {
              ...Object.fromEntries(
                Object.entries(unit.statBases ?? {}).filter(
                  ([key]) => key !== stat,
                ),
              ),
              ...(original?.state === 'known' && original.value === base
                ? {}
                : { [stat]: base }),
            },
          },
        },
      };
    });
  }
  const statsPreview = useDraftProjection(
    context && operationReview
      ? {
          snapshotGeneration: context.snapshotGeneration,
          manualSlotId: context.manualSlotId,
          operations: operationReview.operations,
        }
      : null,
    stageBase,
    reader,
  );
  const review = context
    ? collectDraft(
        reader,
        {
          ...context,
          jobOptions: (context.jobOptions ?? []).map(
            (option) => previewOptions[option.unitPosition] ?? option,
          ),
        },
        draft,
        statsPreview.reader,
      )
    : null;
  const dirty =
    (review?.changes.length ?? 0) > 0 ||
    (review?.errors.length ?? 0) > 0 ||
    statsPreview.inputs.length > 0;
  const reviewOpen =
    reviewRequested &&
    !statsPreview.blocked &&
    (review?.errors.length ?? 0) === 0 &&
    (review?.operations.length ?? 0) > 0;
  Object.assign(heldForGear, review?.heldAfter ?? {});

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);
  useEffect(() => {
    if (reviewOpen) confirmButton.current?.focus();
  }, [reviewOpen]);

  useEffect(() => {
    onViewChange(manualSlotId, {
      section,
      panel,
      selectedUnit,
      selectedJobs,
      abilityKind,
      unitQuery,
      itemQuery,
      itemCategory,
      itemView,
      selectedItem,
    });
  }, [
    onViewChange,
    manualSlotId,
    section,
    panel,
    selectedUnit,
    selectedJobs,
    abilityKind,
    unitQuery,
    itemQuery,
    itemCategory,
    itemView,
    selectedItem,
  ]);

  const unit = units.find((row) => row.key === selectedUnit) ?? units[0];
  const selectedMonster =
    unit?.kind.value.state === 'known' && unit.kind.value.value === 'monster';
  const selectedAvatar = unit
    ? unitAvatar(art, unit)
    : { src: null, variant: 'portrait' as const };
  const visibleUnits = units.filter((row) =>
    `${unitName(row)} ${jobName(row)}`
      .toLocaleLowerCase()
      .includes(unitQuery.trim().toLocaleLowerCase()),
  );
  const unitDraft = unit
    ? (draft.units[unit.key] ?? emptyUnitDraft())
    : emptyUnitDraft();
  const activeGearOptions = unit
    ? context?.gearOptions?.find((row) => row.unitPosition === unit.key)
    : undefined;
  const jobs =
    unit?.jobs.value.state === 'known'
      ? unit.jobs.value.value.filter((row) => {
          const value = row.job.value;
          if (value.state !== 'known' || value.value.label.state !== 'known')
            return false;
          const sex =
            unitDraft.sex === 'male' || unitDraft.sex === 'female'
              ? unitDraft.sex
              : unitSex(unit);
          return !(
            (value.value.label.value === 'Bard' && sex === 'female') ||
            (value.value.label.value === 'Dancer' && sex === 'male')
          );
        })
      : [];
  const jobOptions =
    (unit ? previewOptions[unit.key] : undefined) ??
    context?.jobOptions?.find((row) => row.unitPosition === unit?.key);
  const allowedSlots = jobOptions?.jobSlots ?? [];
  const firstAllowed = jobs.find((row) => allowedSlots.includes(row.slot));
  const currentJob = unit?.current_job.value;
  const currentJobRow = jobs.find(
    (row) =>
      currentJob?.state === 'known' &&
      row.job.value.state === 'known' &&
      row.job.value.value.id === currentJob.value.id,
  );
  const jobSlot = unit
    ? (selectedJobs[unit.key] ??
      currentJobRow?.slot ??
      firstAllowed?.slot ??
      jobs[0]?.slot)
    : undefined;
  const job = jobs.find((row) => row.slot === jobSlot);
  const jobDraft = job ? unitDraft.jobs[job.slot] : undefined;
  const lockedJob = jobOptions?.lockedJobs?.find(
    (entry) => entry.jobSlot === jobSlot,
  );
  const abilityOptions = context?.abilityOptions?.find(
    (row) => row.unitPosition === unit?.key && row.jobSlot === jobSlot,
  );
  const loadout = context?.loadoutOptions?.find(
    (row) => row.unitPosition === unit?.key,
  );
  const categories = [
    ...new Set(inventoryRows.map((row) => row.category)),
  ].sort();
  const filteredItems = inventoryRows.filter(
    (row) =>
      (itemCategory === 'All' || row.category === itemCategory) &&
      (itemView === 'all' ||
        row.stagedQuantity > 0 ||
        (draft.inventory[row.key] !== undefined &&
          draft.inventory[row.key] !== String(row.quantity))) &&
      `${row.label} ${row.category}`
        .toLocaleLowerCase()
        .includes(itemQuery.trim().toLocaleLowerCase()),
  );
  const heldCount = inventoryRows.filter(
    (row) => row.stagedQuantity > 0,
  ).length;
  const item =
    filteredItems.find((row) => row.key === selectedItem) ?? filteredItems[0];

  function changeUnit(
    position: number,
    update: (current: UnitDraft) => UnitDraft,
  ) {
    setReviewOpen(false);
    setDraft((current) => ({
      ...current,
      units: {
        ...current.units,
        [position]: update(current.units[position] ?? emptyUnitDraft()),
      },
    }));
    setSaveError(null);
  }

  function chooseCharacterKind(
    kind: 'generic' | 'named' | 'monster' | 'enemy',
  ) {
    setAddCharacterType(kind);
    const genericDonor = context?.genericCreation.donors[0];
    const character = namedChoices.find((choice) => choice.available);
    const selectedCharacter =
      namedChoices.find(
        (choice) =>
          choice.key === draft.named?.characterKey && choice.available,
      ) ?? character;
    const namedDonor = context?.namedAddition?.donors.find(
      (donor) => donor.sex === selectedCharacter?.sex,
    );
    const creatures =
      kind === 'monster'
        ? context?.creatureAddition?.monsters
        : kind === 'enemy'
          ? context?.creatureAddition?.enemies
          : undefined;
    setDraft((current) => ({
      ...current,
      story: null,
      guest: null,
      creature: creatures?.some((choice) => choice.available)
        ? {
            formKey: creatures.find((choice) => choice.available)?.key ?? '',
            name: '',
          }
        : null,
      generic:
        kind === 'generic' && genericDonor
          ? {
              sourceSlot: genericDonor.sourceSlot,
              sourcePosition: genericDonor.sourcePosition,
              name: current.generic?.name ?? '',
            }
          : null,
      named:
        kind === 'named' && namedDonor && selectedCharacter
          ? {
              sourceSlot: namedDonor.sourceSlot,
              sourcePosition: namedDonor.sourcePosition,
              characterKey: selectedCharacter.key,
            }
          : null,
    }));
  }

  function discardAll() {
    setReviewOpen(false);
    ++previewVersion.current;
    setPreviewBusy(false);
    setDraft({
      gil: loadedContext ? String(loadedContext.gil) : '',
      inventory: {},
      units: {},
      generic: null,
      named: null,
      story: null,
      guest: null,
    });
    setAdditionPreview(null);
    statsPreview.discard();
    setPreviewOptions({});
    setJobWarning(null);
    setSaveError(null);
  }

  function cancelAddCharacter() {
    if (previewBusy) return;
    setDraft((current) => ({ ...current, ...addCharacterPrior }));
    setSaveError(null);
    setAddCharacterOpen(false);
  }

  async function changeJob(
    position: number,
    slot: number,
    initial: JobDraft,
    update: (current: JobDraft) => JobDraft,
  ) {
    const currentUnit = draft.units[position] ?? emptyUnitDraft();
    const before = currentUnit.jobs[slot] ?? initial;
    const next = update(before);
    const jobs = { ...currentUnit.jobs, [slot]: next };
    const version = ++previewVersion.current;
    if (next.level !== before.level && /^\d$/.test(next.level) && context) {
      setPreviewBusy(true);
      try {
        const projected = await previewJobProgress({
          snapshotGeneration: context.snapshotGeneration,
          manualSlotId: context.manualSlotId,
          unitPosition: position,
          levels: Object.entries(jobs)
            .filter(([, value]) => /^\d$/.test(value.level))
            .map(([key, value]) => ({
              jobSlot: Number(key),
              level: Number(value.level),
            })),
        });
        if (version !== previewVersion.current) return;
        if (projected.invalidatedSlots.length > 0) {
          setJobWarning(
            `This decrease would block ${projected.invalidatedSlots.map((key) => (unit ? jobTitle(unit, key) : 'another job')).join(', ')}. Keep the required job levels. The change was not staged.`,
          );
          return;
        }
        setPreviewOptions((current) => ({
          ...current,
          [position]: projected.jobOptions,
        }));
      } catch {
        if (version === previewVersion.current)
          setJobWarning(
            'Could not check job prerequisites. Reload the save before changing job levels. The change was not staged.',
          );
        return;
      } finally {
        if (version === previewVersion.current) setPreviewBusy(false);
      }
    }
    if (version !== previewVersion.current) return;
    setPreviewBusy(false);
    setJobWarning(null);
    changeUnit(position, (current) => ({ ...current, jobs }));
  }

  function showCommand(value: ValueState<import('./reader-ipc').CatalogueRef>) {
    if (value.state !== 'known' || !unit) return;
    showCommandKey(value.value.id, valueText(value.value.label));
  }

  function showCommandKey(key: string, name: string) {
    if (!unit) return;
    const option = context?.abilityOptions?.find(
      (row) => row.unitPosition === unit.key && row.commandKey === key,
    );
    setCommandInfo({
      name,
      members:
        option?.members
          .filter((row) => row.kind === 'action')
          .map((row) => ({
            label: row.label,
            description: context?.abilityDescriptions?.[row.key],
            learned:
              unitDraft.abilities[abilityDraftKey(option.jobSlot, row.key)] ??
              row.learned,
          })) ?? [],
    });
  }

  async function stageCharacter() {
    if (
      !context ||
      !review ||
      review.errors.length ||
      (!draft.generic && !draft.named && !draft.creature) ||
      previewBusy
    )
      return;
    const target = draft.creature
      ? context.creatureAddition?.targetPosition
      : draft.generic
        ? context.genericCreation.targetPosition
        : context.namedAddition?.targetPosition;
    if (target === null || target === undefined) return;
    const pending = review.operations.find(
      (operation) =>
        (operation.kind === 'create_generic_from_slot' ||
          operation.kind === 'add_named_from_unit' ||
          operation.kind === 'create_creature') &&
        operation.unitPosition === target,
    );
    if (
      !pending ||
      (pending.kind !== 'create_generic_from_slot' &&
        pending.kind !== 'add_named_from_unit' &&
        pending.kind !== 'create_creature')
    )
      return;
    const label =
      pending.kind === 'create_creature'
        ? pending.name ||
          [
            ...(context.creatureAddition?.monsters ?? []),
            ...(context.creatureAddition?.enemies ?? []),
          ].find((choice) => choice.key === pending.formKey)?.label ||
          'Creature'
        : pending.kind === 'create_generic_from_slot'
          ? pending.name
          : (namedChoices
              .find((choice) => choice.key === pending.characterKey)
              ?.label.replace(/ \u00b7 variant \d+$/, '') ?? 'Character');
    const additions = [
      ...(draft.additions ?? []),
      { operation: pending, label },
    ];
    await projectAdditions(additions, target);
  }

  async function projectAdditions(
    additions: PendingAddition[],
    selected: number | null,
    removePosition?: number,
  ) {
    if (!context) return;
    const version = ++previewVersion.current;
    setPreviewBusy(true);
    setSaveError(null);
    try {
      if (!additions.length) {
        setAdditionPreview(null);
      } else {
        const projected = await previewCharacter({
          snapshotGeneration: context.snapshotGeneration,
          manualSlotId: context.manualSlotId,
          operations: additions.map((addition) => addition.operation),
        });
        if (version !== previewVersion.current) return;
        if (!projected.reader || !projected.editContext)
          throw new Error('Character preview unavailable');
        setAdditionPreview({
          reader: projected.reader,
          context: projected.editContext,
        });
      }
      setDraft((current) => {
        const units = Object.fromEntries(
          Object.entries(current.units).filter(
            ([position]) => Number(position) !== removePosition,
          ),
        );
        return {
          ...current,
          additions,
          generic: null,
          named: null,
          creature: null,
          units,
        };
      });
      setSelectedUnit(selected);
      setUnitQuery('');
      setPanel('status');
      setPreviewOptions({});
      setAddCharacterOpen(false);
    } catch (error: unknown) {
      if (version !== previewVersion.current) return;
      const code =
        typeof error === 'object' && error !== null && 'code' in error
          ? String(error.code)
          : null;
      setSaveError(
        `Could not add the character. Reload the save and try again.${code ? ` Code: ${code}.` : ''}`,
      );
    } finally {
      if (version === previewVersion.current) setPreviewBusy(false);
    }
  }

  async function commit() {
    if (
      !context ||
      !review ||
      review.errors.length > 0 ||
      review.operations.length === 0 ||
      busy ||
      statsPreview.blocked
    )
      return;
    setBusy(true);
    setSaveError(null);
    try {
      const result = await saveTransaction({
        snapshotGeneration: context.snapshotGeneration,
        manualSlotId: context.manualSlotId,
        operations: review.operations,
      });
      setReviewOpen(false);
      await onReload(
        context.manualSlotId,
        result.backupCreated
          ? `Changes saved to slot ${String(context.manualSlotId + 1)}. A backup was created beside the save.`
          : 'No changes to save.',
      );
    } catch (error: unknown) {
      const code =
        typeof error === 'object' && error !== null && 'code' in error
          ? String(error.code)
          : null;
      setSaveError(
        code === 'save_recovery_required'
          ? 'Could not finish saving. The original save is in the file with " - backup" in its name beside the selected save. Recover that file before trying again.'
          : code === 'equipment_stock_exhausted'
            ? 'Not enough held copies remain for the staged equipment changes.'
            : code === 'equipment_stock_full'
              ? 'The staged equipment changes would put a held item above 99.'
              : code?.startsWith('equipment_')
                ? 'This equipment change is unavailable for the selected job, slot or hand combination. Reload the save and review the choices.'
                : `Could not save changes. The file may have changed; check the selected slot before trying again.${code ? ` Code: ${code}.` : ''}`,
      );
      setReviewOpen(false);
    } finally {
      setBusy(false);
    }
  }

  async function restore() {
    if (!context?.backupAvailable || busy) return;
    setBusy(true);
    setSaveError(null);
    try {
      await restoreLastBackup({
        snapshotGeneration: context.snapshotGeneration,
        manualSlotId: context.manualSlotId,
      });
      setRestoreOpen(false);
      await onReload(context.manualSlotId, 'Previous save restored.');
    } catch (error: unknown) {
      const code =
        typeof error === 'object' && error !== null && 'code' in error
          ? String(error.code)
          : null;
      setSaveError(
        `Could not restore the previous save. Reload the slot and check the backup before trying again.${code ? ` Code: ${code}.` : ''}`,
      );
      setRestoreOpen(false);
    } finally {
      setBusy(false);
    }
  }

  function loadoutFields(combatSet: number | null) {
    if (!unit || !loadout) return <p>Ability editing is unavailable.</p>;
    const slots = loadout.slots.filter((row) => row.combatSet === combatSet);
    if (slots.length === 0) return <p>No ability slots in this set.</p>;
    return (
      <div className="workspace-field-grid">
        {slots.map((slot) => {
          const key = loadoutDraftKey(slot.combatSet, slot.slot);
          const choices =
            slot.slot === 'secondary_command'
              ? loadout.secondaryChoices
              : slot.slot === 'reaction'
                ? loadout.reactionChoices
                : slot.slot === 'support'
                  ? loadout.supportChoices
                  : loadout.movementChoices;
          const currentMissing =
            slot.currentKey !== '' &&
            !choices.some((row) => row.key === slot.currentKey);
          const fieldLabel = `${combatSet === null ? 'Active' : `Combat set ${String(combatSet + 1)}`} ${slot.slot === 'secondary_command' ? 'secondary command' : slot.slot}`;
          return (
            <div
              key={key}
              className="workspace-field"
              data-changed={
                (unitDraft.loadout[key] !== undefined &&
                  unitDraft.loadout[key] !== slot.currentKey) ||
                undefined
              }
            >
              <div className="workspace-field-label">
                <label htmlFor={`loadout-${String(unit.key)}-${key}`}>
                  {slot.slot === 'secondary_command'
                    ? 'Secondary command'
                    : slot.slot.charAt(0).toUpperCase() + slot.slot.slice(1)}
                </label>
                <InfoHint
                  label={fieldLabel}
                  text={
                    context?.abilityDescriptions?.[
                      unitDraft.loadout[key] ?? slot.currentKey
                    ] ??
                    ((unitDraft.loadout[key] ?? slot.currentKey) ===
                    slot.currentKey
                      ? description(unit.abilities[slot.slot])
                      : undefined)
                  }
                />
              </div>
              <select
                id={`loadout-${String(unit.key)}-${key}`}
                aria-label={fieldLabel}
                value={unitDraft.loadout[key] ?? slot.currentKey}
                onChange={(event) => {
                  changeUnit(unit.key, (current) => ({
                    ...current,
                    loadout: {
                      ...current.loadout,
                      [key]: event.target.value,
                    },
                  }));
                }}
              >
                <option value="">None</option>
                {currentMissing && (
                  <option value={slot.currentKey}>Unknown selection</option>
                )}
                {choices.map((choice) => (
                  <option key={choice.key} value={choice.key}>
                    {choice.label}
                  </option>
                ))}
              </select>
              {unitDraft.loadout[key] !== undefined &&
                unitDraft.loadout[key] !== slot.currentKey && (
                  <div className="workspace-saved-selection">
                    <button
                      type="button"
                      className="workspace-reset"
                      aria-label={`Reset ${combatSet === null ? 'Active' : `Combat set ${String(combatSet + 1)}`} ${slot.slot.replace('_', ' ')}`}
                      onClick={() => {
                        changeUnit(unit.key, (current) => ({
                          ...current,
                          loadout: {
                            ...current.loadout,
                            [key]: slot.currentKey,
                          },
                        }));
                      }}
                    >
                      Reset
                    </button>
                  </div>
                )}
              {slot.slot === 'secondary_command' &&
                (unitDraft.loadout[key] ?? slot.currentKey) !== '' && (
                  <button
                    type="button"
                    aria-label="View secondary actions"
                    onClick={() => {
                      const key =
                        unitDraft.loadout[
                          loadoutDraftKey(slot.combatSet, slot.slot)
                        ] ?? slot.currentKey;
                      showCommandKey(
                        key,
                        choices.find((choice) => choice.key === key)?.label ??
                          'Secondary command',
                      );
                    }}
                  >
                    Actions
                  </button>
                )}
            </div>
          );
        })}
      </div>
    );
  }

  return (
    <>
      <nav className="workspace-navigation" aria-label="Workspace sections">
        {(
          [
            ['game', 'Game'],
            ['quests', 'Quests'],
            ['units', 'Units'],
            ['inventory', 'Inventory'],
            ['utilities', 'Utilities'],
          ] as const
        ).map(([key, label]) => (
          <button
            key={key}
            type="button"
            aria-pressed={section === key}
            onClick={() => {
              setSection(key);
            }}
          >
            {label}
          </button>
        ))}
        {context?.backupAvailable && (
          <button
            type="button"
            className="workspace-backup"
            disabled={busy}
            onClick={() => {
              setRestoreOpen(true);
            }}
          >
            Restore file backup
          </button>
        )}
      </nav>

      <div className="workspace-content">
        {section === 'game' && (
          <WorkspaceGame
            reader={statsPreview.reader ?? reader}
            gil={draft.gil}
            savedGil={context?.gil ?? null}
            onGilChange={(gil) => {
              setDraft((current) => ({ ...current, gil }));
              setSaveError(null);
            }}
            context={context}
            storyStep={draft.storyStep}
            onStoryStepChange={(storyStep) => {
              setDraft((current) => ({ ...current, storyStep }));
              setSaveError(null);
            }}
            calendar={draft.calendar}
            onCalendarChange={(calendar) => {
              setDraft((current) => ({ ...current, calendar }));
              setSaveError(null);
            }}
            achievements={draft.achievements}
            onAchievementChange={(index, unlocked) => {
              setDraft((current) => ({
                ...current,
                achievements: { ...current.achievements, [index]: unlocked },
              }));
              setSaveError(null);
            }}
          />
        )}

        {section === 'quests' && <WorkspaceQuests context={context} />}

        {section === 'utilities' && (
          <WorkspaceUtilities
            context={loadedContext}
            occupied={occupiedSlots}
            summaries={slotSummaries}
            loadedSlot={manualSlotId}
            pendingChanges={dirty}
            onReload={onReload}
          />
        )}

        {section === 'inventory' && (
          <section
            className="workspace-inventory"
            aria-labelledby="workspace-inventory-heading"
          >
            <div className="workspace-page-heading">
              <div>
                <h2
                  className="visually-hidden"
                  id="workspace-inventory-heading"
                >
                  Inventory
                </h2>
              </div>
              <p>
                {heldCount} item {heldCount === 1 ? 'type' : 'types'} held
              </p>
            </div>
            {reader?.inventory.value.state !== 'known' ? (
              <p>Inventory is unavailable for this slot.</p>
            ) : (
              <>
                <div className="workspace-inventory-controls">
                  <label>
                    Search items
                    <input
                      type="search"
                      value={itemQuery}
                      onChange={(event) => {
                        setItemQuery(event.target.value);
                      }}
                    />
                  </label>
                  <label>
                    Category
                    <select
                      value={itemCategory}
                      onChange={(event) => {
                        setItemCategory(event.target.value);
                      }}
                    >
                      <option>All</option>
                      {categories.map((category) => (
                        <option key={category}>{category}</option>
                      ))}
                    </select>
                  </label>
                  <div className="workspace-filter" aria-label="Inventory view">
                    {(
                      [
                        ['held', 'Held'],
                        ['all', 'All items'],
                      ] as const
                    ).map(([key, label]) => (
                      <button
                        key={key}
                        type="button"
                        aria-pressed={itemView === key}
                        onClick={() => {
                          setItemView(key);
                        }}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="workspace-inventory-columns">
                  <ul className="workspace-item-list" aria-label="Items">
                    {filteredItems.map((row) => (
                      <li key={row.key}>
                        <button
                          type="button"
                          data-changed={
                            (draft.inventory[row.key] !== undefined &&
                              draft.inventory[row.key] !==
                                String(row.quantity)) ||
                            undefined
                          }
                          aria-pressed={item?.key === row.key}
                          onClick={() => {
                            setSelectedItem(row.key);
                          }}
                        >
                          <ArtImage
                            src={itemArt(art, `item:${String(row.key)}`)}
                            label={`${row.label} icon`}
                            variant="item"
                          />
                          <span>
                            {row.label}
                            <small>{row.category}</small>
                          </span>
                          <strong>
                            × {draft.inventory[row.key] ?? row.quantity}
                          </strong>
                        </button>
                      </li>
                    ))}
                    {filteredItems.length === 0 && <li>No matching items.</li>}
                  </ul>
                  <aside className="workspace-paper workspace-item-detail">
                    {item ? (
                      <>
                        <div className="workspace-item-title">
                          <ArtImage
                            src={itemArt(art, `item:${String(item.key)}`)}
                            label={`${item.label} icon`}
                            variant="item"
                          />
                          <div>
                            <h3>{item.label}</h3>
                            <p className="workspace-item-category">
                              {item.category}
                            </p>
                          </div>
                        </div>
                        {item.description && (
                          <details className="workspace-item-description">
                            <summary>Description</summary>
                            <p>{item.description}</p>
                          </details>
                        )}
                        <ul
                          className="workspace-item-effects"
                          aria-label="Item effects"
                        >
                          {reader.item_details?.[
                            `item:${String(item.key)}`
                          ]?.map((detail) => (
                            <li key={detail}>{detail}</li>
                          ))}
                        </ul>
                        {context ? (
                          <NumericField
                            label="Held quantity"
                            accessibleLabel={`Quantity for ${item.label}`}
                            id="item-quantity"
                            saved={item.quantity}
                            value={
                              draft.inventory[item.key] ?? String(item.quantity)
                            }
                            onChange={(value) => {
                              setDraft((current) => ({
                                ...current,
                                inventory: {
                                  ...current.inventory,
                                  [item.key]: value,
                                },
                              }));
                              setSaveError(null);
                            }}
                          />
                        ) : (
                          <dl className="workspace-facts">
                            <div>
                              <dt>Held quantity</dt>
                              <dd>{item.quantity}</dd>
                            </div>
                          </dl>
                        )}
                      </>
                    ) : null}
                    {otherHoldings.length > 0 && (
                      <details className="workspace-more-data">
                        <summary>
                          Other holdings ({otherHoldings.length})
                        </summary>
                        <ul>
                          {otherHoldings.map((holding) => (
                            <li key={holding.key}>
                              {valueText(holding.item.value, (item) =>
                                valueText(item.label),
                              )}{' '}
                              (position {holding.key}) ×{' '}
                              {valueText(holding.quantity.value)}
                            </li>
                          ))}
                        </ul>
                      </details>
                    )}
                  </aside>
                </div>
              </>
            )}
          </section>
        )}

        {section === 'units' && (
          <section
            className="workspace-units"
            aria-labelledby="workspace-units-heading"
          >
            <div className="workspace-page-heading">
              <div>
                <h2 className="visually-hidden" id="workspace-units-heading">
                  Units
                </h2>
              </div>
            </div>
            <div className="workspace-roster-controls">
              <label className="workspace-unit-search">
                Find a unit
                <input
                  type="search"
                  value={unitQuery}
                  onChange={(event) => {
                    setUnitQuery(event.target.value);
                  }}
                />
              </label>
              {context && (
                <button
                  type="button"
                  disabled={
                    busy ||
                    previewBusy ||
                    (context.genericCreation.targetPosition === null &&
                      (context.namedAddition?.targetPosition ?? null) === null)
                  }
                  title={
                    context.genericCreation.targetPosition === null &&
                    (context.namedAddition?.targetPosition ?? null) === null
                      ? 'The permanent party is full.'
                      : undefined
                  }
                  onClick={() => {
                    setAddCharacterPrior({
                      generic: draft.generic,
                      named: draft.named ?? null,
                      creature: draft.creature ?? null,
                    });
                    chooseCharacterKind(
                      context.genericCreation.donors.length
                        ? 'generic'
                        : 'named',
                    );
                    setAddCharacterOpen(true);
                  }}
                >
                  Add character
                </button>
              )}
            </div>
            {context?.genericCreation.targetPosition === null &&
              (context.namedAddition?.targetPosition ?? null) === null && (
                <p className="workspace-inline-note">
                  The permanent party is full.
                </p>
              )}
            {roster?.state !== 'known' ? (
              <p>Roster: {roster ? valueText(roster) : 'Unavailable'}</p>
            ) : units.length === 0 ? (
              <p>No party members in this slot.</p>
            ) : (
              <div className="workspace-unit-columns">
                <div className="workspace-roster" aria-label="Party members">
                  {visibleUnits.length === 0 && (
                    <div className="workspace-no-results">
                      <p>No matching units.</p>
                      <button
                        type="button"
                        onClick={() => {
                          setUnitQuery('');
                        }}
                      >
                        Clear search
                      </button>
                    </div>
                  )}
                  {visibleUnits.map((row, index) => {
                    const avatar = unitAvatar(art, row);
                    return (
                      <button
                        key={row.key}
                        type="button"
                        data-changed={
                          !!review?.changes.some(
                            (change) => change.group === unitName(row),
                          ) ||
                          draft.additions?.some(
                            (addition) =>
                              addition.operation.unitPosition === row.key,
                          ) ||
                          undefined
                        }
                        data-membership={
                          row.membership.value.state === 'known'
                            ? row.membership.value.value
                            : 'unknown'
                        }
                        aria-pressed={unit?.key === row.key}
                        aria-label={`Member ${String(index + 1)} ${unitName(row)} ${jobName(row)} Level ${draft.units[row.key]?.level ?? valueText(row.stored.level.value)}`}
                        onClick={() => {
                          setSelectedUnit(row.key);
                        }}
                        onKeyDown={(event) => {
                          const nextIndex =
                            event.key === 'ArrowDown'
                              ? Math.min(index + 1, visibleUnits.length - 1)
                              : event.key === 'ArrowUp'
                                ? Math.max(index - 1, 0)
                                : event.key === 'Home'
                                  ? 0
                                  : event.key === 'End'
                                    ? visibleUnits.length - 1
                                    : null;
                          if (nextIndex === null) return;
                          event.preventDefault();
                          const next = visibleUnits[nextIndex];
                          if (!next) return;
                          setSelectedUnit(next.key);
                          const rosterButtons =
                            event.currentTarget.parentElement?.querySelectorAll(
                              'button',
                            );
                          rosterButtons?.[nextIndex]?.focus();
                        }}
                      >
                        <ArtImage
                          src={avatar.src}
                          label={`${unitName(row)} avatar`}
                          variant={avatar.variant}

                          monster={
                            row.kind.value.state === 'known' &&
                            row.kind.value.value === 'monster'
                          }
                        />
                        <span>
                          <strong>{unitName(row)}</strong>
                          <small>
                            {jobName(row)} · Lv.{' '}
                            {draft.units[row.key]?.level ??
                              valueText(row.stored.level.value)}
                          </small>
                        </span>
                        {row.membership.value.state === 'known' &&
                          row.membership.value.value === 'guest' && (
                            <span className="workspace-membership">Guest</span>
                          )}
                        {context?.unitsOnErrands?.includes(row.key) && (
                          <span className="workspace-membership">
                            On an errand
                          </span>
                        )}
                      </button>
                    );
                  })}
                </div>
                <div className="workspace-unit-main">
                  {unit ? (
                    <>
                      <header className="workspace-unit-header">
                        {draft.additions?.some(
                          (addition) =>
                            addition.operation.unitPosition === unit.key,
                        ) && (
                          <button
                            type="button"
                            disabled={busy || previewBusy}
                            onClick={() => {
                              void projectAdditions(
                                (draft.additions ?? []).filter(
                                  (addition) =>
                                    addition.operation.unitPosition !==
                                    unit.key,
                                ),
                                loadedReader?.roster.value.state === 'known'
                                  ? (loadedReader.roster.value.value[0]?.key ??
                                      null)
                                  : null,
                                unit.key,
                              );
                            }}
                          >
                            Remove pending character
                          </button>
                        )}
                        <ArtImage
                          src={selectedAvatar.src}
                          label={`${unitName(unit)} avatar`}
                          variant={selectedAvatar.variant}
                          monster={selectedMonster}
                        />
                        <div>
                          <h3>{unitName(unit)}</h3>
                          <p>
                            {jobName(unit)} · Level{' '}
                            {unitDraft.level ??
                              valueText(unit.stored.level.value)}
                          </p>
                        </div>
                        {!editableUnit(unit) && !editableProgression(unit) && (
                          <span className="workspace-view-only">
                            {unit.membership.value.state === 'known' &&
                            unit.membership.value.value === 'guest'
                              ? 'Guest · View only'
                              : 'View only'}
                          </span>
                        )}
                      </header>
                      <nav
                        className="workspace-unit-tabs"
                        aria-label="Unit panels"
                      >
                        {(
                          [
                            ['status', 'Status'],
                            ['equipment', 'Equipment'],
                            ['jobs', 'Jobs'],
                          ] as const
                        ).map(([key, label]) => (
                          <button
                            key={key}
                            type="button"
                            aria-pressed={panel === key}
                            onClick={() => {
                              setPanel(key);
                            }}
                          >
                            {label}
                          </button>
                        ))}
                      </nav>
                      <div
                        key={panel}
                        className="workspace-unit-panel"
                        data-panel={panel}
                      >
                        {panel === 'status' && (
                          <WorkspaceStatus
                            unit={unit}
                            projected={
                              statsPreview.reader?.roster.value.state ===
                              'known'
                                ? (statsPreview.reader.roster.value.value.find(
                                    (row) => row.key === unit.key,
                                  ) ?? unit)
                                : unit
                            }
                            statInputs={statsPreview.inputs.filter(
                              (input) => input.position === unit.key,
                            )}
                            onStatChange={(stat, value) => {
                              setReviewOpen(false);
                              statsPreview.changeStat(unit.key, stat, value);
                            }}
                            onStatCommit={(stat) => {
                              statsPreview.commitStat(unit.key, stat);
                            }}
                            onStatReset={(stat) => {
                              setReviewOpen(false);
                              statsPreview.resetStat(unit.key, stat);
                            }}
                            projectionError={statsPreview.error}
                            projecting={statsPreview.busy}
                            draft={unitDraft}
                            editable={!!context && editableUnit(unit)}
                            progressionEditable={
                              !!context && editableProgression(unit)
                            }
                            zodiac={context?.zodiacOptions?.find(
                              (option) => option.unitPosition === unit.key,
                            )}
                            sex={context?.sexOptions?.find(
                              (option) => option.unitPosition === unit.key,
                            )}
                            onChange={(key, value) => {
                              changeUnit(unit.key, (current) => ({
                                ...current,
                                [key]: value,
                              }));
                            }}
                          />
                        )}

                        {panel === 'equipment' && (
                          <div className="workspace-equipment">
                            <div className="workspace-paper">
                              <div className="workspace-panel-heading">
                                <h3>Equipment</h3>
                                {!activeGearOptions && (
                                  <span className="workspace-view-only">
                                    View only
                                  </span>
                                )}
                              </div>
                              <EquippedGear
                                unit={unit}
                                art={art}
                                itemDetails={reader?.item_details ?? {}}
                                gearOptions={activeGearOptions}
                                staged={unitDraft.gear}
                                held={heldForGear}
                                onChange={
                                  activeGearOptions
                                    ? (
                                        slot: GearSlot,
                                        itemKey: string,
                                        source: GearSource | null,
                                      ) => {
                                        changeUnit(unit.key, (current) => {
                                          const gear = Object.fromEntries(
                                            Object.entries(current.gear).filter(
                                              ([key]) => key !== slot,
                                            ),
                                          );
                                          if (
                                            itemKey !==
                                            activeGearOptions.slots.find(
                                              (row) => row.slot === slot,
                                            )?.currentKey
                                          ) {
                                            gear[slot] = { itemKey, source };
                                          }
                                          return { ...current, gear };
                                        });
                                      }
                                    : undefined
                                }
                              />
                            </div>
                            <div className="workspace-paper">
                              <h3>Equipped abilities</h3>
                              <div className="workspace-primary-command">
                                <button
                                  type="button"
                                  onClick={() => {
                                    showCommand(
                                      unit.abilities.primary_command.value,
                                    );
                                  }}
                                >
                                  Primary command:{' '}
                                  {unit.abilities.primary_command.value
                                    .state === 'known'
                                    ? valueText(
                                        unit.abilities.primary_command.value
                                          .value.label,
                                      )
                                    : valueText(
                                        unit.abilities.primary_command.value,
                                      )}
                                </button>
                                <InfoHint
                                  label="Primary command"
                                  text={description(
                                    unit.abilities.primary_command,
                                  )}
                                />
                              </div>
                              {loadout ? (
                                loadoutFields(null)
                              ) : (
                                <dl className="workspace-facts">
                                  <div>
                                    <dt>Secondary command</dt>
                                    <dd>
                                      <button
                                        type="button"
                                        onClick={() => {
                                          showCommand(
                                            unit.abilities.secondary_command
                                              .value,
                                          );
                                        }}
                                      >
                                        {valueText(
                                          unit.abilities.secondary_command
                                            .value,
                                          (ability) => valueText(ability.label),
                                        )}
                                      </button>
                                      <InfoHint
                                        label="Secondary command"
                                        text={description(
                                          unit.abilities.secondary_command,
                                        )}
                                      />
                                    </dd>
                                  </div>
                                  {(
                                    [
                                      ['reaction', 'Reaction'],
                                      ['support', 'Support'],
                                      ['movement', 'Movement'],
                                    ] as const
                                  ).map(([key, label]) => (
                                    <div key={key}>
                                      <dt>{label}</dt>
                                      <dd>
                                        {valueText(
                                          unit.abilities[key].value,
                                          (ability) => valueText(ability.label),
                                        )}
                                        <InfoHint
                                          label={label}
                                          text={description(
                                            unit.abilities[key],
                                          )}
                                        />
                                      </dd>
                                    </div>
                                  ))}
                                </dl>
                              )}
                            </div>
                          </div>
                        )}

                        {panel === 'jobs' && (
                          <div className="workspace-jobs">
                            <div
                              className="workspace-job-list"
                              aria-label="Jobs"
                            >
                              {jobs.map((row) => {
                                const value = row.job.value;
                                const name = jobTitle(unit, row.slot);
                                const sprite =
                                  value.state === 'known'
                                    ? (jobSpriteArt(
                                        art,
                                        value.value.id,
                                        unitSex(unit),
                                      ) ??
                                      (row.slot === 0
                                        ? unitAvatar(art, unit).src
                                        : null))
                                    : null;
                                const locked = jobOptions?.lockedJobs?.find(
                                  (entry) => entry.jobSlot === row.slot,
                                );
                                return (
                                  <button
                                    key={row.slot}
                                    type="button"
                                    data-changed={
                                      (!!unitDraft.jobs[row.slot] &&
                                        (unitDraft.jobs[row.slot]?.level !==
                                          String(
                                            row.level.value.state === 'known'
                                              ? row.level.value.value
                                              : '',
                                          ) ||
                                          unitDraft.jobs[row.slot]
                                            ?.currentJp !==
                                            String(
                                              row.current_jp.value.state ===
                                                'known'
                                                ? row.current_jp.value.value
                                                : '',
                                            ) ||
                                          unitDraft.jobs[row.slot]?.totalJp !==
                                            String(
                                              row.total_jp.value.state ===
                                                'known'
                                                ? row.total_jp.value.value
                                                : '',
                                            ))) ||
                                      undefined
                                    }
                                    aria-pressed={row.slot === jobSlot}
                                    aria-disabled={locked ? 'true' : undefined}
                                    className={
                                      locked
                                        ? 'workspace-job-locked'
                                        : undefined
                                    }
                                    onClick={() => {
                                      setSelectedJobs((current) => ({
                                        ...current,
                                        [unit.key]: row.slot,
                                      }));
                                    }}
                                  >
                                    <ArtImage
                                      src={sprite}
                                      label={`${name} avatar`}
                                      variant={sprite ? 'sprite' : 'portrait'}
                                    />
                                    <span>
                                      <strong>{name}</strong>
                                      <small>
                                        Lv.{' '}
                                        {unitDraft.jobs[row.slot]?.level ??
                                          valueText(row.level.value)}{' '}
                                        · JP{' '}
                                        {unitDraft.jobs[row.slot]?.currentJp ??
                                          valueText(row.current_jp.value)}
                                      </small>
                                    </span>
                                  </button>
                                );
                              })}
                              {jobs.length === 0 && (
                                <p>Job progress is unavailable.</p>
                              )}
                            </div>
                            <div className="workspace-paper workspace-job-detail">
                              {jobWarning && <p role="alert">{jobWarning}</p>}
                              {previewBusy && (
                                <span
                                  className="app-spinner"
                                  role="status"
                                  aria-label="Checking job prerequisites"
                                />
                              )}
                              {job ? (
                                <>
                                  <h3>{jobTitle(unit, job.slot)}</h3>
                                  <CatalogueDescription
                                    fact={job.job}
                                    label="Job description"
                                  />
                                  {lockedJob && (
                                    <aside
                                      className="workspace-locked-requirements"
                                      aria-label="Job requirements"
                                      id="locked-job-heading"
                                    >
                                      <h4>Required job levels</h4>
                                      <ul className="workspace-requirements">
                                        {lockedJob.requires.map((required) => (
                                          <li key={required.jobSlot}>
                                            <span>
                                              {jobTitle(unit, required.jobSlot)}
                                            </span>
                                            <span>
                                              Level {required.level} · currently{' '}
                                              {required.currentLevel}
                                            </span>
                                          </li>
                                        ))}
                                      </ul>
                                    </aside>
                                  )}
                                  {allowedSlots.includes(job.slot) &&
                                  editableUnit(unit) &&
                                  context &&
                                  job.level.value.state === 'known' &&
                                  job.current_jp.value.state === 'known' &&
                                  job.total_jp.value.state === 'known' ? (
                                    <div className="workspace-field-grid">
                                      {(
                                        [
                                          [
                                            'level',
                                            'Job level',
                                            job.level.value.value,
                                          ],
                                          [
                                            'currentJp',
                                            'Spendable JP',
                                            job.current_jp.value.value,
                                          ],
                                          [
                                            'totalJp',
                                            'Job EXP',
                                            job.total_jp.value.value,
                                          ],
                                        ] as const
                                      ).map(([key, label, original]) => {
                                        const initial = {
                                          level: String(
                                            job.level.value.state === 'known'
                                              ? job.level.value.value
                                              : '',
                                          ),
                                          currentJp: String(
                                            job.current_jp.value.state ===
                                              'known'
                                              ? job.current_jp.value.value
                                              : '',
                                          ),
                                          totalJp: String(
                                            job.total_jp.value.state === 'known'
                                              ? job.total_jp.value.value
                                              : '',
                                          ),
                                        };
                                        return (
                                          <NumericField
                                            key={key}
                                            label={label}
                                            saved={original}
                                            disabled={previewBusy}
                                            value={
                                              jobDraft?.[key] ??
                                              String(original)
                                            }
                                            onChange={(value) => {
                                              void changeJob(
                                                unit.key,
                                                job.slot,
                                                initial,
                                                (current) => {
                                                  if (key !== 'level')
                                                    return {
                                                      ...current,
                                                      [key]: value,
                                                    };
                                                  if (
                                                    value === String(original)
                                                  )
                                                    return {
                                                      ...current,
                                                      level: value,
                                                      totalJp: initial.totalJp,
                                                    };
                                                  const minimum = /^\d$/.test(
                                                    value,
                                                  )
                                                    ? context.jobLevelTotalJp?.[
                                                        Number(value)
                                                      ]
                                                    : undefined;
                                                  return {
                                                    ...current,
                                                    level: value,
                                                    totalJp:
                                                      minimum === undefined
                                                        ? current.totalJp
                                                        : String(minimum),
                                                  };
                                                },
                                              );
                                            }}
                                          />
                                        );
                                      })}
                                    </div>
                                  ) : (
                                    <dl
                                      className="workspace-facts"
                                      aria-label="Job progress"
                                    >
                                      <div>
                                        <dt>Job level</dt>
                                        <dd>{valueText(job.level.value)}</dd>
                                      </div>
                                      <div>
                                        <dt>Spendable JP</dt>
                                        <dd>
                                          {valueText(job.current_jp.value)}
                                        </dd>
                                      </div>
                                      <div>
                                        <dt>Job EXP</dt>
                                        <dd>{valueText(job.total_jp.value)}</dd>
                                      </div>
                                    </dl>
                                  )}
                                  <section aria-label="Learned abilities">
                                    <h4>Abilities</h4>
                                    <div
                                      className="workspace-filter"
                                      aria-label="Ability category"
                                    >
                                      {(
                                        [
                                          'action',
                                          'reaction',
                                          'support',
                                          'movement',
                                        ] as const
                                      ).map((kind) => (
                                        <button
                                          key={kind}
                                          type="button"
                                          aria-pressed={abilityKind === kind}
                                          onClick={() => {
                                            setAbilityKind(kind);
                                          }}
                                        >
                                          {kind}
                                        </button>
                                      ))}
                                    </div>
                                    {abilityOptions ? (
                                      <ul className="workspace-ability-list">
                                        {abilityOptions.members
                                          .filter(
                                            (row) => row.kind === abilityKind,
                                          )
                                          .map((ability) => (
                                            <li
                                              key={ability.key}
                                              data-changed={
                                                (unitDraft.abilities[
                                                  abilityDraftKey(
                                                    abilityOptions.jobSlot,
                                                    ability.key,
                                                  )
                                                ] !== undefined &&
                                                  unitDraft.abilities[
                                                    abilityDraftKey(
                                                      abilityOptions.jobSlot,
                                                      ability.key,
                                                    )
                                                  ] !== ability.learned) ||
                                                undefined
                                              }
                                            >
                                              <label>
                                                <input
                                                  type="checkbox"
                                                  disabled={
                                                    !allowedSlots.includes(
                                                      job.slot,
                                                    )
                                                  }
                                                  checked={
                                                    unitDraft.abilities[
                                                      abilityDraftKey(
                                                        abilityOptions.jobSlot,
                                                        ability.key,
                                                      )
                                                    ] ?? ability.learned
                                                  }
                                                  onChange={(event) => {
                                                    changeUnit(
                                                      unit.key,
                                                      (current) => ({
                                                        ...current,
                                                        abilities: {
                                                          ...current.abilities,
                                                          [abilityDraftKey(
                                                            abilityOptions.jobSlot,
                                                            ability.key,
                                                          )]:
                                                            event.target
                                                              .checked,
                                                        },
                                                      }),
                                                    );
                                                  }}
                                                />
                                                <Tooltip
                                                  text={
                                                    context
                                                      ?.abilityDescriptions?.[
                                                      ability.key
                                                    ]
                                                  }
                                                >
                                                  <ArtImage
                                                    src={abilityArt(
                                                      art,
                                                      ability.key,
                                                    )}
                                                    label={`${ability.label} icon`}
                                                    variant="item"
                                                  />
                                                </Tooltip>
                                                <span className="workspace-ability-name">
                                                  {ability.label}
                                                </span>
                                              </label>
                                              {(loadout
                                                ? loadout.slots.some(
                                                    (slot) =>
                                                      (unitDraft.loadout[
                                                        loadoutDraftKey(
                                                          slot.combatSet,
                                                          slot.slot,
                                                        )
                                                      ] ?? slot.currentKey) ===
                                                      ability.key,
                                                  )
                                                : ability.equipped) && (
                                                <small>Equipped</small>
                                              )}
                                            </li>
                                          ))}
                                        {abilityOptions.members.every(
                                          (row) => row.kind !== abilityKind,
                                        ) && (
                                          <li>
                                            No abilities in this category.
                                          </li>
                                        )}
                                      </ul>
                                    ) : (
                                      <p>
                                        Ability list unavailable for this job.
                                      </p>
                                    )}
                                  </section>
                                </>
                              ) : null}
                            </div>
                          </div>
                        )}
                      </div>
                    </>
                  ) : null}
                </div>
              </div>
            )}
          </section>
        )}
      </div>

      {addCharacterOpen && context && (
        <div className="workspace-modal-backdrop">
          <section
            role="dialog"
            aria-modal="true"
            aria-label="Add character"
            className="workspace-modal workspace-paper"
            onKeyDown={(event) => {
              if (event.key === 'Escape') cancelAddCharacter();
            }}
          >
            <AddCharacterForm
              context={context}
              draft={draft}
              kind={addCharacterType}
              choices={namedChoices}
              busy={previewBusy}
              invalid={(review?.errors.length ?? 0) > 0}
              errors={review?.errors ?? []}
              onCreature={(creature) => {
                setDraft((current) => ({ ...current, creature }));
              }}
              onKind={chooseCharacterKind}
              onName={(name) => {
                setDraft((current) => ({
                  ...current,
                  generic: current.generic && { ...current.generic, name },
                }));
              }}
              onCharacter={(characterKey) => {
                const character = namedChoices.find(
                  (choice) => choice.key === characterKey,
                );
                const donor = context.namedAddition?.donors.find(
                  (option) => option.sex === character?.sex,
                );
                setDraft((current) => ({
                  ...current,
                  named: current.named
                    ? {
                        ...current.named,
                        characterKey,
                        sourceSlot:
                          donor?.sourceSlot ?? current.named.sourceSlot,
                        sourcePosition:
                          donor?.sourcePosition ?? current.named.sourcePosition,
                      }
                    : null,
                }));
              }}
              onCancel={cancelAddCharacter}
              onAdd={() => void stageCharacter()}
            />
          </section>
        </div>
      )}
      {commandInfo && (
        <div className="workspace-modal-backdrop">
          <section
            role="dialog"
            aria-modal="true"
            aria-label={`${commandInfo.name} actions`}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                setCommandInfo(null);
              }
            }}
            className="workspace-modal workspace-paper"
          >
            <h3>{commandInfo.name}</h3>
            <ul className="workspace-ability-list">
              {commandInfo.members.map((row) => (
                <li key={row.label}>
                  <span>
                    {row.label}{' '}
                    <InfoHint label={row.label} text={row.description} />
                  </span>
                  <small>{row.learned ? 'Learned' : 'Unlearned'}</small>
                </li>
              ))}
            </ul>
            {commandInfo.members.length === 0 && (
              <p>No action list is available for this command.</p>
            )}
            <button
              type="button"
              autoFocus
              onClick={() => {
                setCommandInfo(null);
              }}
            >
              Close
            </button>
          </section>
        </div>
      )}
      {dirty && !addCharacterOpen && (
        <footer className="workspace-pending" aria-label="Pending changes">
          <div>
            <strong>
              {review?.changes.length ?? 0} pending{' '}
              {(review?.changes.length ?? 0) === 1 ? 'change' : 'changes'}
            </strong>
            {!!review?.errors.length && (
              <span>{review.errors.length} values need attention</span>
            )}
            {statsPreview.busy && <span role="status">Calculating stats…</span>}
          </div>

          <button
            type="button"
            className="workspace-secondary"
            disabled={busy || previewBusy}
            onClick={discardAll}
          >
            Discard all
          </button>
          <button
            ref={reviewButton}
            type="button"
            disabled={
              !context ||
              !review ||
              (review.operations.length === 0 &&
                statsPreview.inputs.length === 0) ||
              review.errors.length > 0 ||
              busy ||
              previewBusy ||
              statsPreview.busy ||
              statsPreview.hasErrors
            }
            onClick={() => {
              statsPreview.commitAll();
              setReviewOpen(true);
            }}
            onPointerDown={(event) => {
              if (event.button !== 0) return;
              // Blur can start the calculation before click fires; retain the review request.
              statsPreview.commitAll();
              setReviewOpen(true);
            }}
          >
            Review changes
          </button>
        </footer>
      )}
      {review?.errors.length && !addCharacterOpen ? (
        <div className="workspace-errors" role="alert">
          {review.errors.map((error) => (
            <p key={error}>{error}</p>
          ))}
        </div>
      ) : null}
      {saveError && (
        <div className="workspace-errors" role="alert">
          {saveError}
        </div>
      )}

      {reviewOpen && review && context && (
        <div className="workspace-modal-backdrop">
          <div
            className="workspace-modal"
            role="dialog"
            aria-modal="true"
            aria-label="Review changes"
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                setReviewOpen(false);
                reviewButton.current?.focus();
              }
            }}
          >
            <h2 id="review-heading">
              Review changes for slot {context.manualSlotId + 1}
            </h2>
            <ul className="workspace-review-list">
              {review.changes.map((change, index) => (
                <li key={`${change.group}:${change.label}:${String(index)}`}>
                  {change.summary ? (
                    <strong className="workspace-review-summary">
                      {change.summary}
                    </strong>
                  ) : (
                    <>
                      <strong>{change.group}</strong>
                      <span>{change.label}</span>
                      <span>
                        {change.before} → {change.after}
                      </span>
                    </>
                  )}
                </li>
              ))}
            </ul>
            <div className="workspace-modal-actions">
              <button
                type="button"
                onClick={() => {
                  setReviewOpen(false);
                  reviewButton.current?.focus();
                }}
              >
                Keep editing
              </button>
              <button
                ref={confirmButton}
                type="button"
                disabled={busy || statsPreview.blocked}
                onClick={() => void commit()}
              >
                {busy ? 'Saving…' : 'Confirm and save'}
              </button>
            </div>
          </div>
        </div>
      )}

      {restoreOpen && context && (
        <div className="workspace-modal-backdrop">
          <div
            className="workspace-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="restore-heading"
          >
            <h2 id="restore-heading">Restore file backup?</h2>
            <p>
              This restores every slot in the selected file from its previous
              backup. Changes made since that backup, including pending edits,
              will be lost.
            </p>
            <div className="workspace-modal-actions">
              <button
                type="button"
                onClick={() => {
                  setRestoreOpen(false);
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => void restore()}
              >
                {busy ? 'Restoring…' : 'Restore backup'}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
