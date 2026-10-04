import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { beforeEach, describe, expect, test, vi } from 'vitest';
import type { IpcError, NormalizedSave } from './ipc';
import type {
  CatalogueRef,
  Fact,
  LoadReaderRequest,
  LoadReaderResponse,
  ReaderDocument,
  ReaderUnit,
  SaveTransactionRequest,
} from './reader-ipc';
import { savedUnitRecord } from './reader-test-record';
import { version } from '../package.json';

const mocks = vi.hoisted(() => ({
  chooseSave: vi.fn(),
  getSaveSelection: vi.fn(),
  loadReader:
    vi.fn<(request: LoadReaderRequest) => Promise<LoadReaderResponse>>(),
  saveTransaction: vi.fn(),
  restoreLastBackup: vi.fn(),
  previewJobProgress: vi.fn(),
  previewCharacter: vi.fn(),
  previewDraft: vi.fn(),
}));

vi.mock('./ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./ipc')>()),
  chooseSave: mocks.chooseSave,
  getSaveSelection: mocks.getSaveSelection,
}));

vi.mock('./reader-ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./reader-ipc')>()),
  loadReader: mocks.loadReader,
  saveTransaction: mocks.saveTransaction,
  restoreLastBackup: mocks.restoreLastBackup,
  previewJobProgress: mocks.previewJobProgress,
  previewCharacter: mocks.previewCharacter,
  previewDraft: mocks.previewDraft,
}));

import { App } from './App';

const loadError: IpcError = {
  category: 'unsupported',
  code: 'unsupported_structure',
  retryable: false,
  attempts: null,
  last_transient: null,
};

function saveFor(slot: number): NormalizedSave {
  return {
    schema: 'v6',
    provenance: {
      snapshot: { byte_length: 512 },
      writer_build: { state: 'unknown' },
    },
    container: {
      state: 'known',
      value: {
        embedded_payload_version: 16,
        format_discriminator: 14,
        payload_byte_length: 2_008_216,
        stored_adler_status: 'mismatched',
      },
    },
    selected_manual_slot: {
      state: 'known',
      value: { id: slot },
    },
  };
}

function response(
  requestId: number,
  occupiedManualSlots: number[],
  save: NormalizedSave | null,
): LoadReaderResponse {
  return {
    requestId,
    occupiedManualSlots,
    slotSummaries: [],
    save,
    reader: null,
    readerError: null,
  };
}

function unknown<T>(): Fact<T> {
  return { value: { state: 'unknown' } };
}

function known<T>(value: T): Fact<T> {
  return { value: { state: 'known', value } };
}

function readerUnit(key: number, name: string, level = 12): ReaderUnit {
  const bases = {
    hp: unknown<number>(),
    mp: unknown<number>(),
    speed: unknown<number>(),
    physical_attack: unknown<number>(),
    magical_attack: unknown<number>(),
  };
  return {
    key,
    persistent_identity: unknown(),
    name: known({ text: name, origin: 'localized_catalogue' }),
    kind: unknown(),
    membership: known('party'),
    sprite: unknown(),
    portrait: unknown(),
    stored: {
      level: known(level),
      experience: unknown(),
      brave: unknown(),
      faith: unknown(),
      sex: unknown(),
      birthday: unknown(),
      zodiac: unknown(),
      statuses: unknown(),
      bases,
    },
    effective: {
      ...bases,
      movement_tiles: unknown(),
      jump_tiles: unknown(),
      evasion: unknown(),
    },
    growth: unknown(),
    current_job: known({
      id: 'job:78',
      label: { state: 'known', value: 'Synthetic job' },
      description: {
        state: 'known',
        value: 'First description line.\nSecond description line.',
      },
      asset_key: { state: 'unknown' },
    }),
    jobs: unknown(),
    learned_abilities: unknown(),
    saved_ability_flags: unknown(),
    abilities: {
      primary_command: unknown(),
      secondary_command: unknown(),
      reaction: unknown(),
      support: unknown(),
      movement: unknown(),
    },
    equipment: {
      head: unknown(),
      body: unknown(),
      accessory: unknown(),
      right_weapon: unknown(),
      right_shield: unknown(),
      left_weapon: unknown(),
      left_shield: unknown(),
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
    saved: savedUnitRecord(),
  };
}

function jobReference(id: number, label: string): CatalogueRef {
  return {
    id: `job:${String(id)}`,
    label: { state: 'known', value: label },
    description: { state: 'unknown' },
    asset_key: { state: 'unknown' },
  };
}

function readerFor(
  slot: number,
  generation: number,
  units = [readerUnit(0, 'Étoile 太陽')],
): ReaderDocument {
  return {
    schema: 'reader_v2',
    profile: 'english_steam_enhanced_manual',
    identity: {
      session: 'synthetic-session',
      snapshot_generation: generation,
      resource_generation: generation,
      resource_token: { state: 'known', value: 'a'.repeat(64) },
      manual_slot: slot,
    },
    roster: known(units),
    inventory: unknown(),
    gil: unknown(),
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

const unavailableAdditions = {
  genericCreation: { targetPosition: null, donors: [] },
  storyAddition: { targetPosition: null, donors: [] },
  guestAddition: { targetPosition: null, donors: [] },
};

function readerResponse(
  request: LoadReaderRequest,
  reader: ReaderDocument | null,
): LoadReaderResponse {
  return {
    requestId: request.requestId,
    occupiedManualSlots: [0, 1],
    slotSummaries: [],
    save: request.manualSlotId === null ? null : saveFor(request.manualSlotId),
    reader,
    readerError: null,
  };
}

async function openRoster() {
  await chooseSlot(1);
  await screen.findByRole('heading', { name: 'Units' });
}

async function openSection(name: 'Game' | 'Units' | 'Inventory') {
  fireEvent.click(await screen.findByRole('button', { name }));
}

async function openReview() {
  await waitFor(() => {
    expect(
      screen.getByRole('button', { name: 'Review changes' }),
    ).toHaveProperty('disabled', false);
  });
  fireEvent.click(screen.getByRole('button', { name: 'Review changes' }));
}

async function confirmChanges() {
  await waitFor(() => {
    expect(
      screen
        .getByRole('button', { name: 'Review changes' })
        .hasAttribute('disabled'),
    ).toBe(false);
  });
  await openReview();
  fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
}

async function chooseSlot(displayNumber: number) {
  const select = await screen.findByRole('combobox', { name: 'Manual slot' });
  await screen.findByRole('option', {
    name: new RegExp(`^Slot ${String(displayNumber)}(?:\\b|$)`),
  });
  fireEvent.change(select, { target: { value: String(displayNumber - 1) } });
}

beforeEach(() => {
  mocks.chooseSave.mockReset();
  mocks.getSaveSelection.mockReset();
  mocks.loadReader.mockReset();
  mocks.saveTransaction.mockReset();
  mocks.restoreLastBackup.mockReset();
  mocks.previewDraft.mockReset().mockImplementation(async () => {
    const last = mocks.loadReader.mock.results.at(-1);
    const response = last?.type === 'return' ? await last.value : null;
    if (!response?.reader) throw new Error('Missing reader');
    return { reader: structuredClone(response.reader), solvedBase: null };
  });
  mocks.previewCharacter
    .mockReset()
    .mockImplementation(async (request: SaveTransactionRequest) => {
      const response: LoadReaderResponse = structuredClone(
        await mocks.loadReader({
          requestId: request.snapshotGeneration,
          manualSlotId: request.manualSlotId,
        }),
      );
      for (const operation of request.operations) {
        if (
          operation.kind !== 'create_generic_from_slot' &&
          operation.kind !== 'add_named_from_unit'
        )
          throw new Error('Missing addition');
        const name =
          operation.kind === 'create_generic_from_slot'
            ? operation.name
            : (response.editContext?.namedAddition?.characters
                .find((row) => row.key === operation.characterKey)
                ?.label.replace(/ · variant \d+$/, '') ?? 'Character');
        if (response.reader?.roster.value.state === 'known') {
          response.reader.roster.value.value.push(
            readerUnit(operation.unitPosition, name, 1),
          );
        }
      }
      if (response.editContext) {
        const base =
          response.editContext.genericCreation.targetPosition ??
          response.editContext.namedAddition?.targetPosition;
        let next = base ?? 1;
        const positions = new Set(
          request.operations.flatMap((operation) =>
            'unitPosition' in operation ? [operation.unitPosition] : [],
          ),
        );
        while (positions.has(next)) next++;
        for (const addition of [
          response.editContext.genericCreation,
          response.editContext.namedAddition,
        ]) {
          if (!addition) continue;
          addition.targetPosition = next < 50 ? next : null;
          addition.donors.forEach((donor) => {
            donor.unitPosition = next;
          });
        }
        response.editContext.namedAddition?.characters.forEach((character) => {
          if (
            request.operations.some(
              (operation) =>
                operation.kind === 'add_named_from_unit' &&
                operation.characterKey === character.key,
            )
          )
            character.available = false;
        });
      }
      return response;
    });
  mocks.previewJobProgress
    .mockReset()
    .mockImplementation(
      (request: {
        unitPosition: number;
        levels: { jobSlot: number; level: number }[];
      }) =>
        Promise.resolve({
          jobOptions: {
            unitPosition: request.unitPosition,
            jobSlots: request.levels.map((row) => row.jobSlot),
          },
          invalidatedSlots: [],
        }),
    );
});

describe('workspace cleanup', () => {
  test.each([
    ['Equip from inventory', 'held', '1 → 0'],
    ['Create and equip', 'create_and_equip', '1 → 2'],
  ] as const)(
    'stages equipment and held counts with %s',
    async (action, source, countChange) => {
      mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
      mocks.loadReader.mockImplementation((request) => {
        const document =
          request.manualSlotId === null
            ? null
            : readerFor(0, request.requestId, [readerUnit(0, 'Ramza')]);
        if (document) {
          document.inventory = known(
            [1, 23].map((id) => ({
              key: id,
              item: known({
                id: `item:${String(id)}`,
                label: {
                  state: 'known',
                  value: id === 1 ? 'Old sword' : 'Blood Sword',
                },
                description: { state: 'unknown' },
                asset_key: { state: 'unknown' },
              }),
              category: known('Swords'),
              quantity: known(id === 1 ? 0 : 1),
            })),
          );
        }
        return Promise.resolve({
          ...readerResponse(request, document),
          editContext: document
            ? {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 0,
                gil: 100,
                gearOptions: [
                  {
                    unitPosition: 0,
                    slots: [
                      {
                        slot: 'right_hand' as const,
                        currentKey: 'item:1',
                        choices: [
                          { key: 'item:1', label: 'Old sword' },
                          { key: 'item:23', label: 'Blood Sword' },
                        ],
                      },
                    ],
                  },
                ],
              }
            : null,
        });
      });
      mocks.saveTransaction.mockResolvedValue({
        gil: 100,
        backupCreated: true,
      });
      render(<App />);
      await openRoster();
      fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
      fireEvent.click(screen.getByRole('button', { name: 'Change Right arm' }));
      fireEvent.change(
        screen.getByRole('combobox', { name: 'Right arm item' }),
        {
          target: { value: 'item:23' },
        },
      );
      fireEvent.click(screen.getByRole('button', { name: action }));
      await openReview();
      const dialog = screen.getByRole('dialog', { name: 'Review changes' });
      expect(within(dialog).getByText(countChange)).toBeTruthy();
      expect(within(dialog).getByText('0 → 1')).toBeTruthy();
      fireEvent.click(
        within(dialog).getByRole('button', { name: 'Confirm and save' }),
      );
      await waitFor(() => {
        expect(mocks.saveTransaction).toHaveBeenCalledWith({
          snapshotGeneration: 2,
          manualSlotId: 0,
          operations: [
            {
              kind: 'gear',
              unitPosition: 0,
              slot: 'right_hand',
              itemKey: 'item:23',
              source,
            },
          ],
        });
      });
    },
  );

  test('shows sourced ability tooltips on hover and focus, including changed loadouts', async () => {
    const unit = readerUnit(0, 'Ramza');
    unit.jobs = known([
      {
        slot: 0,
        job: unit.current_job,
        level: known(1),
        current_jp: known(20),
        total_jp: known(100),
      },
    ]);
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(0, request.requestId, [unit]),
        ),
        editContext: {
          ...unavailableAdditions,
          snapshotGeneration: request.requestId,
          manualSlotId: 0,
          gil: 100,
          jobOptions: [{ unitPosition: 0, jobSlots: [0] }],
          abilityDescriptions: {
            'ability:1': '<color=1>Action description.</color>',
            'ability:2': 'First reaction description.',
            'ability:3': 'Second reaction description.',
          },
          abilityOptions: [
            {
              unitPosition: 0,
              jobSlot: 0,
              commandKey: 'command:1',
              commandLabel: 'Arts',
              members: [
                {
                  key: 'ability:1',
                  label: 'Action one',
                  learned: false,
                  equipped: false,
                  kind: 'action',
                },
                {
                  key: 'ability:2',
                  label: 'Reaction one',
                  learned: true,
                  equipped: true,
                  kind: 'reaction',
                },
                {
                  key: 'ability:3',
                  label: 'Reaction two',
                  learned: true,
                  equipped: false,
                  kind: 'reaction',
                },
              ],
            },
          ],
          loadoutOptions: [
            {
              unitPosition: 0,
              secondaryChoices: [],
              reactionChoices: [
                { key: 'ability:2', label: 'Reaction one' },
                { key: 'ability:3', label: 'Reaction two' },
              ],
              supportChoices: [],
              movementChoices: [],
              slots: [
                { combatSet: null, slot: 'reaction', currentKey: 'ability:2' },
              ],
            },
          ],
        },
      }),
    );
    const rendered = render(<App />);
    await openRoster();
    expect(screen.queryByRole('button', { name: 'Combat sets' })).toBeNull();
    expect(
      rendered.container.querySelector('.workspace-readonly-mark'),
    ).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Jobs' }));
    fireEvent.mouseEnter(screen.getByText('Action one'));
    expect(screen.getByRole('tooltip').textContent).toBe('Action description.');
    fireEvent.mouseLeave(screen.getByText('Action one'));
    await waitFor(() => {
      expect(screen.queryByRole('tooltip')).toBeNull();
    });
    fireEvent.click(screen.getByRole('button', { name: 'reaction' }));
    fireEvent.focus(screen.getByText('Reaction one'));
    expect(screen.getByRole('tooltip').textContent).toBe(
      'First reaction description.',
    );
    fireEvent.blur(screen.getByText('Reaction one'));
    fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
    const choice = screen.getByRole('combobox', { name: 'Active reaction' });
    fireEvent.focus(choice);
    expect(screen.getByRole('tooltip').textContent).toBe(
      'First reaction description.',
    );
    fireEvent.change(choice, { target: { value: 'ability:3' } });
    expect(screen.getByRole('tooltip').textContent).toBe(
      'Second reaction description.',
    );
    expect(screen.queryByText(/Saved:/)).toBeNull();
    fireEvent.click(
      screen.getByRole('button', { name: 'Reset Active reaction' }),
    );
    expect(choice).toHaveProperty('value', 'ability:2');
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
  });
  function setup(units: ReaderUnit[], inventory?: ReaderDocument['inventory']) {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request) => {
      const document =
        request.manualSlotId === null
          ? null
          : readerFor(request.manualSlotId, request.requestId, units);
      if (document && inventory) document.inventory = inventory;
      return Promise.resolve({
        ...readerResponse(request, document),
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: request.manualSlotId,
                gil: 100,
              },
      });
    });
  }

  test.each([
    ['named', 0],
    ['generic', 1],
    ['monster', 2],
    ['guest', 50],
  ] as const)(
    'stages and resets Zodiac for a %s unit',
    async (family, position) => {
      const unit = readerUnit(position, 'Zodiac trial');
      unit.kind = known(
        family === 'monster'
          ? 'monster'
          : family === 'generic'
            ? 'regular_human'
            : 'unique_human',
      );
      unit.membership = known(family === 'guest' ? 'guest' : 'party');
      mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
      mocks.loadReader.mockImplementation((request) =>
        Promise.resolve({
          ...readerResponse(
            request,
            request.manualSlotId === null
              ? null
              : readerFor(request.manualSlotId, request.requestId, [unit]),
          ),
          editContext:
            request.manualSlotId === null
              ? null
              : {
                  ...unavailableAdditions,
                  snapshotGeneration: request.requestId,
                  manualSlotId: request.manualSlotId,
                  gil: 100,
                  zodiacOptions: [
                    {
                      unitPosition: position,
                      current: 'aries',
                      choices: [
                        { key: 'aries', label: 'Aries' },
                        { key: 'libra', label: 'Libra' },
                      ],
                    },
                  ],
                },
        }),
      );
      mocks.saveTransaction.mockResolvedValue({
        gil: 100,
        backupCreated: true,
      });
      render(<App />);
      await openRoster();
      const picker = screen.getByRole('combobox', { name: 'Zodiac' });
      fireEvent.change(picker, { target: { value: 'libra' } });
      await openReview();
      expect(
        within(screen.getByRole('dialog')).getByText(/Aries.*Libra/),
      ).toBeTruthy();
      fireEvent.click(screen.getByRole('button', { name: 'Keep editing' }));
      fireEvent.click(screen.getByRole('button', { name: 'Discard all' }));
      expect(screen.getByRole('combobox', { name: 'Zodiac' })).toHaveProperty(
        'value',
        'aries',
      );
      fireEvent.change(screen.getByRole('combobox', { name: 'Zodiac' }), {
        target: { value: 'libra' },
      });
      await confirmChanges();
      await waitFor(() => {
        expect(mocks.saveTransaction).toHaveBeenCalledWith({
          snapshotGeneration: 2,
          manualSlotId: 0,
          operations: [
            { kind: 'zodiac', unitPosition: position, sign: 'libra' },
          ],
        });
      });
    },
  );

  test('stages a generic Sex swap', async () => {
    const generic = readerUnit(1, 'Recruit');
    generic.kind = known('regular_human');
    generic.stored.sex = known('Male');
    generic.jobs = known(
      [
        [17, 91, 'Bard'],
        [18, 92, 'Dancer'],
      ].map(([slot, id, label]) => ({
        slot: Number(slot),
        job: known(jobReference(Number(id), String(label))),
        level: known(0),
        current_jp: known(0),
        total_jp: known(0),
      })),
    );
    const named = readerUnit(0, 'Named');
    named.kind = known('unique_human');
    named.stored.sex = known('Male');
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId, [
                generic,
                named,
              ]),
        ),
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: request.manualSlotId,
                gil: 100,
                sexOptions: [
                  {
                    unitPosition: 1,
                    current: 'male',
                    choices: [
                      { key: 'male', label: 'Male' },
                      { key: 'female', label: 'Female' },
                    ],
                    unavailableReason: null,
                  },
                ],
              },
      }),
    );
    mocks.saveTransaction.mockResolvedValue({ gil: 100, backupCreated: true });
    render(<App />);
    await openRoster();
    expect(screen.getByRole('combobox', { name: 'Sex' })).toHaveProperty(
      'value',
      'male',
    );
    fireEvent.change(screen.getByRole('combobox', { name: 'Sex' }), {
      target: { value: 'female' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Jobs' }));
    expect(screen.getByRole('button', { name: /Dancer/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Bard/ })).toBeNull();
    await openReview();
    expect(
      within(screen.getByRole('dialog')).getByText(/Male.*Female/),
    ).toBeTruthy();
    await confirmChanges();
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 0,
        operations: [{ kind: 'sex', unitPosition: 1, sex: 'female' }],
      });
    });
  });

  test('retains view-only Bravery, Faith, job EXP, evasion and descriptions', async () => {
    const guest = readerUnit(4, 'Guest');
    guest.membership = known('guest');
    guest.stored.brave = known(0);
    guest.stored.faith = known(100);
    guest.effective.evasion = known([
      {
        source: 'character',
        physical_basis_points: known(1250),
        magical_basis_points: known(0),
      },
    ]);
    guest.jobs = known([
      {
        slot: 0,
        job: guest.current_job,
        level: known(0),
        current_jp: known(12),
        total_jp: known(34),
      },
    ]);
    guest.abilities.reaction = known({
      ...jobReference(1, 'Saved reaction'),
      description: { state: 'known', value: 'A useful reaction description.' },
    });
    setup([guest]);
    const rendered = render(<App />);
    await openRoster();
    const status = rendered.container.querySelector('.workspace-status-grid');
    expect(status?.textContent).toContain('Bravery0');
    expect(status?.textContent).toContain('Faith100');
    expect(status?.textContent).toContain('Physical 12.5%');
    expect(status?.textContent).toContain('Magical 0%');
    expect(screen.queryByRole('textbox', { name: 'Bravery' })).toBeNull();
    expect(rendered.container.querySelector('.eyebrow')).toBeNull();
    expect(rendered.container.querySelector('.app-brand-mark')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Jobs' }));
    const progress = screen.getByLabelText('Job progress');
    expect(progress.textContent).toContain('Job level0');
    expect(progress.textContent).toContain('Spendable JP12');
    expect(progress.textContent).toContain('Job EXP34');
    expect(screen.getByLabelText('Job description').textContent).toContain(
      'First description',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
    fireEvent.focus(screen.getByText('Saved reaction'));
    expect(screen.getByRole('tooltip').textContent).toBe(
      'A useful reaction description.',
    );
    fireEvent.keyDown(screen.getByText('Saved reaction'), { key: 'Escape' });
    expect(screen.queryByRole('tooltip')).toBeNull();
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
  });

  test('resets individual fields and discards edits across sections without writing', async () => {
    const first = readerUnit(0, 'First');
    first.saved.start_bcp = 60;
    const second = readerUnit(1, 'Second');
    second.saved.start_faith = 45;
    setup([first, second]);
    render(<App />);
    await openRoster();
    expect(screen.queryByText(/Saved: 60/)).toBeNull();
    fireEvent.change(screen.getByRole('textbox', { name: 'Bravery' }), {
      target: { value: '75' },
    });
    expect(screen.queryByText(/Saved:/)).toBeNull();
    expect(screen.getByRole('button', { name: 'Reset Bravery' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Reset Bravery' }));
    expect(screen.getByRole('textbox', { name: 'Bravery' })).toHaveProperty(
      'value',
      '60',
    );
    expect(screen.queryByRole('button', { name: 'Review changes' })).toBeNull();
    fireEvent.change(screen.getByRole('textbox', { name: 'Bravery' }), {
      target: { value: '70' },
    });
    fireEvent.click(screen.getByRole('button', { name: /^Member 2 Second/ }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Faith' }), {
      target: { value: '25' },
    });
    await openSection('Game');
    fireEvent.change(screen.getByRole('textbox', { name: 'Gil' }), {
      target: { value: '200' },
    });
    expect(screen.getByText('3 pending changes')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Discard all' }));
    expect(screen.getByRole('textbox', { name: 'Gil' })).toHaveProperty(
      'value',
      '100',
    );
    await openSection('Units');
    expect(screen.getByRole('textbox', { name: 'Faith' })).toHaveProperty(
      'value',
      '45',
    );
    fireEvent.click(screen.getByRole('button', { name: /^Member 1 First/ }));
    expect(screen.getByRole('textbox', { name: 'Bravery' })).toHaveProperty(
      'value',
      '60',
    );
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
  });

  test('merges editable Gil and useful overview facts into one panel', async () => {
    setup([readerUnit(0, 'Ramza')]);
    render(<App />);
    await openRoster();
    await openSection('Game');
    const overview = screen
      .getByRole('heading', { name: 'Save overview' })
      .closest('article');
    if (!overview) throw new Error('Missing overview panel');
    expect(overview.parentElement?.querySelectorAll('article').length).toBe(1);
    expect(
      within(overview).getByRole('textbox', { name: 'Gil' }),
    ).toHaveProperty('value', '100');
    for (const label of [
      'Chapter',
      'Current area',
      'Ramza level',
      'Saved at',
    ]) {
      expect(within(overview).getByText(label)).toBeTruthy();
    }
    expect(screen.queryByText('Difficulty')).toBeNull();
    expect(screen.queryByText('Stored difficulty code')).toBeNull();
    expect(screen.queryByText('Slot title')).toBeNull();
    expect(screen.queryByText('Hero name')).toBeNull();
    expect(screen.queryByText('Play time')).toBeNull();
    fireEvent.change(within(overview).getByRole('textbox', { name: 'Gil' }), {
      target: { value: '101' },
    });
    expect(screen.getByRole('button', { name: 'Review changes' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Reset Gil' }));
    expect(screen.queryByRole('button', { name: 'Review changes' })).toBeNull();
  });

  test('identifies the recovery file if save rotation cannot roll back', async () => {
    setup([readerUnit(0, 'Ramza')]);
    mocks.saveTransaction.mockRejectedValue({ code: 'save_recovery_required' });
    render(<App />);
    await openRoster();
    await openSection('Game');
    fireEvent.change(screen.getByRole('textbox', { name: 'Gil' }), {
      target: { value: '101' },
    });
    await confirmChanges();
    expect(
      await screen.findByText(
        /The original save is in the file with " - backup" in its name/,
      ),
    ).toBeTruthy();
    expect(screen.getByRole('textbox', { name: 'Gil' })).toHaveProperty(
      'value',
      '101',
    );
  });

  test('preserves selection, panel and filters through save and reload, and handles a removed member', async () => {
    const first = readerUnit(0, 'First');
    const second = readerUnit(1, 'Second');
    setup([first, second]);
    mocks.saveTransaction.mockResolvedValue({ backupCreated: true });
    render(<App />);
    await openRoster();
    fireEvent.click(screen.getByRole('button', { name: /^Member 2 Second/ }));
    fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
    fireEvent.click(screen.getByRole('button', { name: /^Member 1 First/ }));
    expect(
      screen
        .getByRole('button', { name: 'Equipment' })
        .getAttribute('aria-pressed'),
    ).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: /^Member 2 Second/ }));
    fireEvent.change(screen.getByRole('searchbox', { name: 'Find a unit' }), {
      target: { value: 'Second' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    expect(
      (
        await screen.findByRole('button', { name: /^Member 1 Second/ })
      ).getAttribute('aria-pressed'),
    ).toBe('true');
    expect(
      screen
        .getByRole('button', { name: 'Equipment' })
        .getAttribute('aria-pressed'),
    ).toBe('true');
    expect(
      screen.getByRole('searchbox', { name: 'Find a unit' }),
    ).toHaveProperty('value', 'Second');
    await openSection('Game');
    fireEvent.change(screen.getByRole('textbox', { name: 'Gil' }), {
      target: { value: '101' },
    });
    await confirmChanges();
    await screen.findByText(/Changes saved to slot 1/);
    expect(
      screen.getByRole('button', { name: 'Game' }).getAttribute('aria-pressed'),
    ).toBe('true');
    await openSection('Units');
    expect(screen.getByRole('heading', { name: 'Second' })).toBeTruthy();
    expect(
      screen
        .getByRole('button', { name: 'Equipment' })
        .getAttribute('aria-pressed'),
    ).toBe('true');
    setup([first]);
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    await screen.findByText('No matching units.');
    expect(screen.getByRole('heading', { name: 'First' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Clear search' }));
    expect(
      screen.getByRole('button', { name: /^Member 1 First/ }),
    ).toBeTruthy();
  });

  test('updates held counts and quantities from staged inventory and resets an unheld item', async () => {
    const inventory = known([
      {
        key: 1,
        item: known({ ...jobReference(1, 'Potion'), id: 'item:1' }),
        category: known('Consumables'),
        quantity: known(5),
      },
      {
        key: 2,
        item: known({ ...jobReference(2, 'Ether'), id: 'item:2' }),
        category: known('Consumables'),
        quantity: known(0),
      },
    ]);
    setup([readerUnit(0, 'Ramza')], inventory);
    render(<App />);
    await openRoster();
    await openSection('Inventory');
    expect(screen.getByText('1 item type held')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'All items' }));
    fireEvent.click(screen.getByRole('button', { name: /^Ether/ }));
    fireEvent.change(
      screen.getByRole('textbox', { name: 'Quantity for Ether' }),
      { target: { value: '3' } },
    );
    expect(screen.getByText('2 item types held')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Held' }));
    expect(
      screen.getByRole('button', { name: /^Ether/ }).textContent,
    ).toContain('× 3');
    fireEvent.click(
      screen.getByRole('button', { name: 'Reset Quantity for Ether' }),
    );
    expect(screen.queryByRole('button', { name: /^Ether/ })).toBeNull();
    expect(screen.getByText('1 item type held')).toBeTruthy();
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
  });
});

describe('verified save inspection', () => {
  test('prefers named and sex-matched job stills before portrait fallback', async () => {
    const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          schema: 'reader_art_v1',
          jobs: {},
          items: {},
          abilities: {},
          portraits: {
            'character:120': 'images/portrait-120.png',
            'character:125': 'images/portrait-125.png',
          },
          job_sprites: {
            'job:78': { female: 'images/sprite-job-78-female.png' },
          },
          character_sprites: { 'character:4': 'images/sprite-character-4.png' },
        }),
      ),
    );
    try {
      mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
      const jobStill = readerUnit(0, 'Job still');
      jobStill.portrait = known('character:120');
      jobStill.stored.sex = known('Female');
      const namedStill = readerUnit(1, 'Named still');
      namedStill.portrait = known('character:4');
      namedStill.stored.sex = known('Female');
      const portrait = readerUnit(2, 'Portrait fallback');
      portrait.portrait = known('character:125');
      mocks.loadReader.mockImplementation((request) =>
        Promise.resolve(
          readerResponse(
            request,
            request.manualSlotId === null
              ? null
              : readerFor(0, request.requestId, [
                  jobStill,
                  namedStill,
                  portrait,
                ]),
          ),
        ),
      );
      render(<App />);
      await openRoster();
      for (const [name, file] of [
        ['Job still', 'sprite-job-78-female.png'],
        ['Named still', 'sprite-character-4.png'],
        ['Portrait fallback', 'portrait-125.png'],
      ] as const) {
        const button = screen.getByRole('button', {
          name: new RegExp(`^Member \\d+ ${name}`),
        });
        await waitFor(() => {
          expect(
            within(button)
              .getByRole('img', { name: `${name} avatar` })
              .getAttribute('src'),
          ).toContain(file);
        });
      }
    } finally {
      fetch.mockRestore();
    }
  });
  test('shows current-job growth in its Status panel and missing rates as unknown', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    const ramza = readerUnit(0, 'Ramza');
    ramza.growth = known({
      hp: 11,
      mp: 12,
      speed: 100,
      physical_attack: 40,
      magical_attack: 50,
    });
    const agrias = readerUnit(1, 'Agrias');
    mocks.loadReader.mockImplementation((request) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(0, request.requestId, [ramza, agrias]),
        ),
      ),
    );
    render(<App />);
    await openRoster();
    fireEvent.click(screen.getByRole('button', { name: /^Member 1 Ramza/ }));
    const panel = screen.getByRole('region', { name: 'Synthetic job Growth' });
    expect(
      within(panel).getByRole('heading', { name: 'Synthetic job Growth' }),
    ).toBeTruthy();
    expect(panel.querySelector('details')).toBeNull();
    expect(within(panel).queryByText(/description|coefficients/i)).toBeNull();
    expect(
      [...panel.querySelectorAll('dd')].map((row) => row.textContent),
    ).toEqual(['11', '12', '100', '40', '50']);
    expect(within(panel).queryByRole('textbox')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /^Member 2 Agrias/ }));
    expect(
      within(
        screen.getByRole('region', { name: 'Synthetic job Growth' }),
      ).getAllByText('Unknown'),
    ).toHaveLength(5);
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
  });
  test('offers accessible manual selection when no saved selection exists', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'no_selection' });
    render(<App />);

    await waitFor(() => {
      expect(mocks.getSaveSelection).toHaveBeenCalled();
    });
    expect(screen.queryByRole('region', { name: 'Welcome' })).toBeNull();
    expect(screen.queryByText(/Your journey|Open a save/)).toBeNull();
    expect(screen.getByRole('button', { name: 'Choose save' })).toBeTruthy();
    expect(screen.getByLabelText(`App version ${version}`).textContent).toBe(
      `v${version}`,
    );
    expect(screen.queryByText('Save workspace')).toBeNull();
    expect(
      screen.queryByRole('button', { name: 'Job and ability reference' }),
    ).toBeNull();
    expect(
      screen.getByRole('button', { name: 'Refresh slots' }),
    ).toHaveProperty('disabled', true);
    expect(
      screen.queryByRole('heading', { name: 'Save file details' }),
    ).toBeNull();
  });

  test('shows no occupied slots without inventing a slot or roster', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockResolvedValue(response(1, [], null));
    render(<App />);

    expect(
      await screen.findByRole('heading', { name: 'No occupied manual slots' }),
    ).toBeTruthy();
    expect(screen.getByLabelText(`App version ${version}`).textContent).toBe(
      `v${version}`,
    );
    expect(screen.queryByRole('combobox', { name: 'Manual slot' })).toBeNull();
  });

  test('enumerates multiple manual slots and shows selected file details', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    const withSummaries = (value: LoadReaderResponse): LoadReaderResponse => ({
      ...value,
      slotSummaries: [
        {
          manualSlot: 0,
          title: 'First',
          savedAtUnixSeconds: null,
          playTimeSeconds: 0,
        },
        {
          manualSlot: 9,
          title: 'Second',
          savedAtUnixSeconds: 1_700_000_000,
          playTimeSeconds: 7200,
        },
      ],
    });
    mocks.loadReader
      .mockResolvedValueOnce(withSummaries(response(1, [0, 9], null)))
      .mockResolvedValueOnce(withSummaries(response(2, [0, 9], saveFor(9))));
    render(<App />);

    const select = await screen.findByRole('combobox', { name: 'Manual slot' });
    const options = [...select.querySelectorAll('option')];
    expect(options.map((option) => option.value)).toEqual(['', '9', '0']);
    expect(options[1]?.textContent).toContain('Slot 10 · Second');
    expect(options[1]?.textContent).toContain('2023');
    expect(options[1]?.textContent).toMatch(/\d{2}:\d{2}/);
    expect(options[1]?.textContent).not.toContain('played');
    expect(options[2]?.textContent).toContain('Slot 1 · First');
    fireEvent.change(select, { target: { value: '9' } });
    expect(mocks.loadReader).toHaveBeenLastCalledWith({
      requestId: 2,
      manualSlotId: 9,
    });

    expect(await screen.findByRole('heading', { name: 'Game' })).toBeTruthy();
    expect(screen.queryByText('Diagnostics')).toBeNull();
    expect(
      screen.getByRole('combobox', { name: 'Manual slot' }),
    ).toHaveProperty('value', '9');
    expect(screen.getByRole('button', { name: 'Reload slot 10' })).toBeTruthy();
    expect(screen.queryByText('Writer build')).toBeNull();
    expect(
      screen.queryByText(/Unnamed job positions remain unnamed/),
    ).toBeNull();
    expect(screen.queryByText('Writer build')).toBeNull();
  });

  test('edits loaded slot gil only after valid input and reloads after save', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...response(
          request.requestId,
          [33],
          request.manualSlotId === null ? null : saveFor(33),
        ),
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 33,
                gil: request.requestId === 3 ? 313041 : 313040,
                backupAvailable: request.requestId === 3,
              },
      }),
    );
    mocks.saveTransaction.mockResolvedValue({
      gil: 313041,
      backupCreated: true,
    });
    render(<App />);
    await chooseSlot(34);
    await openSection('Game');
    const input = await screen.findByRole('textbox', { name: 'Gil' });
    expect(input).toHaveProperty('value', '313040');
    expect(screen.queryByText(/Changes are staged until/)).toBeNull();
    fireEvent.change(input, { target: { value: '-1' } });
    expect(
      screen.getByRole('button', { name: 'Review changes' }),
    ).toHaveProperty('disabled', true);
    fireEvent.change(input, { target: { value: '313041' } });
    await confirmChanges();
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 33,
        operations: [{ kind: 'gil', value: '313041' }],
      });
    });
    expect(await screen.findByText(/Changes saved to slot 34/)).toBeTruthy();
    expect(input).toHaveProperty('value', '313041');
    fireEvent.click(
      screen.getByRole('button', { name: 'Restore file backup' }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Restore backup' }));
    await waitFor(() => {
      expect(mocks.restoreLastBackup).toHaveBeenCalledWith({
        snapshotGeneration: 3,
        manualSlotId: 33,
      });
    });
  });

  test('stages a named generic through the add-character dialog', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId),
        ),
        occupiedManualSlots: [31, 37],
        editContext:
          request.manualSlotId === 31
            ? {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 31,
                gil: 100,
                genericCreation: {
                  targetPosition: 13,
                  donors: [
                    {
                      sourceSlot: 37,
                      sourcePosition: 12,
                      unitPosition: 13,
                      label:
                        'Latisha · Squire · Female · Lv 1 · save slot 38, member 13',
                      level: 1,
                      sex: 'Female',
                    },
                  ],
                },
                storyAddition: { targetPosition: 13, donors: [] },
                guestAddition: { targetPosition: 13, donors: [] },
              }
            : null,
      }),
    );
    mocks.saveTransaction.mockResolvedValue({ gil: 100, backupCreated: true });
    render(<App />);
    await chooseSlot(32);
    fireEvent.click(
      await screen.findByRole('button', { name: 'Add character' }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: 'Create a generic character' }),
    );
    fireEvent.change(screen.getByRole('textbox', { name: 'New name' }), {
      target: { value: 'Mira' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add and edit' }));
    await waitFor(() => {
      expect(
        screen.queryByRole('dialog', { name: 'Add character' }),
      ).toBeNull();
    });
    expect(screen.getByRole('heading', { name: 'Mira' })).toBeTruthy();
    fireEvent.change(screen.getByRole('textbox', { name: 'Faith' }), {
      target: { value: '25' },
    });
    await openReview();
    expect(screen.getByText('Mira added')).toBeTruthy();
    expect(screen.queryByText('Equipment and progress')).toBeNull();
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 31,
        operations: [
          {
            kind: 'create_generic_from_slot',
            sourceSlot: 37,
            sourcePosition: 12,
            unitPosition: 13,
            name: 'Mira',
          },
          { kind: 'faith', unitPosition: 13, value: '25' },
        ],
      });
    });
  });

  test('a failed character preview can be cancelled without saving or adding a roster member', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(31, request.requestId),
        ),
        occupiedManualSlots: [31],
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 31,
                gil: 100,
                genericCreation: {
                  targetPosition: 13,
                  donors: [
                    {
                      sourceSlot: 37,
                      sourcePosition: 12,
                      unitPosition: 13,
                      label: 'Generic',
                      level: 1,
                      sex: 'Female',
                    },
                  ],
                },
              },
      }),
    );
    mocks.previewCharacter.mockRejectedValueOnce(new Error('Preview failed'));
    render(<App />);
    await chooseSlot(32);
    fireEvent.click(
      await screen.findByRole('button', { name: 'Add character' }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: 'Create a generic character' }),
    );
    fireEvent.change(screen.getByRole('textbox', { name: 'New name' }), {
      target: { value: 'Mira' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add and edit' }));
    expect(await screen.findByText(/Could not add the character/)).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Mira' })).toBeNull();
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByText('0 pending changes')).toBeNull();
    expect(
      screen.queryByRole('contentinfo', { name: 'Pending changes' }),
    ).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  test('offers named identities absent from every save slot', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId),
        ),
        occupiedManualSlots: [31, 37],
        editContext:
          request.manualSlotId === 31
            ? {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 31,
                gil: 100,
                genericCreation: { targetPosition: 13, donors: [] },
                storyAddition: { targetPosition: 13, donors: [] },
                guestAddition: { targetPosition: 13, donors: [] },
                namedAddition: {
                  targetPosition: 13,
                  donors: [
                    {
                      sourceSlot: 31,
                      sourcePosition: 1,
                      unitPosition: 13,
                      label: 'Female starting record',
                      level: 1,
                      sex: 'Female',
                    },
                    {
                      sourceSlot: 31,
                      sourcePosition: 0,
                      unitPosition: 13,
                      label:
                        'Ramza · Squire · Male · Lv 1 · save slot 32, member 1',
                      level: 1,
                      sex: 'Male',
                    },
                  ],
                  characters: [
                    {
                      key: 'character_name:4',
                      label: 'Delita · variant 1',
                      available: true,
                      sex: 'Male',
                    },
                    {
                      key: 'character_name:32',
                      label: 'Wiegraf · variant 1',
                      available: true,
                      sex: 'Male',
                    },
                    {
                      key: 'character_name:30',
                      label: 'Agrias · variant 1',
                      available: false,
                      sex: 'Female',
                    },
                  ],
                },
              }
            : null,
      }),
    );
    render(<App />);
    await chooseSlot(32);
    fireEvent.click(
      await screen.findByRole('button', { name: 'Add character' }),
    );
    fireEvent.click(
      screen.getByRole('button', { name: 'Add a named character' }),
    );
    const picker = screen.getByRole('combobox', { name: 'Character' });
    expect(
      [...picker.querySelectorAll('option')].map(
        (option) => option.textContent,
      ),
    ).toEqual([
      'Agrias · variant 1 (already in party)',
      'Delita · variant 1',
      'Wiegraf · variant 1',
    ]);
    expect(screen.queryByText(/permanent-party position/)).toBeNull();
    fireEvent.change(screen.getByRole('combobox', { name: 'Character' }), {
      target: { value: 'character_name:32' },
    });
    expect(screen.queryByRole('combobox', { name: 'Starting Sex' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Add and edit' }));
    await waitFor(() => {
      expect(
        screen.queryByRole('dialog', { name: 'Add character' }),
      ).toBeNull();
    });
    expect(
      screen.getByRole('button', { name: 'Add character' }),
    ).toHaveProperty('disabled', false);
    await openReview();
    expect(screen.getByText('Wiegraf added')).toBeTruthy();
    expect(screen.queryByText(/Personal job where known/)).toBeNull();
    expect(screen.getByRole('heading', { name: 'Wiegraf' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Keep editing' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Bravery' }), {
      target: { value: '75' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add character' }));
    fireEvent.click(screen.getByRole('button', { name: 'Add and edit' }));
    await screen.findByRole('heading', { name: 'Delita' });
    fireEvent.change(screen.getByRole('textbox', { name: 'Bravery' }), {
      target: { value: '80' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Member.*Wiegraf/ }));
    fireEvent.click(
      screen.getByRole('button', { name: 'Remove pending character' }),
    );
    await waitFor(() => {
      expect(
        screen.queryByRole('button', { name: /Member.*Wiegraf/ }),
      ).toBeNull();
    });
    fireEvent.click(screen.getByRole('button', { name: /Member.*Delita/ }));
    expect(screen.getByRole('textbox', { name: 'Bravery' })).toHaveProperty(
      'value',
      '80',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Add character' }));
    fireEvent.change(screen.getByRole('combobox', { name: 'Character' }), {
      target: { value: 'character_name:32' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add and edit' }));
    await screen.findByRole('heading', { name: 'Wiegraf' });
    fireEvent.change(screen.getByRole('textbox', { name: 'Faith' }), {
      target: { value: '25' },
    });
    await openReview();
    expect(screen.getByText('Delita added')).toBeTruthy();
    expect(screen.getByText('Wiegraf added')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 31,
        operations: [
          {
            kind: 'add_named_from_unit',
            sourceSlot: 31,
            sourcePosition: 0,
            unitPosition: 14,
            characterKey: 'character_name:4',
          },
          {
            kind: 'add_named_from_unit',
            sourceSlot: 31,
            sourcePosition: 0,
            unitPosition: 13,
            characterKey: 'character_name:32',
          },
          { kind: 'faith', unitPosition: 13, value: '25' },
          { kind: 'bravery', unitPosition: 14, value: '80' },
        ],
      });
    });
  });

  test.each([
    ['Agrias', 'character_name:30'],
    ['Delita', 'character_name:4'],
  ])(
    'adds %s through the unified named flow without donor panels',
    async (label, key) => {
      mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
      mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
        Promise.resolve({
          ...readerResponse(
            request,
            request.manualSlotId === null
              ? null
              : readerFor(request.manualSlotId, request.requestId),
          ),
          occupiedManualSlots: [31],
          editContext:
            request.manualSlotId === 31
              ? {
                  ...unavailableAdditions,
                  snapshotGeneration: request.requestId,
                  manualSlotId: 31,
                  gil: 100,
                  genericCreation: { targetPosition: 13, donors: [] },
                  storyAddition: { targetPosition: 13, donors: [] },
                  guestAddition: { targetPosition: 13, donors: [] },
                  namedAddition: {
                    targetPosition: 13,
                    donors: [
                      {
                        sourceSlot: 31,
                        sourcePosition: 0,
                        unitPosition: 13,
                        label: 'Starting record',
                        level: 1,
                        sex: 'Male',
                      },
                      {
                        sourceSlot: 31,
                        sourcePosition: 1,
                        unitPosition: 13,
                        label: 'Female starting record',
                        level: 1,
                        sex: 'Female',
                      },
                    ],
                    characters: [
                      {
                        key: 'character_name:30',
                        label: 'Agrias',
                        available: true,
                        sex: 'Female',
                      },
                      {
                        key: 'character_name:4',
                        label: 'Delita',
                        available: true,
                        sex: 'Male',
                      },
                    ],
                  },
                }
              : null,
        }),
      );
      render(<App />);
      await chooseSlot(32);
      expect(screen.queryByText('Add a story character')).toBeNull();
      expect(screen.queryByText('Add a guest character')).toBeNull();
      fireEvent.click(
        await screen.findByRole('button', { name: 'Add character' }),
      );
      fireEvent.click(
        screen.getByRole('button', { name: 'Add a named character' }),
      );
      fireEvent.change(screen.getByRole('combobox', { name: 'Character' }), {
        target: { value: key },
      });
      expect(
        screen.queryByRole('combobox', { name: 'Starting Sex' }),
      ).toBeNull();
      fireEvent.click(screen.getByRole('button', { name: 'Add and edit' }));
      await waitFor(() => {
        expect(
          screen.queryByRole('dialog', { name: 'Add character' }),
        ).toBeNull();
      });
      expect(mocks.previewCharacter).toHaveBeenLastCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 31,
        operations: [
          {
            kind: 'add_named_from_unit',
            sourceSlot: 31,
            sourcePosition: label === 'Agrias' ? 1 : 0,
            unitPosition: 13,
            characterKey: key,
          },
        ],
      });
      await openReview();
      expect(screen.getByText(`${label} added`)).toBeTruthy();
      expect(
        screen.queryByText(/Experimental; story events are unchanged/),
      ).toBeNull();
    },
  );

  test('stages named owned and unowned item counts while preserving untouched high counts', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) => {
      const loaded = request.manualSlotId !== null;
      const document = loaded ? readerFor(33, request.requestId) : null;
      if (document) {
        document.inventory = known([
          {
            key: 1,
            item: known({
              id: 'item:1',
              label: { state: 'known', value: 'Potion' },
              description: { state: 'unknown' },
              asset_key: { state: 'unknown' },
            }),
            category: known('Consumables'),
            quantity: known(5),
          },
          {
            key: 2,
            item: known({
              id: 'item:2',
              label: { state: 'known', value: 'Eye Drops' },
              description: { state: 'unknown' },
              asset_key: { state: 'unknown' },
            }),
            category: known('Consumables'),
            quantity: known(0),
          },
          {
            key: 3,
            item: known({
              id: 'item:3',
              label: { state: 'known', value: 'Ether' },
              description: { state: 'unknown' },
              asset_key: { state: 'unknown' },
            }),
            category: known('Consumables'),
            quantity: known(255),
          },
          {
            key: 254,
            item: unknown(),
            category: unknown(),
            quantity: known(0),
          },
        ]);
      }
      return Promise.resolve({
        ...readerResponse(request, document),
        occupiedManualSlots: [33],
        editContext: loaded
          ? {
              ...unavailableAdditions,
              snapshotGeneration: request.requestId,
              manualSlotId: 33,
              gil: 313040,
            }
          : null,
      });
    });
    mocks.saveTransaction.mockResolvedValue({
      gil: 313040,
      backupCreated: true,
    });
    render(<App />);
    await chooseSlot(34);
    await openSection('Inventory');
    fireEvent.click(await screen.findByRole('button', { name: /^Ether/ }));
    const ether = screen.getByRole('textbox', { name: 'Quantity for Ether' });
    expect(ether).toHaveProperty('value', '255');
    fireEvent.click(screen.getByRole('button', { name: /Potion/ }));
    const potion = screen.getByRole('textbox', {
      name: 'Quantity for Potion',
    });
    fireEvent.change(potion, { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: 'All items' }));
    fireEvent.click(screen.getByRole('button', { name: /Eye Drops/ }));
    const eyeDrops = screen.getByRole('textbox', {
      name: 'Quantity for Eye Drops',
    });
    expect(
      screen.queryByRole('textbox', { name: /Quantity for .*254/ }),
    ).toBeNull();
    fireEvent.change(eyeDrops, { target: { value: '100' } });
    expect(
      screen.getByRole('button', { name: 'Review changes' }),
    ).toHaveProperty('disabled', true);
    fireEvent.change(eyeDrops, { target: { value: '99' } });
    await confirmChanges();
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 33,
        operations: [
          { kind: 'inventory_quantity', itemPosition: 1, quantity: '0' },
          { kind: 'inventory_quantity', itemPosition: 2, quantity: '99' },
        ],
      });
    });
  });

  test('stages one party member Bravery and Faith with independent bounds', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(33, request.requestId, [
                readerUnit(0, 'Ramza'),
                readerUnit(1, 'Roger'),
              ]),
        ),
        occupiedManualSlots: [33],
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 33,
                gil: 313040,
              },
      }),
    );
    mocks.saveTransaction.mockResolvedValue({
      gil: 313040,
      backupCreated: true,
    });
    render(<App />);
    await chooseSlot(34);
    const brave = await screen.findByRole('textbox', { name: 'Bravery' });
    const faith = screen.getByRole('textbox', { name: 'Faith' });
    fireEvent.change(brave, { target: { value: '101' } });
    expect(
      screen.getByRole('button', { name: 'Review changes' }),
    ).toHaveProperty('disabled', true);
    fireEvent.change(brave, { target: { value: '100' } });
    fireEvent.change(faith, { target: { value: '1' } });
    await openReview();
    expect(
      screen.getByRole('dialog', { name: 'Review changes' }).textContent,
    ).toContain('0 → 100');
    expect(
      screen.getByRole('dialog', { name: 'Review changes' }).textContent,
    ).toContain('0 → 1');
    fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 33,
        operations: [
          { kind: 'bravery', unitPosition: 0, value: '100' },
          { kind: 'faith', unitPosition: 0, value: '1' },
        ],
      });
    });
  });

  test('auto-fills Samurai Job EXP for level 5 and preserves spendable JP', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    const unit = readerUnit(0, 'Ramza');
    unit.jobs = known([
      {
        slot: 14,
        job: known({
          id: 'job:88',
          label: { state: 'known', value: 'Samurai' },
          description: { state: 'unknown' },
          asset_key: { state: 'unknown' },
        }),
        level: known(4),
        current_jp: known(88),
        total_jp: known(788),
      },
    ]);
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(8, request.requestId, [unit]),
        ),
        occupiedManualSlots: [8],
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 8,
                gil: 313040,
                jobOptions: [{ unitPosition: 0, jobSlots: [14] }],
                jobLevelTotalJp: [
                  0, 100, 200, 400, 700, 1100, 1600, 2200, 3000,
                ],
              },
      }),
    );
    mocks.saveTransaction.mockResolvedValue({
      gil: 313040,
      backupCreated: true,
    });
    render(<App />);
    await chooseSlot(9);
    fireEvent.click(await screen.findByRole('button', { name: 'Jobs' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Spendable JP' }), {
      target: { value: '99' },
    });
    expect(screen.getByRole('textbox', { name: 'Job EXP' })).toHaveProperty(
      'value',
      '788',
    );
    await confirmChanges();
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 8,
        operations: [
          {
            kind: 'job_progress',
            unitPosition: 0,
            jobSlot: 14,
            level: '4',
            currentJp: '99',
            totalJp: '788',
          },
        ],
      });
    });
    fireEvent.click(await screen.findByRole('button', { name: 'Jobs' }));
    await waitFor(() => {
      expect(
        screen.getByRole('textbox', { name: 'Spendable JP' }),
      ).toHaveProperty('value', '88');
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Spendable JP' }), {
      target: { value: '88' },
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Job level' }), {
      target: { value: '5' },
    });
    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'Job EXP' })).toHaveProperty(
        'value',
        '1100',
      );
    });
    expect(
      screen.getByRole('textbox', { name: 'Spendable JP' }),
    ).toHaveProperty('value', '88');
    await openReview();
    expect(
      screen.getByRole('dialog', { name: 'Review changes' }).textContent,
    ).toContain('4 → 5');
    fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 3,
        manualSlotId: 8,
        operations: [
          {
            kind: 'job_progress',
            unitPosition: 0,
            jobSlot: 14,
            level: '5',
            currentJp: '88',
            totalJp: '1100',
          },
        ],
      });
    });
  });

  test('saves Ninja JP above EXP and preserves JP through a level decrease', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    const unit = readerUnit(0, 'Ramza');
    unit.jobs = known([
      {
        slot: 15,
        job: known({
          id: 'job:89',
          label: { state: 'known', value: 'Ninja' },
          description: { state: 'unknown' },
          asset_key: { state: 'unknown' },
        }),
        level: known(7),
        current_jp: known(500),
        total_jp: known(2200),
      },
    ]);
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(8, request.requestId, [unit]),
        ),
        occupiedManualSlots: [8],
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 8,
                gil: 313040,
                jobOptions: [{ unitPosition: 0, jobSlots: [15] }],
                jobLevelTotalJp: [
                  0, 100, 200, 400, 700, 1100, 1600, 2200, 3000,
                ],
              },
      }),
    );
    mocks.saveTransaction.mockResolvedValue({
      gil: 313040,
      backupCreated: true,
    });
    render(<App />);
    await chooseSlot(9);
    fireEvent.click(await screen.findByRole('button', { name: 'Jobs' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Job level' }), {
      target: { value: '8' },
    });
    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'Job level' })).toHaveProperty(
        'value',
        '8',
      );
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Spendable JP' }), {
      target: { value: '4000' },
    });
    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'Job EXP' })).toHaveProperty(
        'value',
        '3000',
      );
    });
    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: 'Review changes' }),
      ).toHaveProperty('disabled', false);
    });
    await confirmChanges();
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 8,
        operations: [
          {
            kind: 'job_progress',
            unitPosition: 0,
            jobSlot: 15,
            level: '8',
            currentJp: '4000',
            totalJp: '3000',
          },
        ],
      });
    });
    fireEvent.click(await screen.findByRole('button', { name: 'Jobs' }));
    await waitFor(() => {
      expect(
        screen.getByRole('textbox', { name: 'Spendable JP' }),
      ).toHaveProperty('value', '500');
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Job level' }), {
      target: { value: '4' },
    });
    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'Job EXP' })).toHaveProperty(
        'value',
        '700',
      );
    });
    expect(
      screen.getByRole('textbox', { name: 'Spendable JP' }),
    ).toHaveProperty('value', '500');
    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: 'Review changes' }),
      ).toHaveProperty('disabled', false);
    });
    await confirmChanges();
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 3,
        manualSlotId: 8,
        operations: [
          {
            kind: 'job_progress',
            unitPosition: 0,
            jobSlot: 15,
            level: '4',
            currentJp: '500',
            totalJp: '700',
          },
        ],
      });
    });
    fireEvent.click(await screen.findByRole('button', { name: 'Jobs' }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Spendable JP' }), {
      target: { value: '65536' },
    });
    expect(
      screen.getByRole('button', { name: 'Review changes' }),
    ).toHaveProperty('disabled', true);
  });

  test('reviews linked unlearn and reaction unequip together', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    const unit = readerUnit(0, 'Ramza');
    unit.jobs = known([
      {
        slot: 13,
        job: known({
          id: 'job:87',
          label: { state: 'known', value: 'Dragoon' },
          description: { state: 'unknown' },
          asset_key: { state: 'unknown' },
        }),
        level: known(6),
        current_jp: known(3250),
        total_jp: known(1600),
      },
    ]);
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(33, request.requestId, [unit]),
        ),
        occupiedManualSlots: [33],
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: 33,
                gil: 313040,
                jobOptions: [{ unitPosition: 0, jobSlots: [13] }],
                abilityOptions: [
                  {
                    unitPosition: 0,
                    jobSlot: 13,
                    commandKey: 'command:21',
                    commandLabel: 'Jump',
                    members: [
                      {
                        key: 'ability:427',
                        label: 'Dragonheart',
                        learned: true,
                        equipped: true,
                        kind: 'reaction' as const,
                      },
                    ],
                  },
                ],
                loadoutOptions: [
                  {
                    unitPosition: 0,
                    secondaryChoices: [],
                    reactionChoices: [
                      { key: 'ability:427', label: 'Dragonheart' },
                    ],
                    supportChoices: [],
                    movementChoices: [],
                    slots: [
                      {
                        combatSet: null,
                        slot: 'reaction' as const,
                        currentKey: 'ability:427',
                      },
                    ],
                  },
                ],
                jobLevelTotalJp: [
                  0, 100, 200, 400, 700, 1100, 1600, 2200, 3000,
                ],
              },
      }),
    );
    mocks.saveTransaction.mockResolvedValue({
      gil: 313040,
      backupCreated: true,
    });
    render(<App />);
    await chooseSlot(34);
    fireEvent.click(await screen.findByRole('button', { name: 'Jobs' }));
    fireEvent.click(screen.getByRole('button', { name: 'reaction' }));
    fireEvent.click(screen.getByRole('checkbox', { name: /Dragonheart/ }));
    expect(
      screen.getByRole('button', { name: 'Review changes' }),
    ).toHaveProperty('disabled', true);
    fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
    fireEvent.change(
      screen.getByRole('combobox', { name: 'Active reaction' }),
      {
        target: { value: '' },
      },
    );
    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: 'Review changes' }),
      ).toHaveProperty('disabled', false);
    });
    await openReview();
    expect(
      screen.getByRole('dialog', { name: 'Review changes' }).textContent,
    ).toContain('Dragonheart');
    expect(
      screen.getByRole('dialog', { name: 'Review changes' }).textContent,
    ).toContain('Active reaction');
    fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 33,
        operations: [
          {
            kind: 'learned_ability',
            unitPosition: 0,
            jobSlot: 13,
            abilityKey: 'ability:427',
            learned: false,
          },
          {
            kind: 'equipped_slot',
            unitPosition: 0,
            combatSet: null,
            slot: 'reaction',
            valueKey: '',
          },
        ],
      });
    });
  });

  test('keeps changes across characters and sections until one confirmed save', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId, [
                readerUnit(0, 'Ramza'),
                readerUnit(1, 'Roger'),
              ]),
        ),
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: request.manualSlotId,
                gil: 313040,
              },
      }),
    );
    mocks.saveTransaction.mockResolvedValue({
      gil: 313041,
      backupCreated: true,
    });
    render(<App />);
    await chooseSlot(1);
    fireEvent.change(await screen.findByRole('textbox', { name: 'Bravery' }), {
      target: { value: '60' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Member 2 Roger/ }));
    fireEvent.change(screen.getByRole('textbox', { name: 'Faith' }), {
      target: { value: '40' },
    });
    await openSection('Game');
    fireEvent.change(screen.getByRole('textbox', { name: 'Gil' }), {
      target: { value: '313041' },
    });
    await openSection('Units');
    fireEvent.click(screen.getByRole('button', { name: /Member 1 Ramza/ }));
    expect(screen.getByRole('textbox', { name: 'Bravery' })).toHaveProperty(
      'value',
      '60',
    );
    expect(screen.getByText('3 pending changes')).toBeTruthy();
    await openReview();
    const dialog = screen.getByRole('dialog', { name: 'Review changes' });
    expect(dialog.textContent).toContain('Ramza');
    expect(dialog.textContent).toContain('Roger');
    expect(dialog.textContent).toContain('Gil');
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
    await waitFor(() => {
      expect(mocks.saveTransaction).toHaveBeenCalledWith({
        snapshotGeneration: 2,
        manualSlotId: 0,
        operations: [
          { kind: 'gil', value: '313041' },
          { kind: 'bravery', unitPosition: 0, value: '60' },
          { kind: 'faith', unitPosition: 1, value: '40' },
        ],
      });
    });
  });

  test('requires a discard decision before switching slots with pending edits', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId),
        ),
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: request.manualSlotId,
                gil: 100,
              },
      }),
    );
    render(<App />);
    await chooseSlot(1);
    await openSection('Game');
    fireEvent.change(screen.getByRole('textbox', { name: 'Gil' }), {
      target: { value: '101' },
    });
    await chooseSlot(2);
    expect(
      screen.getByRole('dialog', { name: 'Discard pending changes?' }),
    ).toBeTruthy();
    expect(mocks.loadReader).toHaveBeenCalledTimes(2);
    fireEvent.click(screen.getByRole('button', { name: 'Keep editing' }));
    expect(screen.getByRole('textbox', { name: 'Gil' })).toHaveProperty(
      'value',
      '101',
    );
    await chooseSlot(2);
    fireEvent.click(screen.getByRole('button', { name: 'Discard changes' }));
    expect(mocks.loadReader).toHaveBeenLastCalledWith({
      requestId: 3,
      manualSlotId: 1,
    });
  });

  test('switching slots replaces general unit details without old first-record previews', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) => {
      if (request.manualSlotId === null) {
        return Promise.resolve(response(request.requestId, [25, 26, 27], null));
      }
      const name =
        request.manualSlotId === 26
          ? 'First slot unit'
          : request.manualSlotId === 27
            ? 'Second slot unit'
            : 'Third slot unit';
      return Promise.resolve({
        ...readerResponse(
          request,
          readerFor(request.manualSlotId, request.requestId, [
            readerUnit(0, name),
          ]),
        ),
        occupiedManualSlots: [25, 26, 27],
      });
    });
    render(<App />);
    await chooseSlot(27);
    expect(
      (await screen.findAllByText('First slot unit')).length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText('Slot 27').length).toBeGreaterThan(0);
    expect(screen.queryByText('First-record Mettle preview')).toBeNull();
    await chooseSlot(28);
    expect(
      (await screen.findAllByText('Second slot unit')).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText('First slot unit')).toBeNull();
    await chooseSlot(26);
    expect(
      (await screen.findAllByText('Third slot unit')).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText('Second slot unit')).toBeNull();
  });

  test('shows loading while a request is pending and preserves dialog cancellation', async () => {
    let resolveLoad!: (value: LoadReaderResponse) => void;
    const pendingLoad = new Promise<LoadReaderResponse>((resolve) => {
      resolveLoad = resolve;
    });
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.chooseSave.mockResolvedValue(null);
    mocks.loadReader.mockReturnValue(pendingLoad);
    render(<App />);

    await waitFor(() => {
      expect(mocks.loadReader).toHaveBeenCalledTimes(1);
    });
    fireEvent.click(await screen.findByRole('button', { name: 'Choose save' }));
    await waitFor(() => {
      expect(mocks.loadReader).toHaveBeenCalledTimes(1);
    });
    expect(
      await screen.findByRole('status', { name: 'Loading save' }),
    ).toBeTruthy();
    expect(screen.queryByText(/Loading save/)).toBeNull();
    expect(
      screen.getByRole('button', { name: 'Refresh slots' }),
    ).toHaveProperty('disabled', true);

    const latestRequest: unknown = mocks.loadReader.mock.lastCall?.[0];
    if (
      typeof latestRequest !== 'object' ||
      latestRequest === null ||
      !('requestId' in latestRequest) ||
      typeof latestRequest.requestId !== 'number'
    ) {
      throw new Error('Missing refreshed request');
    }
    resolveLoad(response(latestRequest.requestId, [], null));
    expect(
      await screen.findByRole('heading', { name: 'No occupied manual slots' }),
    ).toBeTruthy();
  });

  test('labels unsupported and corrupt native failures without rendering raw diagnostics', async () => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockRejectedValueOnce(loadError).mockRejectedValueOnce({
      ...loadError,
      category: 'corrupt',
      code: 'payload_crc',
    });
    render(<App />);

    expect((await screen.findByRole('alert')).textContent).toContain(
      'This save format is unsupported.',
    );
    expect(screen.getByRole('alert').textContent).toContain(
      'Code: unsupported_structure.',
    );

    fireEvent.click(screen.getByRole('button', { name: 'Refresh slots' }));
    expect((await screen.findByRole('alert')).textContent).toContain(
      'The selected file could not be safely parsed.',
    );
    expect(screen.getByRole('alert').textContent).toContain(
      'Code: payload_crc.',
    );
  });

  test('uses the most recent slot response when an earlier response arrives late', async () => {
    let resolveOld!: (value: LoadReaderResponse) => void;
    const oldResponse = new Promise<LoadReaderResponse>((resolve) => {
      resolveOld = resolve;
    });
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader
      .mockResolvedValueOnce(response(1, [2, 3], null))
      .mockReturnValueOnce(oldResponse)
      .mockResolvedValueOnce(response(3, [2, 3], saveFor(3)));
    render(<App />);

    await chooseSlot(3);
    await chooseSlot(4);

    expect((await screen.findAllByText('Slot 4')).length).toBeGreaterThan(0);
    resolveOld(response(2, [2, 3], saveFor(2)));
    await waitFor(() => {
      expect(screen.getAllByText('Slot 4').length).toBeGreaterThan(0);
    });
    expect(screen.queryByText('99')).toBeNull();
  });
});

describe('named reader roster', () => {
  beforeEach(() => {
    mocks.getSaveSelection.mockResolvedValue({ state: 'selected' });
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId),
        ),
      ),
    );
  });

  test('pending levels unlock jobs immediately and prerequisite-breaking decreases are rejected', async () => {
    const member = readerUnit(0, 'Ramza');
    member.jobs = known([
      {
        slot: 7,
        job: known(jobReference(81, 'Time Mage')),
        level: known(2),
        current_jp: known(397),
        total_jp: known(200),
      },
      {
        slot: 8,
        job: known(jobReference(82, 'Summoner')),
        level: known(1),
        current_jp: known(767),
        total_jp: known(100),
      },
    ]);
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(0, request.requestId, [member]),
        ),
        editContext: {
          ...unavailableAdditions,
          snapshotGeneration: request.requestId,
          manualSlotId: 0,
          gil: 100,
          jobOptions: [
            {
              unitPosition: 0,
              jobSlots: [7],
              lockedJobs: [
                {
                  jobSlot: 8,
                  requires: [{ jobSlot: 7, level: 3, currentLevel: 2 }],
                },
              ],
            },
          ],
          jobLevelTotalJp: [0, 100, 200, 400, 700, 1100, 1600, 2200, 3000],
          abilityOptions: [
            {
              unitPosition: 0,
              jobSlot: 8,
              commandKey: 'command:8',
              commandLabel: 'Summon',
              members: [
                {
                  key: 'ability:1',
                  label: 'Synthetic summon',
                  learned: false,
                  equipped: false,
                  kind: 'action',
                },
              ],
            },
          ],
        },
      }),
    );
    mocks.previewJobProgress.mockResolvedValueOnce({
      jobOptions: { unitPosition: 0, jobSlots: [7, 8], lockedJobs: [] },
      invalidatedSlots: [],
    });
    render(<App />);
    await openRoster();
    fireEvent.click(screen.getByRole('button', { name: 'Jobs' }));
    expect(
      screen
        .getByRole('button', { name: /Summoner/ })
        .getAttribute('aria-disabled'),
    ).toBe('true');
    fireEvent.change(screen.getByRole('textbox', { name: 'Job level' }), {
      target: { value: '3' },
    });
    await waitFor(() => {
      expect(screen.getByRole('textbox', { name: 'Job EXP' })).toHaveProperty(
        'value',
        '400',
      );
    });
    expect(
      screen
        .getByRole('button', { name: /Summoner/ })
        .getAttribute('aria-disabled'),
    ).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /Summoner/ }));
    expect(
      screen.getByRole('checkbox', { name: /Synthetic summon/ }),
    ).toHaveProperty('disabled', false);
    fireEvent.change(screen.getByRole('textbox', { name: 'Spendable JP' }), {
      target: { value: '900' },
    });
    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: 'Review changes' }),
      ).toHaveProperty('disabled', false);
    });
    fireEvent.click(screen.getByRole('button', { name: /Time Mage/ }));
    mocks.previewJobProgress.mockResolvedValueOnce({
      jobOptions: { unitPosition: 0, jobSlots: [7], lockedJobs: [] },
      invalidatedSlots: [8],
    });
    fireEvent.change(screen.getByRole('textbox', { name: 'Job level' }), {
      target: { value: '1' },
    });
    expect((await screen.findByRole('alert')).textContent).toContain(
      'would block Summoner',
    );
    expect(screen.getByRole('textbox', { name: 'Job level' })).toHaveProperty(
      'value',
      '3',
    );
    expect(mocks.saveTransaction).not.toHaveBeenCalled();
  });

  test('inactive story records are excluded while active duplicate names remain distinct', async () => {
    const inactive = readerUnit(50, 'Agrias', 11);
    inactive.membership = known('inactive');
    const party = readerUnit(3, 'Agrias', 25);
    const other = readerUnit(4, 'Agrias', 8);
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(0, request.requestId, [party, inactive, other]),
        ),
      ),
    );
    render(<App />);
    await openRoster();
    expect(
      screen.getByRole('button', { name: /Member 1 Agrias.*Level 25/ }),
    ).toBeTruthy();
    expect(
      screen.getByRole('button', { name: /Member 2 Agrias.*Level 8/ }),
    ).toBeTruthy();
    expect(
      screen.queryByRole('button', { name: /Agrias.*Level 11/ }),
    ).toBeNull();
    expect(screen.queryByLabelText('Current gil')).toBeNull();
    expect(screen.queryByText('More saved details')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Abilities' })).toBeNull();
  });

  test('equipment exposes primary and secondary action members and inventory shares item effects', async () => {
    const member = readerUnit(0, 'Ramza');
    const command = {
      id: 'command:1',
      label: { state: 'known' as const, value: 'Arts' },
      description: { state: 'unknown' as const },
      asset_key: { state: 'unknown' as const },
    };
    member.abilities.primary_command = known(command);
    member.equipment.head = known({
      id: 'item:1',
      label: { state: 'known', value: 'Hat' },
      description: { state: 'unknown' },
      asset_key: { state: 'unknown' },
    });
    const reader = readerFor(0, 2, [member]);
    reader.item_details = {
      'item:1': ['+2 magic attack', 'Chance to inflict on hit: Stop'],
    };
    reader.inventory = known([
      {
        key: 1,
        item: member.equipment.head,
        quantity: known(1),
        category: known('Headgear'),
      },
    ]);
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null ? null : reader,
        ),
        editContext: {
          ...unavailableAdditions,
          snapshotGeneration: request.requestId,
          manualSlotId: 0,
          gil: 100,
          abilityOptions: [
            {
              unitPosition: 0,
              jobSlot: 0,
              commandKey: 'command:1',
              commandLabel: 'Arts',
              members: [
                {
                  key: 'ability:1',
                  label: 'Action one',
                  learned: true,
                  equipped: false,
                  kind: 'action',
                },
                {
                  key: 'ability:2',
                  label: 'Reaction one',
                  learned: true,
                  equipped: false,
                  kind: 'reaction',
                },
              ],
            },
          ],
          loadoutOptions: [
            {
              unitPosition: 0,
              secondaryChoices: [{ key: 'command:1', label: 'Arts' }],
              reactionChoices: [],
              supportChoices: [],
              movementChoices: [],
              slots: [
                {
                  combatSet: null,
                  slot: 'secondary_command',
                  currentKey: 'command:1',
                },
              ],
            },
          ],
        },
      }),
    );
    render(<App />);
    await openRoster();
    fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
    const hatHint = screen.getByText('Hat').closest('.workspace-hint');
    if (!hatHint) throw new Error('Missing equipment tooltip');
    fireEvent.focus(hatHint);
    expect(screen.getByRole('tooltip').textContent).toContain(
      '+2 magic attack',
    );
    expect(screen.getByRole('tooltip').textContent).toContain(
      'Chance to inflict on hit: Stop',
    );
    fireEvent.blur(hatHint);
    fireEvent.click(
      screen.getByRole('button', { name: 'Primary command: Arts' }),
    );
    expect(
      screen.getByRole('dialog', { name: 'Arts actions' }).textContent,
    ).toContain('Action one');
    expect(screen.getByRole('dialog').textContent).not.toContain(
      'Reaction one',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    fireEvent.click(
      screen.getByRole('button', { name: 'View secondary actions' }),
    );
    expect(
      screen.getByRole('dialog', { name: 'Arts actions' }).textContent,
    ).toContain('Action one');
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    fireEvent.click(screen.getByRole('button', { name: 'Inventory' }));
    expect(screen.getByText('Chance to inflict on hit: Stop')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Add new' })).toBeNull();
    expect(screen.getByRole('button', { name: 'All items' })).toBeTruthy();
  });

  test('hides incompatible and unnamed jobs and explains locked requirements', async () => {
    const member = readerUnit(0, 'Ramza');
    member.stored.sex = known('Male');
    member.current_job = known(jobReference(76, 'Knight'));
    member.jobs = known([
      {
        slot: 2,
        job: known(jobReference(76, 'Knight')),
        level: known(2),
        current_jp: known(50),
        total_jp: known(200),
      },
      {
        slot: 3,
        job: known(jobReference(77, 'Archer')),
        level: known(0),
        current_jp: known(0),
        total_jp: known(0),
      },
      {
        slot: 17,
        job: known(jobReference(91, 'Bard')),
        level: known(0),
        current_jp: known(0),
        total_jp: known(0),
      },
      {
        slot: 18,
        job: known(jobReference(92, 'Dancer')),
        level: known(0),
        current_jp: known(0),
        total_jp: known(0),
      },
      {
        slot: 20,
        job: unknown<CatalogueRef>(),
        level: known(0),
        current_jp: known(0),
        total_jp: known(0),
      },
    ]);
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId, [member]),
        ),
        editContext:
          request.manualSlotId === null
            ? null
            : {
                ...unavailableAdditions,
                snapshotGeneration: request.requestId,
                manualSlotId: request.manualSlotId,
                gil: 100,
                jobOptions: [
                  {
                    unitPosition: 0,
                    jobSlots: [2, 17],
                    lockedJobs: [
                      {
                        jobSlot: 3,
                        requires: [{ jobSlot: 2, level: 4, currentLevel: 2 }],
                      },
                    ],
                  },
                ],
              },
      }),
    );
    render(<App />);
    await openRoster();
    fireEvent.click(screen.getByRole('button', { name: 'Jobs' }));
    expect(screen.getByRole('button', { name: /Bard/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Dancer/ })).toBeNull();
    expect(screen.queryByText('Unknown job')).toBeNull();
    const locked = screen.getByRole('button', { name: /Archer/ });
    expect(locked.getAttribute('aria-disabled')).toBe('true');
    fireEvent.click(locked);
    expect(
      screen.getByRole('complementary', { name: 'Job requirements' }),
    ).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByText('Level 4 · currently 2')).toBeTruthy();
    expect(screen.getByText('Job EXP')).toBeTruthy();
  });

  test('keeps saved ability loadouts visible for view-only units', async () => {
    const member = readerUnit(0, 'Guest');
    member.membership = known('guest');
    member.abilities.primary_command = known(
      jobReference(10, 'Primary actions'),
    );
    member.abilities.secondary_command = known(
      jobReference(11, 'Secondary actions'),
    );
    member.abilities.reaction = known(jobReference(12, 'Saved reaction'));
    member.abilities.support = known(jobReference(13, 'Saved support'));
    member.abilities.movement = known(jobReference(14, 'Saved movement'));
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId, [member]),
        ),
      ),
    );
    render(<App />);
    await openRoster();
    fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
    expect(
      screen.getByRole('button', { name: /Primary actions/ }),
    ).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Secondary actions' }),
    ).toBeTruthy();
    for (const label of ['Saved reaction', 'Saved support', 'Saved movement']) {
      expect(screen.getByText(label)).toBeTruthy();
    }
    expect(screen.queryByRole('combobox', { name: /Active/ })).toBeNull();
  });

  test('shows Dancer and hides Bard for a female unit', async () => {
    const member = readerUnit(0, 'Lavian');
    member.stored.sex = known('Female');
    member.jobs = known(
      [
        [17, 91, 'Bard'],
        [18, 92, 'Dancer'],
      ].map(([slot, id, label]) => ({
        slot: Number(slot),
        job: known(jobReference(Number(id), String(label))),
        level: known(0),
        current_jp: known(0),
        total_jp: known(0),
      })),
    );
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId, [member]),
        ),
      ),
    );
    render(<App />);
    await openRoster();
    fireEvent.click(screen.getByRole('button', { name: 'Jobs' }));
    expect(screen.getByRole('button', { name: /Dancer/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Bard/ })).toBeNull();
  });

  test('shows saved names and jobs without a roster visibility setting', async () => {
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) => {
      const unit = readerUnit(0, 'Étoile 太陽');
      return Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId, [unit]),
        ),
      );
    });
    render(<App />);
    await openRoster();
    expect(screen.queryByLabelText('Roster visibility')).toBeNull();
    expect(screen.getAllByText('Étoile 太陽').length).toBeGreaterThan(0);
    expect(screen.getAllByText(/Synthetic job/)[0]).toBeTruthy();
    const member = await screen.findByRole('button', {
      name: /Member 1 Étoile 太陽 Synthetic job Level 12/,
    });
    fireEvent.click(member);
    expect(screen.queryByText('More saved details')).toBeNull();
    expect(
      screen.getByRole('heading', { name: 'Character details' }),
    ).toBeTruthy();
    expect(screen.queryByText('job:78')).toBeNull();
  });

  test('duplicate long Unicode names stay separate and support keyboard focus and selection', async () => {
    const duplicate = 'Étoile 太陽 '.repeat(8).trim();
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, request.requestId, [
                readerUnit(3, duplicate, 12),
                readerUnit(9, duplicate, 27),
              ]),
        ),
      ),
    );
    render(<App />);
    await openRoster();
    const first = await screen.findByRole('button', {
      name: /Member 1 Étoile.*Level 12/,
    });
    const second = screen.getByRole('button', {
      name: /Member 2 Étoile.*Level 27/,
    });
    first.focus();
    fireEvent.click(first);
    expect(first.getAttribute('aria-pressed')).toBe('true');
    fireEvent.keyDown(first, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(second);
    expect(second.getAttribute('aria-pressed')).toBe('true');
    expect(first.getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByText(/Synthetic job.*Level 27/)).toBeTruthy();
    fireEvent.keyDown(second, { key: 'Home' });
    expect(document.activeElement).toBe(first);
    expect(first.getAttribute('aria-pressed')).toBe('true');
  });

  test('reload refreshes data while preserving the selected unit panel', async () => {
    render(<App />);
    await openRoster();
    fireEvent.click(
      await screen.findByRole('button', { name: /Member 1 Étoile/ }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
    expect(screen.getByRole('heading', { name: 'Equipment' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    expect(screen.queryByRole('tabpanel')).toBeNull();
    const fresh = await screen.findByRole('button', {
      name: /Member 1 Étoile/,
    });
    expect(fresh.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('heading', { name: 'Equipment' })).toBeTruthy();
  });

  test('a newer slot request rejects a late response and retains saved labels', async () => {
    let resolveFull!: (value: LoadReaderResponse) => void;
    let fullRequest: LoadReaderRequest | undefined;
    const slowFull = new Promise<LoadReaderResponse>((resolve) => {
      resolveFull = resolve;
    });
    render(<App />);
    await openRoster();
    fireEvent.click(
      await screen.findByRole('button', { name: /Member 1 Étoile/ }),
    );
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) => {
      if (!fullRequest) {
        fullRequest = request;
        return slowFull;
      }
      return Promise.resolve(
        readerResponse(
          request,
          readerFor(request.manualSlotId ?? 0, request.requestId),
        ),
      );
    });
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    expect(screen.queryByText('Étoile 太陽')).toBeNull();
    await chooseSlot(2);
    await screen.findByRole('heading', { name: 'Units' });
    expect(screen.getAllByText(/Synthetic job/)[0]).toBeTruthy();
    if (!fullRequest) throw new Error('Missing full request');
    resolveFull(
      readerResponse(
        fullRequest,
        readerFor(0, fullRequest.requestId, [
          readerUnit(0, 'Late private label', 99),
        ]),
      ),
    );
    await waitFor(() => {
      expect(screen.queryByText('Late private label')).toBeNull();
    });
    expect(screen.queryByText('More saved details')).toBeNull();
    expect(screen.getAllByText(/Synthetic job/)[0]).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Level 99/ })).toBeNull();
  });

  test('missing catalogue preserves saved previews and gives an actionable reload path', async () => {
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve({
        ...readerResponse(request, null),
        readerError:
          request.manualSlotId === null
            ? null
            : {
                ...loadError,
                category: 'resource',
                code: 'reader_catalogue_missing',
              },
      }),
    );
    render(<App />);
    await chooseSlot(1);
    expect(
      await screen.findByRole('heading', { name: 'Named roster unavailable' }),
    ).toBeTruthy();
    expect(screen.getByRole('alert').textContent).toContain(
      'complete resources folder beside the app, then reload this slot',
    );
    expect(screen.getByRole('heading', { name: 'Game' })).toBeTruthy();
    expect(screen.queryByText('Diagnostics')).toBeNull();
    expect(screen.getByRole('button', { name: 'Reload slot 1' })).toBeTruthy();
    expect(screen.queryByRole('heading', { name: 'Units' })).toBeNull();
  });

  test.each([
    ['unsupported', 'reader_catalogue_profile'],
    ['corrupt', 'reader_catalogue_invalid'],
    ['corrupt', 'reader_catalogue_noncanonical'],
  ] as const)(
    'catalogue %s failure %s directs repair without discarding previews',
    async (category, code) => {
      mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
        Promise.resolve({
          ...readerResponse(request, null),
          readerError:
            request.manualSlotId === null
              ? null
              : { ...loadError, category, code },
        }),
      );
      render(<App />);
      await chooseSlot(1);
      expect(
        await screen.findByRole('heading', {
          name: 'Named roster unavailable',
        }),
      ).toBeTruthy();
      const message = screen.getByRole('alert').textContent;
      expect(message).toContain(
        'Replace or regenerate the supported offline catalogue, then reload this slot.',
      );
      expect(message).not.toContain('game has finished writing');
      expect(message).not.toContain('choose another save');
      expect(screen.getByRole('heading', { name: 'Game' })).toBeTruthy();
      expect(screen.queryByText('Diagnostics')).toBeNull();
      expect(
        screen.getByRole('combobox', { name: 'Manual slot' }),
      ).toHaveProperty('value', '0');
      expect(
        screen.getByRole('button', { name: 'Reload slot 1' }),
      ).toBeTruthy();
    },
  );

  test('empty roster and unknown names never invent a name or numeric substitute', async () => {
    const unresolved = readerUnit(8, 'Must not display');
    unresolved.name = unknown();
    unresolved.current_job = unknown();
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(
                request.manualSlotId,
                request.requestId,
                request.manualSlotId === 1 ? [] : [unresolved],
              ),
        ),
      ),
    );
    render(<App />);
    await openRoster();
    fireEvent.click(
      await screen.findByRole('button', {
        name: /Member 1 Unknown/,
      }),
    );
    expect(screen.queryByText('Must not display')).toBeNull();
    expect(screen.queryByText('job:78')).toBeNull();
    await chooseSlot(2);
    expect(
      await screen.findByText('No party members in this slot.'),
    ).toBeTruthy();
    expect(screen.queryByRole('tabpanel')).toBeNull();
  });

  test('a current request with mismatched slot or regressed identity is rejected', async () => {
    render(<App />);
    await openRoster();
    await screen.findByRole('button', { name: /Member 1 Étoile/ });
    mocks.loadReader.mockImplementationOnce((request: LoadReaderRequest) =>
      Promise.resolve(readerResponse(request, readerFor(1, 20))),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    await screen.findByRole('alert');
    expect(screen.queryByRole('heading', { name: 'Units' })).toBeNull();
    mocks.loadReader.mockImplementationOnce((request: LoadReaderRequest) =>
      Promise.resolve(readerResponse(request, readerFor(0, 1))),
    );
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    await screen.findByRole('alert');
    expect(screen.queryByRole('heading', { name: 'Units' })).toBeNull();
  });

  test('choosing another save invalidates a pending reader response', async () => {
    let resolveOld!: (value: LoadReaderResponse) => void;
    let oldRequest: LoadReaderRequest | undefined;
    const pending = new Promise<LoadReaderResponse>((resolve) => {
      resolveOld = resolve;
    });
    render(<App />);
    await openRoster();
    await screen.findByRole('button', { name: /Member 1 Étoile/ });
    mocks.loadReader.mockImplementationOnce((request: LoadReaderRequest) => {
      oldRequest = request;
      return pending;
    });
    mocks.chooseSave.mockResolvedValue({ state: 'selected' });
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    fireEvent.click(screen.getByRole('button', { name: 'Choose save' }));
    await screen.findByRole('button', { name: 'Refresh slots' });
    if (!oldRequest) throw new Error('Missing old request');
    resolveOld(
      readerResponse(
        oldRequest,
        readerFor(0, 99, [readerUnit(0, 'Previous save name')]),
      ),
    );
    await waitFor(() => {
      expect(screen.queryByText('Previous save name')).toBeNull();
    });
    expect(screen.queryByRole('heading', { name: 'Units' })).toBeNull();
  });

  test('late named slot responses cannot replace the selected slot even with newer generations', async () => {
    let resolveOld!: (value: LoadReaderResponse) => void;
    let oldRequest: LoadReaderRequest | undefined;
    const old = new Promise<LoadReaderResponse>((resolve) => {
      resolveOld = resolve;
    });
    render(<App />);
    await openRoster();
    await screen.findByRole('button', { name: /Member 1 Étoile/ });
    mocks.loadReader.mockImplementationOnce((request: LoadReaderRequest) => {
      oldRequest = request;
      return old;
    });
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    await chooseSlot(2);
    await screen.findByRole('button', { name: /Member 1 Étoile/ });
    expect(screen.getAllByText('Slot 2').length).toBeGreaterThan(0);
    if (!oldRequest) throw new Error('Missing old request');
    resolveOld(
      readerResponse(
        oldRequest,
        readerFor(0, 999, [readerUnit(0, 'Wrong slot label', 99)]),
      ),
    );
    await waitFor(() => {
      expect(screen.getAllByText('Slot 2').length).toBeGreaterThan(0);
    });
    expect(
      screen.queryByRole('button', { name: /Wrong slot label/ }),
    ).toBeNull();
  });

  test('resource token cannot change inside an unchanged resource generation', async () => {
    mocks.loadReader.mockImplementation((request: LoadReaderRequest) =>
      Promise.resolve(
        readerResponse(
          request,
          request.manualSlotId === null
            ? null
            : readerFor(request.manualSlotId, 44),
        ),
      ),
    );
    render(<App />);
    await openRoster();
    await screen.findByRole('button', { name: /Member 1 Étoile/ });
    mocks.loadReader.mockImplementationOnce((request: LoadReaderRequest) => {
      const changed = readerFor(0, 45);
      changed.identity.resource_generation = 44;
      changed.identity.resource_token = {
        state: 'known',
        value: 'b'.repeat(64),
      };
      return Promise.resolve(readerResponse(request, changed));
    });
    fireEvent.click(screen.getByRole('button', { name: 'Reload slot 1' }));
    await screen.findByRole('alert');
    expect(screen.queryByRole('heading', { name: 'Units' })).toBeNull();
  });
});
