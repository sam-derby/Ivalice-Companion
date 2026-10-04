import { beforeEach, describe, expect, test, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open }));

import { chooseSave, getSaveSelection } from './ipc';

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.open.mockReset();
});

describe('native save IPC', () => {
  test('uses the explicit selection command', async () => {
    mocks.invoke.mockResolvedValueOnce({ state: 'no_selection' });
    await expect(getSaveSelection()).resolves.toEqual({
      state: 'no_selection',
    });
    expect(mocks.invoke).toHaveBeenLastCalledWith('get_save_selection');
  });

  test('cancellation does not change the current selection', async () => {
    mocks.open.mockResolvedValueOnce(null);
    await expect(chooseSave()).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  test('passes only the single chosen path to the selection command', async () => {
    mocks.open.mockResolvedValueOnce('C:\\synthetic\\enhanced.png');
    mocks.invoke.mockResolvedValueOnce({ state: 'selected' });
    await expect(chooseSave()).resolves.toEqual({ state: 'selected' });
    expect(mocks.open).toHaveBeenCalledWith({
      title: 'Choose an Enhanced save',
      directory: false,
      multiple: false,
      filters: [{ name: 'Enhanced save', extensions: ['png'] }],
    });
    expect(mocks.invoke).toHaveBeenCalledWith('set_save_selection', {
      path: 'C:\\synthetic\\enhanced.png',
    });
  });
});
