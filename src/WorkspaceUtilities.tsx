import { useState } from 'react';
import { chooseExportFile, chooseImportFile } from './ipc';
import {
  exportSaveSlot,
  inspectSaveFile,
  slotOperation,
  type EditContext,
  type SlotOperationRequest,
  type SlotSummary,
} from './reader-ipc';
import { displaySavedAt } from './workspace-fields';

type Mode = 'copy' | 'move' | 'swap' | 'delete' | 'import';

interface ImportSource {
  path: string;
  name: string;
  slots: { slot: number; title: string | null }[];
}

function slotName(slot: number): string {
  return `Slot ${String(slot + 1)}`;
}

function errorText(error: unknown): string {
  const code =
    typeof error === 'object' && error !== null && 'code' in error
      ? String(error.code)
      : null;
  switch (code) {
    case 'save_changed_since_load':
      return 'The save changed since it was loaded. Reload and try again.';
    case 'export_path_protected':
      return 'Exports cannot replace the open save or its backup. Choose another file.';
    case 'export_path_not_png':
      return 'Exports must be saved as a .png file.';
    case 'export_target_exists':
      return 'That file already exists. Choose another name.';
    case 'import_slot_empty':
      return 'That slot is empty in the chosen file.';
    case 'save_recovery_required':
      return 'Could not finish writing. The original save is in the file with " - backup" in its name beside the selected save. Recover that file before trying again.';
    default:
      return `The operation could not be completed.${code ? ` Code: ${code}.` : ''}`;
  }
}

export function WorkspaceUtilities({
  context,
  occupied,
  summaries,
  loadedSlot,
  pendingChanges,
  onReload,
}: {
  context: EditContext | null;
  occupied: number[];
  summaries: SlotSummary[];
  loadedSlot: number;
  pendingChanges: boolean;
  onReload: (slot: number, message: string) => Promise<void>;
}) {
  const [selected, setSelected] = useState(loadedSlot);
  const [mode, setMode] = useState<Mode | null>(null);
  const [target, setTarget] = useState<number | null>(null);
  const [replace, setReplace] = useState(false);
  const [source, setSource] = useState<ImportSource | null>(null);
  const [sourceSlot, setSourceSlot] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const isOccupied = (slot: number) => occupied.includes(slot);
  const summary = (slot: number) =>
    summaries.find((row) => row.manualSlot === slot);
  const describe = (slot: number) => {
    if (!isOccupied(slot)) return 'Empty';
    const row = summary(slot);
    return (
      [
        row?.title,
        row?.savedAtUnixSeconds ? displaySavedAt(row.savedAtUnixSeconds) : null,
      ]
        .filter(Boolean)
        .join(' · ') || 'Saved game'
    );
  };
  const slots = Array.from({ length: 50 }, (_, slot) => slot);
  const disabled = busy || pendingChanges || !context;

  function start(next: Mode) {
    setMode(next);
    setTarget(null);
    setReplace(false);
    setSource(null);
    setSourceSlot(null);
    setError(null);
    setMessage(null);
  }

  function cancel() {
    setMode(null);
    setError(null);
  }

  const targets =
    mode === 'copy'
      ? slots.filter((slot) => slot !== selected)
      : mode === 'move'
        ? slots.filter((slot) => !isOccupied(slot))
        : mode === 'swap'
          ? slots.filter((slot) => slot !== selected && isOccupied(slot))
          : [];
  const replacing =
    (mode === 'copy' && target !== null && isOccupied(target)) ||
    (mode === 'import' && isOccupied(selected));

  function request(): {
    operation: SlotOperationRequest;
    summary: string;
    reload: number;
  } | null {
    switch (mode) {
      case 'copy':
        return target === null
          ? null
          : {
              operation: { kind: 'copy', from: selected, to: target, replace },
              summary: `Copy ${slotName(selected)} to ${slotName(target)}`,
              reload: target,
            };
      case 'move':
        return target === null
          ? null
          : {
              operation: { kind: 'move', from: selected, to: target },
              summary: `Move ${slotName(selected)} to ${slotName(target)}`,
              reload: target,
            };
      case 'swap':
        return target === null
          ? null
          : {
              operation: { kind: 'swap', first: selected, second: target },
              summary: `Swap ${slotName(selected)} and ${slotName(target)}`,
              reload: selected,
            };
      case 'delete': {
        const remaining = occupied.filter((slot) => slot !== selected);
        return {
          operation: { kind: 'delete', slot: selected },
          summary: `Delete ${slotName(selected)}`,
          reload:
            loadedSlot !== selected ? loadedSlot : (remaining[0] ?? loadedSlot),
        };
      }
      case 'import':
        return source === null || sourceSlot === null
          ? null
          : {
              operation: {
                kind: 'import',
                sourcePath: source.path,
                sourceSlot,
                slot: selected,
                replace,
              },
              summary: `Import ${slotName(sourceSlot)} of ${source.name} into ${slotName(selected)}`,
              reload: selected,
            };
      case null:
        return null;
    }
  }

  const pending = request();
  const ready = pending !== null && (!replacing || replace);

  async function confirm() {
    if (!context || !pending || !ready) return;
    setBusy(true);
    setError(null);
    try {
      await slotOperation({
        snapshotGeneration: context.snapshotGeneration,
        operation: pending.operation,
      });
      setMode(null);
      await onReload(
        pending.reload,
        `${pending.summary}: done. A backup was created beside the save.`,
      );
    } catch (failure: unknown) {
      setError(errorText(failure));
    } finally {
      setBusy(false);
    }
  }

  async function chooseSource() {
    setError(null);
    try {
      const path = await chooseImportFile();
      if (path === null) return;
      const inspected = await inspectSaveFile(path);
      setSource({
        path,
        name: path.split(/[\\/]/).pop() ?? path,
        slots: inspected.slots,
      });
      setSourceSlot(inspected.slots[0]?.slot ?? null);
    } catch (failure: unknown) {
      setError(errorText(failure));
    }
  }

  async function exportSelected() {
    if (!context) return;
    setError(null);
    setMessage(null);
    try {
      const path = await chooseExportFile(selected);
      if (path === null) return;
      setBusy(true);
      await exportSaveSlot({
        snapshotGeneration: context.snapshotGeneration,
        slot: selected,
        path,
        overwrite: true,
      });
      setMessage(`${slotName(selected)} exported.`);
    } catch (failure: unknown) {
      setError(errorText(failure));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section
      className="workspace-utilities"
      aria-labelledby="workspace-utilities-heading"
    >
      <h2 className="visually-hidden" id="workspace-utilities-heading">
        Utilities
      </h2>
      <div className="workspace-utilities-layout">
        <ul className="workspace-slot-list" aria-label="Save slots">
          {slots.map((slot) => (
            <li key={slot}>
              <button
                type="button"
                aria-pressed={slot === selected}
                data-empty={isOccupied(slot) ? undefined : ''}
                onClick={() => {
                  setSelected(slot);
                  setMode(null);
                  setError(null);
                  setMessage(null);
                }}
              >
                <strong>
                  {slotName(slot)}
                  {slot === loadedSlot ? ' (open)' : ''}
                </strong>{' '}
                <small>{describe(slot)}</small>
              </button>
            </li>
          ))}
        </ul>
        <article
          className="workspace-paper workspace-slot-detail"
          aria-labelledby="workspace-slot-heading"
        >
          <h3 id="workspace-slot-heading">{slotName(selected)}</h3>
          <p>{describe(selected)}</p>
          <p className="workspace-utilities-note">
            Every change first copies the save to the file with &quot; -
            backup&quot; in its name.
          </p>
          {pendingChanges && (
            <p role="status">
              Save or discard your pending changes before managing slots.
            </p>
          )}
          {mode === null && (
            <div className="workspace-slot-actions">
              {isOccupied(selected) ? (
                <>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => {
                      start('copy');
                    }}
                  >
                    Copy to…
                  </button>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => {
                      start('move');
                    }}
                  >
                    Move to…
                  </button>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => {
                      start('swap');
                    }}
                  >
                    Swap with…
                  </button>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => {
                      void exportSelected();
                    }}
                  >
                    Save as separate file…
                  </button>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => {
                      start('import');
                    }}
                  >
                    Replace from file…
                  </button>
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => {
                      start('delete');
                    }}
                  >
                    Delete
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  disabled={disabled}
                  onClick={() => {
                    start('import');
                  }}
                >
                  Import from file…
                </button>
              )}
            </div>
          )}
          {mode === null && isOccupied(selected) && (
            <small className="workspace-slot-hint">
              Save as separate file writes a new game save containing only this
              slot, in the same slot number, for backing up or sharing. The open
              save is not changed.
            </small>
          )}
          {(mode === 'copy' || mode === 'move' || mode === 'swap') && (
            <label className="workspace-slot-target">
              {mode === 'swap' ? 'Swap with' : 'Target slot'}
              <select
                value={target === null ? '' : String(target)}
                onChange={(event) => {
                  setTarget(
                    event.target.value === ''
                      ? null
                      : Number(event.target.value),
                  );
                  setReplace(false);
                }}
              >
                <option value="">Choose a slot</option>
                {targets.map((slot) => (
                  <option key={slot} value={slot}>
                    {slotName(slot)} · {describe(slot)}
                  </option>
                ))}
              </select>
            </label>
          )}
          {mode === 'import' && (
            <div className="workspace-slot-target">
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  void chooseSource();
                }}
              >
                {source ? 'Choose another file…' : 'Choose a save file…'}
              </button>
              {source && source.slots.length === 0 && (
                <p>{source.name} has no saved slots.</p>
              )}
              {source && source.slots.length > 0 && (
                <label>
                  Slot to import from {source.name}
                  <select
                    value={sourceSlot === null ? '' : String(sourceSlot)}
                    onChange={(event) => {
                      setSourceSlot(Number(event.target.value));
                    }}
                  >
                    {source.slots.map((row) => (
                      <option key={row.slot} value={row.slot}>
                        {slotName(row.slot)}
                        {row.title ? ` · ${row.title}` : ''}
                      </option>
                    ))}
                  </select>
                </label>
              )}
            </div>
          )}
          {mode !== null && replacing && (
            <label className="workspace-slot-replace">
              <input
                type="checkbox"
                checked={replace}
                onChange={(event) => {
                  setReplace(event.target.checked);
                }}
              />
              Replace the saved game in{' '}
              {slotName(mode === 'copy' && target !== null ? target : selected)}
            </label>
          )}
          {mode !== null && (
            <div className="workspace-slot-confirm">
              {pending && <p>{pending.summary}.</p>}
              <button
                type="button"
                disabled={busy || !ready}
                onClick={() => {
                  void confirm();
                }}
              >
                {mode === 'delete' ? 'Delete slot' : 'Confirm'}
              </button>
              <button
                type="button"
                className="workspace-secondary"
                disabled={busy}
                onClick={cancel}
              >
                Cancel
              </button>
            </div>
          )}
          {message && <p role="status">{message}</p>}
          {error && <p role="alert">{error}</p>}
        </article>
      </div>
    </section>
  );
}
