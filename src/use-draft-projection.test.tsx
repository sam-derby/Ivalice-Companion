import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, test, vi } from 'vitest';
import { draftDocument, knownFact } from './draft-test-fixture';
import type {
  previewBaseStat,
  previewDraft,
  SaveTransactionRequest,
} from './reader-ipc';
const mocks = vi.hoisted(() => ({
  preview: vi.fn<typeof previewDraft>(),
  compute: vi.fn<typeof previewBaseStat>(),
}));
vi.mock('./reader-ipc', async (original) => ({
  ...(await original<typeof import('./reader-ipc')>()),
  previewDraft: mocks.preview,
  previewBaseStat: mocks.compute,
}));
import { useDraftProjection } from './use-draft-projection';

const empty: SaveTransactionRequest = {
  snapshotGeneration: 1,
  manualSlotId: 0,
  operations: [],
};
const edited: SaveTransactionRequest = {
  ...empty,
  operations: [
    { kind: 'base_stat', unitPosition: 0, stat: 'hp', value: 9_830_400 },
  ],
};
const response = {
  storedBase: 9_830_400,
  base: knownFact(600),
  total: knownFact(600),
};
beforeEach(() => {
  mocks.preview
    .mockReset()
    .mockResolvedValue({ reader: draftDocument(), solvedBase: null });
  mocks.compute.mockReset().mockResolvedValue(response);
});

function setup(request = empty) {
  const onBase = vi.fn();
  const document = draftDocument();
  const hook = renderHook(
    ({ request }) => useDraftProjection(request, onBase, document),
    { initialProps: { request } },
  );
  return { ...hook, onBase };
}

test('typing and invalid ranges perform no calculations, committing calculates only the edited stat', async () => {
  const { result, onBase, rerender } = setup();
  act(() => {
    result.current.changeStat(0, 'physical_attack', '555');
  });
  expect(result.current.inputs[0]?.error).toBe('Enter PA from 1 to 99.');
  expect(result.current.busy).toBe(false);
  act(() => {
    result.current.resetStat(0, 'physical_attack');
  });
  for (const text of ['6', '60', '600']) {
    act(() => {
      result.current.changeStat(0, 'hp', text);
    });
  }
  await act(async () => {
    await new Promise((resolve) => window.setTimeout(resolve, 120));
  });
  expect(mocks.compute).not.toHaveBeenCalled();
  expect(mocks.preview).not.toHaveBeenCalled();
  expect(result.current.busy).toBe(false);
  expect(result.current.blocked).toBe(true);
  act(() => {
    result.current.commitStat(0, 'hp');
  });
  expect(result.current.busy).toBe(true);
  await waitFor(() => {
    expect(onBase).toHaveBeenCalledWith(0, 'hp', 9_830_400);
  });
  expect(mocks.compute).toHaveBeenCalledWith({
    stat: 'hp',
    previousBase: 8_847_360,
    value: 600,
    jobMultiplier: 100,
    equipmentBonus: 0,
  });
  rerender({ request: edited });
  expect(result.current.current).toBe(true);
  expect(result.current.busy).toBe(false);
  expect(result.current.inputs).toEqual([]);
  await act(async () => {
    await new Promise((resolve) => window.setTimeout(resolve, 120));
  });
  expect(mocks.compute).toHaveBeenCalledTimes(1);
  expect(mocks.preview).not.toHaveBeenCalled();
  rerender({
    request: {
      ...edited,
      operations: [
        ...edited.operations,
        { kind: 'gil', value: '101' },
        { kind: 'experience', unitPosition: 0, value: '10' },
        { kind: 'character_level', unitPosition: 0, value: '11' },
        { kind: 'bravery', unitPosition: 0, value: '75' },
        {
          kind: 'equipped_slot',
          unitPosition: 0,
          combatSet: null,
          slot: 'reaction',
          valueKey: 'ability:445',
        },
      ],
    },
  });
  await act(async () => {
    await new Promise((resolve) => window.setTimeout(resolve, 120));
  });
  expect(result.current.busy).toBe(false);
  expect(mocks.compute).toHaveBeenCalledTimes(1);
  expect(mocks.preview).not.toHaveBeenCalled();
});

test('a story step is previewed so the overview can show its chapter', async () => {
  const projected = draftDocument();
  projected.progress.chapter = knownFact('Chapter 2');
  mocks.preview.mockResolvedValue({ reader: projected, solvedBase: null });
  const { result } = setup({
    ...empty,
    operations: [{ kind: 'story_step', progress: '465' }],
  });
  await waitFor(() => {
    expect(result.current.reader?.progress.chapter).toEqual(
      knownFact('Chapter 2'),
    );
  });
  expect(mocks.preview).toHaveBeenCalledTimes(1);
});

test('committing an unchanged value skips even the arithmetic request', async () => {
  const { result } = setup();
  act(() => {
    result.current.changeStat(0, 'hp', '540');
    result.current.commitAll();
  });
  await waitFor(() => {
    expect(result.current.blocked).toBe(false);
  });
  expect(mocks.compute).not.toHaveBeenCalled();
  expect(mocks.preview).not.toHaveBeenCalled();
});

test('gear and movement changes project once without inverse-solving existing bases', async () => {
  const { result, rerender, onBase } = setup(edited);
  await waitFor(() => {
    expect(result.current.busy).toBe(false);
  });
  const gear: SaveTransactionRequest = {
    ...edited,
    operations: [
      ...edited.operations,
      {
        kind: 'gear',
        unitPosition: 0,
        slot: 'accessory',
        itemKey: 'item:213',
        source: 'create_and_equip',
      },
    ],
  };
  rerender({ request: gear });
  await waitFor(() => {
    expect(mocks.preview).toHaveBeenLastCalledWith({ transaction: gear });
  });
  await waitFor(() => {
    expect(result.current.busy).toBe(false);
  });
  const movement: SaveTransactionRequest = {
    ...gear,
    operations: [
      ...gear.operations,
      {
        kind: 'equipped_slot',
        unitPosition: 0,
        combatSet: null,
        slot: 'movement',
        valueKey: 'ability:486',
      },
    ],
  };
  rerender({ request: movement });
  await waitFor(() => {
    expect(mocks.preview).toHaveBeenLastCalledWith({ transaction: movement });
  });
  expect(mocks.compute).not.toHaveBeenCalled();
  expect(onBase).not.toHaveBeenCalled();
});

test('reset restores the exact saved base using current gear, without a full reprojection', async () => {
  const document = draftDocument();
  if (document.roster.value.state !== 'known') throw new Error('Fixture');
  const unit = document.roster.value.value[0];
  if (!unit?.effective.breakdown?.hp) throw new Error('Fixture');
  unit.stored.bases.hp = knownFact(9_830_400);
  unit.effective.hp = knownFact(720);
  unit.effective.breakdown.hp = {
    ...unit.effective.breakdown.hp,
    base: knownFact(600),
    equipment_bonus: knownFact(120),
  };
  mocks.preview.mockResolvedValue({ reader: document, solvedBase: null });
  const gear: SaveTransactionRequest = {
    ...edited,
    operations: [
      ...edited.operations,
      {
        kind: 'gear',
        unitPosition: 0,
        slot: 'body',
        itemKey: 'item:150',
        source: 'create_and_equip',
      },
    ],
  };
  const { result, rerender, onBase } = setup(gear);
  await waitFor(() => {
    expect(result.current.current).toBe(true);
  });
  let resolveReset:
    ((value: Awaited<ReturnType<typeof previewBaseStat>>) => void) | undefined;
  mocks.compute.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveReset = resolve;
      }),
  );
  act(() => {
    result.current.resetStat(0, 'hp');
  });
  await waitFor(() => {
    expect(mocks.compute).toHaveBeenLastCalledWith({
      stat: 'hp',
      previousBase: 8_847_360,
      value: null,
      jobMultiplier: 100,
      equipmentBonus: 120,
    });
  });
  // Apply the parent's draft update in the same turn as the calculation.
  await act(async () => {
    resolveReset?.({
      storedBase: 8_847_360,
      base: knownFact(540),
      total: knownFact(660),
    });
    await Promise.resolve();
    rerender({
      request: {
        ...gear,
        operations: gear.operations.filter(
          (operation) => operation.kind !== 'base_stat',
        ),
      },
    });
  });
  expect(onBase).toHaveBeenCalledWith(0, 'hp', 8_847_360);
  expect(result.current.current).toBe(true);
  const roster = result.current.reader?.roster.value;
  expect(roster?.state === 'known' && roster.value[0]?.effective.hp).toEqual(
    knownFact(660),
  );
  expect(mocks.preview).toHaveBeenCalledTimes(1);
});

test('late arithmetic cannot overwrite newer text or a discarded draft', async () => {
  let resolveOld:
    ((value: Awaited<ReturnType<typeof previewBaseStat>>) => void) | undefined;
  mocks.compute.mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveOld = resolve;
      }),
  );
  const { result, onBase } = setup();
  act(() => {
    result.current.changeStat(0, 'hp', '600');
    result.current.commitAll();
  });
  await waitFor(() => {
    expect(mocks.compute).toHaveBeenCalledTimes(1);
  });
  act(() => {
    result.current.changeStat(0, 'hp', '601');
  });
  await act(async () => {
    resolveOld?.(response);
    await Promise.resolve();
  });
  expect(onBase).not.toHaveBeenCalled();
  expect(result.current.inputs[0]?.text).toBe('601');
  act(() => {
    result.current.commitAll();
  });
  await waitFor(() => {
    expect(mocks.compute).toHaveBeenCalledTimes(2);
  });
  act(() => {
    result.current.discard();
  });
  await act(async () => {
    resolveOld?.(response);
    await Promise.resolve();
  });
  expect(onBase).not.toHaveBeenCalled();
  expect(result.current.reader).toBeNull();
});

test('late full projections cannot overwrite a newer draft', async () => {
  let resolveOld:
    ((value: Awaited<ReturnType<typeof previewDraft>>) => void) | undefined;
  mocks.preview.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveOld = resolve;
      }),
  );
  const { result, rerender } = setup(edited);
  await waitFor(() => {
    expect(mocks.preview).toHaveBeenCalledTimes(1);
  });
  rerender({
    request: {
      ...edited,
      operations: [
        { kind: 'base_stat', unitPosition: 0, stat: 'hp', value: 9_846_784 },
      ],
    },
  });
  await waitFor(() => {
    expect(result.current.current).toBe(true);
  });
  const accepted = result.current.reader;
  await act(async () => {
    resolveOld?.({ reader: draftDocument(), solvedBase: null });
    await Promise.resolve();
  });
  expect(result.current.reader).toBe(accepted);
});

test('an unrepresentable committed base blocks review and resets without recalculation', async () => {
  const { result, onBase } = setup();
  mocks.compute.mockRejectedValue(new Error('Unrepresentable'));
  act(() => {
    result.current.changeStat(0, 'hp', '999');
    result.current.commitAll();
  });
  await waitFor(() => {
    expect(result.current.hasErrors).toBe(true);
  });
  expect(result.current.inputs[0]?.error).toContain('cannot be represented');
  expect(result.current.busy).toBe(false);
  expect(result.current.blocked).toBe(true);
  act(() => {
    result.current.resetStat(0, 'hp');
  });
  expect(result.current.blocked).toBe(false);
  expect(onBase).not.toHaveBeenCalled();
  expect(mocks.preview).not.toHaveBeenCalled();
});
