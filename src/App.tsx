import { useCallback, useEffect, useRef, useState } from 'react';
import {
  chooseSave,
  getSaveSelection,
  type IpcError,
  type NormalizedSave,
  type SaveSelectionStatus,
} from './ipc';
import {
  loadReader,
  type EditContext,
  type ReaderDocument,
  type ReaderIdentity,
  type SlotSummary,
} from './reader-ipc';
import { readerIdentityKey } from './ReaderRoster';
import { SaveWorkspace } from './SaveWorkspace';
import type { WorkspaceView } from './workspace-view';
import { displaySavedAt, displayTime } from './workspace-fields';
import { version } from '../package.json';

type SelectionView =
  | { state: 'checking' }
  | { state: 'no_selection' }
  | { state: 'selected' }
  | { state: 'unavailable'; error: IpcError }
  | { state: 'error'; error: IpcError | null };

type LoadView =
  | { state: 'idle' }
  | { state: 'loading' }
  | { state: 'loaded' }
  | { state: 'error'; error: IpcError | null };

type PendingAction =
  { kind: 'choose' } | { kind: 'reload' } | { kind: 'slot'; slot: number };

function isIpcError(error: unknown): error is IpcError {
  return (
    typeof error === 'object' &&
    error !== null &&
    'category' in error &&
    'code' in error &&
    'retryable' in error
  );
}

function errorMessage(error: IpcError | null): string {
  if (error?.category === 'unsupported') {
    return 'This save format is unsupported. Choose an Enhanced save.';
  }
  if (error?.category === 'corrupt') {
    return 'The selected file could not be safely parsed. Reload after the game has finished writing, or choose another save.';
  }
  if (error?.retryable) {
    return 'The source changed or was busy while it was read. Try reloading after the game has finished writing.';
  }
  return 'The companion could not load the selected save safely. Choose it again or try reloading.';
}

function slotSummaryText(summary: SlotSummary | undefined): string | null {
  if (!summary) return null;
  const parts: string[] = [];
  if (summary.title) parts.push(summary.title);
  if (
    summary.savedAtUnixSeconds !== null &&
    Number.isSafeInteger(summary.savedAtUnixSeconds) &&
    summary.savedAtUnixSeconds > 0
  ) {
    parts.push(displaySavedAt(summary.savedAtUnixSeconds));
  }
  if (summary.playTimeSeconds !== null && summary.playTimeSeconds >= 0) {
    parts.push(`${displayTime(summary.playTimeSeconds)} played`);
  }
  return parts.length ? parts.join(' · ') : null;
}

export function App() {
  const [selection, setSelection] = useState<SelectionView>({
    state: 'checking',
  });
  const [load, setLoad] = useState<LoadView>({ state: 'idle' });
  const [slots, setSlots] = useState<number[]>([]);
  const [slotSummaries, setSlotSummaries] = useState<SlotSummary[]>([]);
  const [selectedSlot, setSelectedSlot] = useState<number | null>(null);
  const [save, setSave] = useState<NormalizedSave | null>(null);
  const [reader, setReader] = useState<ReaderDocument | null>(null);
  const [readerError, setReaderError] = useState<IpcError | null>(null);
  const [editContext, setEditContext] = useState<EditContext | null>(null);
  const [hasPending, setHasPending] = useState(false);
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(
    null,
  );
  const [notice, setNotice] = useState<string | null>(null);
  const requestSequence = useRef(0);
  const selectionSequence = useRef(0);
  const lastReaderIdentity = useRef<ReaderIdentity | null>(null);
  const workspaceViews = useRef(new Map<number, WorkspaceView>());
  const [initialWorkspaceView, setInitialWorkspaceView] =
    useState<WorkspaceView>();
  const onViewChange = useCallback((slot: number, view: WorkspaceView) => {
    workspaceViews.current.set(slot, view);
  }, []);

  useEffect(() => {
    let active = true;
    const selectionId = selectionSequence.current;
    void getSaveSelection().then(
      (status: SaveSelectionStatus) => {
        if (active && selectionSequence.current === selectionId)
          setSelection(status);
      },
      (error: unknown) => {
        if (active && selectionSequence.current === selectionId) {
          setSelection({
            state: 'error',
            error: isIpcError(error) ? error : null,
          });
        }
      },
    );
    return () => {
      active = false;
      requestSequence.current += 1;
    };
  }, []);

  const clearLoadedSave = useCallback(() => {
    requestSequence.current += 1;
    setLoad({ state: 'idle' });
    setSlots([]);
    setSlotSummaries([]);
    setSelectedSlot(null);
    setSave(null);
    setReader(null);
    setReaderError(null);
    setEditContext(null);
    setHasPending(false);
    setNotice(null);
    lastReaderIdentity.current = null;
    workspaceViews.current.clear();
    setInitialWorkspaceView(undefined);
  }, []);

  const requestLoad = useCallback(async (manualSlotId: number | null) => {
    const requestId = requestSequence.current + 1;
    requestSequence.current = requestId;
    setLoad({ state: 'loading' });
    setSelectedSlot(manualSlotId);
    setSave(null);
    setReader(null);
    setReaderError(null);
    setEditContext(null);
    setHasPending(false);
    setNotice(null);
    try {
      const response = await loadReader({ requestId, manualSlotId });
      if (
        requestSequence.current !== requestId ||
        response.requestId !== requestId
      )
        return false;
      const nextIdentity = response.reader?.identity;
      const previousIdentity = lastReaderIdentity.current;
      if (
        (nextIdentity && nextIdentity.manual_slot !== manualSlotId) ||
        (response.save?.selected_manual_slot.state === 'known' &&
          response.save.selected_manual_slot.value.id !== manualSlotId) ||
        (nextIdentity &&
          previousIdentity &&
          nextIdentity.session === previousIdentity.session &&
          (nextIdentity.snapshot_generation <
            previousIdentity.snapshot_generation ||
            nextIdentity.resource_generation <
              previousIdentity.resource_generation ||
            (nextIdentity.resource_generation ===
              previousIdentity.resource_generation &&
              JSON.stringify(nextIdentity.resource_token) !==
                JSON.stringify(previousIdentity.resource_token))))
      ) {
        setLoad({ state: 'error', error: null });
        return false;
      }
      setSlots(response.occupiedManualSlots);
      setSlotSummaries(response.slotSummaries);
      setSelectedSlot(manualSlotId);
      setSave(response.save);
      setReader(response.reader);
      setReaderError(response.readerError);
      setEditContext(response.editContext ?? null);
      setInitialWorkspaceView(
        manualSlotId === null
          ? undefined
          : workspaceViews.current.get(manualSlotId),
      );
      if (nextIdentity) lastReaderIdentity.current = nextIdentity;
      setLoad({ state: 'loaded' });
      return true;
    } catch (error: unknown) {
      if (requestSequence.current === requestId) {
        setLoad({ state: 'error', error: isIpcError(error) ? error : null });
      }
      return false;
    }
  }, []);

  useEffect(() => {
    let active = true;
    if (selection.state === 'selected' && load.state === 'idle') {
      queueMicrotask(() => {
        if (active) void requestLoad(null);
      });
    }
    return () => {
      active = false;
    };
  }, [selection.state, load.state, requestLoad]);

  async function selectSave() {
    const selectionId = selectionSequence.current + 1;
    selectionSequence.current = selectionId;
    const previousSelection = selection;
    setSelection({ state: 'checking' });
    try {
      const status = await chooseSave();
      if (selectionSequence.current !== selectionId) return;
      if (status === null) {
        setSelection(previousSelection);
        return;
      }
      clearLoadedSave();
      setSelection(status);
    } catch (error: unknown) {
      if (selectionSequence.current === selectionId) {
        setSelection({
          state: 'error',
          error: isIpcError(error) ? error : null,
        });
      }
    }
  }

  function runAction(action: PendingAction) {
    switch (action.kind) {
      case 'choose':
        void selectSave();
        break;
      case 'reload':
        void requestLoad(selectedSlot);
        break;
      case 'slot':
        void requestLoad(action.slot);
        break;
    }
  }

  function requestAction(action: PendingAction) {
    if (hasPending) setPendingAction(action);
    else runAction(action);
  }

  const onDirtyChange = useCallback((dirty: boolean) => {
    setHasPending(dirty);
  }, []);
  const onReload = useCallback(
    async (slot: number, message: string) => {
      if (await requestLoad(slot)) setNotice(message);
    },
    [requestLoad],
  );

  const orderedSlots = [...slots].sort((left, right) => {
    const savedAt = (slot: number) => {
      const value = slotSummaries.find(
        (summary) => summary.manualSlot === slot,
      )?.savedAtUnixSeconds;
      return value !== null &&
        value !== undefined &&
        Number.isSafeInteger(value)
        ? value
        : Number.NEGATIVE_INFINITY;
    };
    const leftTime = savedAt(left);
    const rightTime = savedAt(right);
    return leftTime === rightTime ? left - right : rightTime - leftTime;
  });

  return (
    <main className="app-shell">
      <header className="app-header">
        <div className="app-brand">
          <h1>Ivalice Companion</h1>
          <span className="app-version" aria-label={`App version ${version}`}>
            v{version}
          </span>
        </div>
        <div className="app-file-controls">
          <button
            type="button"
            disabled={selection.state === 'checking'}
            onClick={() => {
              requestAction({ kind: 'choose' });
            }}
          >
            Choose save
          </button>
          {slots.length > 0 && (
            <label className="slot-picker" htmlFor="manual-slot-select">
              <span className="visually-hidden">Manual slot</span>
              <select
                id="manual-slot-select"
                value={selectedSlot === null ? '' : String(selectedSlot)}
                onChange={(event) => {
                  const slot = Number(event.target.value);
                  if (
                    event.target.value !== '' &&
                    slots.includes(slot) &&
                    slot !== selectedSlot
                  )
                    requestAction({ kind: 'slot', slot });
                }}
              >
                <option value="">Choose a slot</option>
                {orderedSlots.map((slot) => {
                  const detail = slotSummaryText(
                    slotSummaries.find(
                      (summary) => summary.manualSlot === slot,
                    ),
                  );
                  return (
                    <option key={slot} value={slot}>
                      Slot {slot + 1}
                      {detail ? ` · ${detail}` : ''}
                    </option>
                  );
                })}
              </select>
            </label>
          )}
          <button
            type="button"
            aria-label={
              selectedSlot === null
                ? 'Refresh slots'
                : `Reload slot ${String(selectedSlot + 1)}`
            }
            disabled={
              selection.state !== 'selected' || load.state === 'loading'
            }
            onClick={() => {
              requestAction({ kind: 'reload' });
            }}
          >
            {selectedSlot === null ? 'Refresh slots' : 'Reload'}
          </button>
        </div>
      </header>

      {load.state !== 'loading' &&
        (selection.state === 'unavailable' ||
          selection.state === 'error' ||
          notice) && (
          <div className="app-status" role="status" aria-live="polite">
            {(selection.state === 'unavailable' ||
              selection.state === 'error') &&
              errorMessage(selection.error)}
            {notice}
          </div>
        )}
      {load.state === 'error' && (
        <p className="app-load-error" role="alert">
          {errorMessage(load.error)}{' '}
          {load.error ? `Code: ${load.error.code}.` : ''}
        </p>
      )}
      {readerError && (
        <section className="app-load-error">
          <h2>Named roster unavailable</h2>
          <p role="alert">
            {readerError.code === 'reader_catalogue_missing'
              ? 'This app copy is missing its offline reader data. Keep the complete resources folder beside the app, then reload this slot.'
              : readerError.category === 'resource' ||
                  readerError.code.startsWith('reader_catalogue_')
                ? 'The local reader catalogue is missing or could not be safely loaded. Replace or regenerate the supported offline catalogue, then reload this slot.'
                : 'The named roster could not be safely prepared. Reload after the game has finished writing, or choose another save.'}{' '}
            Gil remains available.
          </p>
        </section>
      )}
      {load.state === 'loaded' && save === null && slots.length === 0 && (
        <section className="app-empty">
          <h2>No occupied manual slots</h2>
          <p>
            The selected save did not report an occupied supported manual slot.
          </p>
        </section>
      )}
      {load.state === 'loaded' && selectedSlot !== null && (save || reader) && (
        <SaveWorkspace
          key={
            reader
              ? readerIdentityKey(reader.identity)
              : `slot:${String(selectedSlot)}`
          }
          reader={reader}
          context={editContext}
          onDirtyChange={onDirtyChange}
          onReload={onReload}
          manualSlotId={selectedSlot}
          initialView={initialWorkspaceView}
          onViewChange={onViewChange}
        />
      )}
      {load.state === 'loading' && (
        <section
          className="app-empty app-loading"
          role="status"
          aria-label="Loading save"
        >
          <span className="app-spinner" aria-hidden="true" />
        </section>
      )}

      {pendingAction && (
        <div className="workspace-modal-backdrop">
          <div
            className="workspace-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="discard-heading"
          >
            <h2 id="discard-heading">Discard pending changes?</h2>
            <p>Changing the save or slot will clear every staged edit.</p>
            <div className="workspace-modal-actions">
              <button
                type="button"
                onClick={() => {
                  setPendingAction(null);
                }}
              >
                Keep editing
              </button>
              <button
                type="button"
                onClick={() => {
                  const action = pendingAction;
                  setPendingAction(null);
                  runAction(action);
                }}
              >
                Discard changes
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}
