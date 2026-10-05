import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { beforeEach, expect, test, vi } from 'vitest';
import { draftContext, draftDocument, knownFact } from './draft-test-fixture';
import type {
  previewBaseStat,
  previewDraft,
  saveTransaction,
} from './reader-ipc';
const mocks = vi.hoisted(() => ({
  preview: vi.fn<typeof previewDraft>(),
  compute: vi.fn<typeof previewBaseStat>(),
  save: vi.fn<typeof saveTransaction>(),
}));
vi.mock('./reader-ipc', async (original) => ({
  ...(await original<typeof import('./reader-ipc')>()),
  previewDraft: mocks.preview,
  previewBaseStat: mocks.compute,
  saveTransaction: mocks.save,
}));
import { SaveWorkspace } from './SaveWorkspace';

beforeEach(() => {
  mocks.save.mockReset().mockResolvedValue({ gil: 100, backupCreated: true });
  mocks.compute.mockReset().mockImplementation(({ value, equipmentBonus }) =>
    Promise.resolve({
      storedBase: value === null ? 8_847_360 : 9_830_400,
      base: knownFact(value === null ? 540 : 600),
      total: knownFact(
        value === null
          ? equipmentBonus === 120
            ? 660
            : 540
          : equipmentBonus === 120
            ? 720
            : 600,
      ),
    }),
  );
  mocks.preview.mockReset().mockImplementation(({ transaction }) => {
    const document = draftDocument();
    if (document.roster.value.state !== 'known') throw new Error('Fixture');
    const unit = document.roster.value.value[0];
    if (!unit?.effective.breakdown?.hp) throw new Error('Fixture');
    const edited = transaction.operations.some(
      (operation) => operation.kind === 'base_stat' && operation.stat === 'hp',
    );
    const armour = transaction.operations.some(
      (operation) =>
        operation.kind === 'gear' &&
        operation.slot === 'body' &&
        operation.itemKey !== '',
    );
    // Fixed response fixtures; stat arithmetic is tested in Rust.
    unit.stored.bases.hp = knownFact(edited ? 9_830_400 : 8_847_360);
    unit.effective.hp = knownFact(
      edited ? (armour ? 720 : 600) : armour ? 660 : 540,
    );
    unit.effective.breakdown.hp = {
      ...unit.effective.breakdown.hp,
      base: knownFact(edited ? 600 : 540),
      equipment_bonus: knownFact(armour ? 120 : 0),
    };
    return Promise.resolve({ reader: document, solvedBase: null });
  });
});

function openWorkspace(reader = draftDocument()) {
  reader.inventory = knownFact([
    {
      key: 150,
      item: knownFact({
        id: 'item:150',
        label: { state: 'known', value: 'Test armour' },
        description: { state: 'unknown' },
        asset_key: { state: 'unknown' },
      }),
      category: knownFact('armor'),
      quantity: knownFact(0),
    },
  ]);
  render(
    <SaveWorkspace
      reader={reader}
      context={draftContext()}
      onDirtyChange={vi.fn()}
      onReload={vi.fn().mockResolvedValue(undefined)}
      manualSlotId={0}
      initialView={undefined}
      defaultSection="units"
      onViewChange={vi.fn()}
    />,
  );
}

function firstUnit(document: ReturnType<typeof draftDocument>) {
  if (
    document.roster.value.state !== 'known' ||
    !document.roster.value.value[0]
  )
    throw new Error('Fixture');
  return document.roster.value.value[0];
}

test('PA errors are immediate, typing defers calculation, and a committed input visibly calculates only that stat', async () => {
  const document = draftDocument();
  const unit = firstUnit(document);
  if (!unit.effective.breakdown?.physical_attack) throw new Error('Fixture');
  unit.effective.physical_attack = knownFact(15);
  unit.effective.breakdown.physical_attack.equipment_bonus = knownFact(1);
  openWorkspace(document);
  fireEvent.change(screen.getByRole('textbox', { name: 'PA' }), {
    target: { value: '555' },
  });
  expect(screen.getByRole('alert').textContent).toBe('Enter PA from 1 to 99.');
  expect(mocks.preview).not.toHaveBeenCalled();
  expect(mocks.compute).not.toHaveBeenCalled();
  let finish:
    ((value: Awaited<ReturnType<typeof previewBaseStat>>) => void) | undefined;
  mocks.compute.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  fireEvent.change(screen.getByRole('textbox', { name: 'PA' }), {
    target: { value: '55' },
  });
  expect(mocks.compute).not.toHaveBeenCalled();
  expect(screen.getByText('Waiting to apply')).toBeTruthy();
  fireEvent.keyDown(screen.getByRole('textbox', { name: 'PA' }), {
    key: 'Enter',
  });
  await waitFor(() => {
    expect(mocks.compute).toHaveBeenCalledTimes(1);
  });
  expect(screen.getAllByText('Calculating stats\u2026')).toHaveLength(2);
  expect(screen.getByRole('button', { name: 'Review changes' })).toHaveProperty(
    'disabled',
    true,
  );
  await act(async () => {
    finish?.({
      storedBase: 901_120,
      base: knownFact(55),
      total: knownFact(56),
    });
    await Promise.resolve();
  });
  await waitFor(() => {
    expect(
      screen.getByRole('button', { name: 'Review changes' }),
    ).toHaveProperty('disabled', false);
  });
  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.getByRole('textbox', { name: 'PA' })).toHaveProperty(
    'value',
    '55',
  );
  expect(screen.getByText('Equipment +1 \u00b7 Total 56')).toBeTruthy();
  expect(mocks.preview).not.toHaveBeenCalled();
});

test('MA base 99 plus equipment 4 displays and reviews total 99', async () => {
  const document = draftDocument();
  const unit = firstUnit(document);
  if (!unit.effective.breakdown?.magical_attack) throw new Error('Fixture');
  unit.effective.magical_attack = knownFact(15);
  unit.effective.breakdown.magical_attack.equipment_bonus = knownFact(4);
  mocks.compute.mockResolvedValueOnce({
    storedBase: 1_622_016,
    base: knownFact(99),
    total: knownFact(99),
  });
  openWorkspace(document);
  expect(screen.getByRole('textbox', { name: 'MA' })).toHaveProperty(
    'value',
    '11',
  );
  fireEvent.change(screen.getByRole('textbox', { name: 'MA' }), {
    target: { value: '99' },
  });
  fireEvent.blur(screen.getByRole('textbox', { name: 'MA' }));
  await waitFor(() => {
    expect(screen.getByText('Equipment +4 \u00b7 Total 99')).toBeTruthy();
  });
  expect(screen.getByRole('textbox', { name: 'MA' })).toHaveProperty(
    'value',
    '99',
  );
  expect(screen.getByText('Saved base 11 \u2192 Base 99')).toBeTruthy();
  expect(mocks.compute).toHaveBeenCalledWith({
    stat: 'magical_attack',
    previousBase: 180_224,
    value: 99,
    jobMultiplier: 100,
    equipmentBonus: 4,
  });
  expect(mocks.preview).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Review changes' }));
  await waitFor(() => {
    expect(
      screen.getByRole('dialog', { name: 'Review changes' }).textContent,
    ).toContain('15 \u2192 99');
  });
});

test('gear changes the total while keeping the entered base, and reset restores the saved base with staged gear', async () => {
  openWorkspace();
  fireEvent.change(screen.getByRole('textbox', { name: 'HP' }), {
    target: { value: '600' },
  });
  fireEvent.blur(screen.getByRole('textbox', { name: 'HP' }));
  await waitFor(() => {
    expect(screen.getByText('Equipment +0 \u00b7 Total 600')).toBeTruthy();
  });
  fireEvent.click(screen.getByRole('button', { name: 'Equipment' }));
  fireEvent.click(screen.getByRole('button', { name: 'Change Body' }));
  fireEvent.change(screen.getByRole('combobox', { name: 'Body item' }), {
    target: { value: 'item:150' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Create and equip' }));
  fireEvent.click(screen.getByRole('button', { name: 'Status' }));
  await waitFor(() => {
    expect(screen.getByText('Equipment +120 \u00b7 Total 720')).toBeTruthy();
  });
  expect(screen.getByRole('textbox', { name: 'HP' })).toHaveProperty(
    'value',
    '600',
  );
  expect(mocks.compute).toHaveBeenCalledTimes(1);
  expect(mocks.preview).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: 'Review changes' }));
  const dialog = await screen.findByRole('dialog', { name: 'Review changes' });
  expect(dialog.textContent).toContain('540 \u2192 720');
  expect(dialog.textContent).not.toContain('9830400');
  fireEvent.click(within(dialog).getByRole('button', { name: 'Keep editing' }));
  fireEvent.click(screen.getByRole('button', { name: 'Reset HP' }));
  await waitFor(() => {
    expect(screen.getByText('Equipment +120 \u00b7 Total 660')).toBeTruthy();
    expect(screen.getByRole('textbox', { name: 'HP' })).toHaveProperty(
      'value',
      '540',
    );
  });
  expect(screen.getByText('Equipment +120 \u00b7 Total 660')).toBeTruthy();
  expect(mocks.preview).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: 'Discard all' }));
  expect(screen.getByRole('textbox', { name: 'HP' })).toHaveProperty(
    'value',
    '540',
  );
  expect(screen.getByText('Equipment +0 \u00b7 Total 540')).toBeTruthy();
  expect(mocks.save).not.toHaveBeenCalled();
});

test('pointer-down review applies pending input even when blur starts calculation before click', async () => {
  openWorkspace();
  fireEvent.change(screen.getByRole('textbox', { name: 'Level' }), {
    target: { value: '10' },
  });
  fireEvent.change(screen.getByRole('textbox', { name: 'EXP' }), {
    target: { value: '64' },
  });
  fireEvent.change(screen.getByRole('textbox', { name: 'HP' }), {
    target: { value: '600' },
  });
  expect(mocks.compute).not.toHaveBeenCalled();
  fireEvent.pointerDown(
    screen.getByRole('button', { name: 'Review changes' }),
    { button: 0 },
  );
  fireEvent.blur(screen.getByRole('textbox', { name: 'HP' }));
  await screen.findByRole('dialog', { name: 'Review changes' });
  expect(mocks.compute).toHaveBeenCalledTimes(1);
  expect(mocks.preview).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Confirm and save' }));
  await waitFor(() => {
    expect(mocks.save).toHaveBeenCalledWith({
      snapshotGeneration: 1,
      manualSlotId: 0,
      operations: [
        { kind: 'character_level', unitPosition: 0, value: '10' },
        { kind: 'experience', unitPosition: 0, value: '64' },
        { kind: 'base_stat', unitPosition: 0, stat: 'hp', value: 9_830_400 },
      ],
    });
  });
});
