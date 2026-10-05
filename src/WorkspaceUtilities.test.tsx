import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { beforeEach, expect, test, vi } from 'vitest';
import { draftContext } from './draft-test-fixture';

const mocks = vi.hoisted(() => ({
  slotOperation: vi.fn(),
  exportSaveSlot: vi.fn(),
  inspectSaveFile: vi.fn(),
  chooseImportFile: vi.fn(),
  chooseExportFile: vi.fn(),
}));

vi.mock('./ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./ipc')>()),
  chooseImportFile: mocks.chooseImportFile,
  chooseExportFile: mocks.chooseExportFile,
}));

vi.mock('./reader-ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./reader-ipc')>()),
  slotOperation: mocks.slotOperation,
  exportSaveSlot: mocks.exportSaveSlot,
  inspectSaveFile: mocks.inspectSaveFile,
}));

import { WorkspaceUtilities } from './WorkspaceUtilities';

const onReload = vi.fn<(slot: number, message: string) => Promise<void>>();

function renderUtilities(pendingChanges = false) {
  render(
    <WorkspaceUtilities
      context={{ ...draftContext(), snapshotGeneration: 7 }}
      occupied={[0, 1]}
      summaries={[
        {
          manualSlot: 0,
          title: 'Gariland',
          savedAtUnixSeconds: null,
          playTimeSeconds: null,
        },
        {
          manualSlot: 1,
          title: 'Dorter',
          savedAtUnixSeconds: null,
          playTimeSeconds: null,
        },
      ]}
      loadedSlot={0}
      pendingChanges={pendingChanges}
      onReload={onReload}
    />,
  );
}

function slotButton(name: RegExp) {
  return within(screen.getByRole('list', { name: 'Save slots' })).getByRole(
    'button',
    { name },
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.slotOperation.mockResolvedValue(null);
  mocks.exportSaveSlot.mockResolvedValue(null);
  onReload.mockResolvedValue(undefined);
});

test('lists all fifty slots and copies only after a replace confirmation', async () => {
  renderUtilities();
  expect(
    within(screen.getByRole('list', { name: 'Save slots' })).getAllByRole(
      'button',
    ),
  ).toHaveLength(50);
  fireEvent.click(screen.getByRole('button', { name: 'Copy to…' }));
  fireEvent.change(screen.getByRole('combobox', { name: 'Target slot' }), {
    target: { value: '1' },
  });
  const confirm = screen.getByRole('button', { name: 'Confirm' });
  expect(confirm).toHaveProperty('disabled', true);
  fireEvent.click(
    screen.getByRole('checkbox', { name: /Replace the saved game in Slot 2/ }),
  );
  fireEvent.click(confirm);
  await waitFor(() => {
    expect(onReload).toHaveBeenCalledWith(
      1,
      expect.stringContaining('Copy Slot 1 to Slot 2'),
    );
  });
  expect(mocks.slotOperation).toHaveBeenCalledWith({
    snapshotGeneration: 7,
    operation: { kind: 'copy', from: 0, to: 1, replace: true },
  });
});

test('moves only into empty slots and deletes with a confirmation', async () => {
  renderUtilities();
  fireEvent.click(slotButton(/^Slot 2\b/));
  fireEvent.click(screen.getByRole('button', { name: 'Move to…' }));
  const options = within(
    screen.getByRole('combobox', { name: 'Target slot' }),
  ).getAllByRole('option');
  expect(options.map((option) => option.textContent)).not.toContain(
    'Slot 1 · Gariland',
  );
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
  fireEvent.click(screen.getByRole('button', { name: 'Delete slot' }));
  await waitFor(() => {
    expect(onReload).toHaveBeenCalledWith(
      0,
      expect.stringContaining('Delete Slot 2'),
    );
  });
  expect(mocks.slotOperation).toHaveBeenCalledWith({
    snapshotGeneration: 7,
    operation: { kind: 'delete', slot: 1 },
  });
});

test('imports a chosen slot from another file into an empty slot', async () => {
  mocks.chooseImportFile.mockResolvedValue('C:\\saves\\other.png');
  mocks.inspectSaveFile.mockResolvedValue({
    slots: [{ slot: 4, title: 'Zeklaus', savedAtUnixSeconds: null }],
  });
  renderUtilities();
  fireEvent.click(slotButton(/^Slot 10\b/));
  fireEvent.click(screen.getByRole('button', { name: 'Import from file…' }));
  fireEvent.click(screen.getByRole('button', { name: 'Choose a save file…' }));
  await screen.findByRole('combobox', {
    name: /Slot to import from other.png/,
  });
  fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
  await waitFor(() => {
    expect(mocks.slotOperation).toHaveBeenCalledWith({
      snapshotGeneration: 7,
      operation: {
        kind: 'import',
        sourcePath: 'C:\\saves\\other.png',
        sourceSlot: 4,
        slot: 9,
        replace: false,
      },
    });
  });
  expect(onReload).toHaveBeenCalledWith(9, expect.any(String));
});

test('exports a slot through the save dialog', async () => {
  mocks.chooseExportFile.mockResolvedValue('C:\\exports\\slot-1.png');
  renderUtilities();
  fireEvent.click(
    screen.getByRole('button', { name: 'Save as separate file…' }),
  );
  await screen.findByText('Slot 1 exported.');
  expect(mocks.exportSaveSlot).toHaveBeenCalledWith({
    snapshotGeneration: 7,
    slot: 0,
    path: 'C:\\exports\\slot-1.png',
    overwrite: true,
  });
});

test('pending edits block slot operations', () => {
  renderUtilities(true);
  expect(screen.getByRole('button', { name: 'Copy to…' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(screen.getByText(/pending changes/)).toBeTruthy();
});
