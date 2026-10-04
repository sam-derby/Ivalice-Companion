import { useEffect, useEffectEvent, useState } from 'react';
import {
  previewBaseStat,
  previewDraft,
  statFields,
  type BaseStatKind,
  type ReaderDocument,
  type SaveTransactionRequest,
} from './reader-ipc';

interface StatInput {
  position: number;
  stat: BaseStatKind;
  text: string;
  error: string | null;
  ready: boolean;
  reset?: boolean;
}

/** Only operations which affect these mechanics invalidate their projection. */
function transactionKey(transaction: SaveTransactionRequest | null): string {
  return transaction
    ? JSON.stringify([
        transaction.snapshotGeneration,
        transaction.manualSlotId,
        transaction.operations
          .filter(
            (operation) =>
              operation.kind === 'base_stat' ||
              operation.kind === 'gear' ||
              (operation.kind === 'equipped_slot' &&
                operation.slot === 'movement'),
          )
          .map((operation) => JSON.stringify(operation))
          .sort(),
      ])
    : '';
}

/** Text is committed on blur/Enter/review. Only Rust-computed bases enter the draft. */
export function useDraftProjection(
  transaction: SaveTransactionRequest | null,
  onBase: (position: number, stat: BaseStatKind, base: number) => void,
  baseline: ReaderDocument | null = null,
) {
  const [projection, setProjection] = useState<{
    key: string;
    reader: ReaderDocument;
  } | null>(null);
  const [inputs, setInputs] = useState<StatInput[]>([]);
  const [failure, setFailure] = useState<{
    key: string;
    message: string;
  } | null>(null);
  const required =
    transaction?.operations.some(
      (operation) =>
        operation.kind === 'base_stat' ||
        operation.kind === 'gear' ||
        (operation.kind === 'equipped_slot' && operation.slot === 'movement'),
    ) ?? false;
  const key = transactionKey(transaction);
  const pending = inputs.find((input) => input.ready && input.error === null);
  const inputKey = pending ? JSON.stringify(pending) : '';
  const needsProjection =
    required && projection?.key !== key && failure?.key !== key;
  const busy =
    !!transaction && failure?.key !== key && (needsProjection || !!pending);

  const runPreview = useEffectEvent(
    (active: () => boolean, requestKey: string) => {
      if (!transaction || failure?.key === key) return;
      const input = pending;
      const document = projection?.key === key ? projection.reader : baseline;
      // Gear/movement must be projected before using their new bonuses for an input.
      if (needsProjection || !input) {
        void previewDraft({ transaction })
          .then((result) => {
            if (!active()) return;
            setProjection({ key: requestKey, reader: result.reader });
            setFailure(null);
          })
          .catch(() => {
            if (!active()) return;
            setFailure({
              key: requestKey,
              message:
                'Could not preview these changes. Reload the save or revert the edit.',
            });
          });
        return;
      }
      const units = document?.roster.value;
      const unit =
        units?.state === 'known'
          ? units.value.find((unit) => unit.key === input.position)
          : undefined;
      const originalUnits = baseline?.roster.value;
      const original =
        originalUnits?.state === 'known'
          ? originalUnits.value.find((unit) => unit.key === input.position)
          : undefined;
      const detail = unit?.effective.breakdown?.[input.stat];
      const multiplier = detail?.job_multiplier?.value;
      const previous = (input.reset ? original : unit)?.stored.bases[input.stat]
        .value;
      const bonus = detail?.equipment_bonus.value;
      const fail = () => {
        if (!active()) return;
        setInputs((current) =>
          current.map((entry) =>
            entry.position === input.position && entry.stat === input.stat
              ? {
                  ...entry,
                  error:
                    'This base value cannot be represented for this character.',
                }
              : entry,
          ),
        );
      };
      if (
        !document ||
        !unit ||
        !detail ||
        units?.state !== 'known' ||
        multiplier?.state !== 'known' ||
        previous?.state !== 'known'
      ) {
        fail();
        return;
      }
      if (
        !input.reset &&
        detail.base.value.state === 'known' &&
        detail.base.value.value === Number(input.text)
      ) {
        setInputs((current) =>
          current.filter(
            (entry) =>
              entry.position !== input.position || entry.stat !== input.stat,
          ),
        );
        return;
      }
      void previewBaseStat({
        stat: input.stat,
        previousBase: previous.value,
        value: input.reset ? null : Number(input.text),
        jobMultiplier: multiplier.value,
        equipmentBonus: bonus?.state === 'known' ? bonus.value : null,
      })
        .then((result) => {
          if (!active()) return;
          const unchanged = original?.stored.bases[input.stat].value;
          const acceptedKey = transactionKey({
            ...transaction,
            operations: [
              ...transaction.operations.filter(
                (operation) =>
                  operation.kind !== 'base_stat' ||
                  operation.unitPosition !== input.position ||
                  operation.stat !== input.stat,
              ),
              ...(unchanged?.state === 'known' &&
              unchanged.value === result.storedBase
                ? []
                : [
                    {
                      kind: 'base_stat' as const,
                      unitPosition: input.position,
                      stat: input.stat,
                      value: result.storedBase,
                    },
                  ]),
            ],
          });
          onBase(input.position, input.stat, result.storedBase);
          // Presentation copies the backend's one-field result; no stat formula runs here.
          setProjection({
            key: acceptedKey,
            reader: {
              ...document,
              roster: {
                value: {
                  state: 'known',
                  value: units.value.map((row) =>
                    row.key !== input.position
                      ? row
                      : {
                          ...row,
                          stored: {
                            ...row.stored,
                            bases: {
                              ...row.stored.bases,
                              [input.stat]: {
                                value: {
                                  state: 'known',
                                  value: result.storedBase,
                                },
                              },
                            },
                          },
                          effective: {
                            ...row.effective,
                            [input.stat]: result.total,
                            breakdown: {
                              ...row.effective.breakdown,
                              [input.stat]: { ...detail, base: result.base },
                            },
                          },
                        },
                  ),
                },
              },
            },
          });
          setInputs((current) =>
            current.filter(
              (entry) =>
                entry.position !== input.position || entry.stat !== input.stat,
            ),
          );
          setFailure(null);
        })
        .catch(fail);
    },
  );

  useEffect(() => {
    if (!key || (!needsProjection && !inputKey)) return;
    let active = true;
    // Only equipment/movement changes need a debounced full-draft preview.
    const timer = window.setTimeout(
      () => {
        runPreview(() => active, key);
      },
      needsProjection ? 80 : 0,
    );
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [key, inputKey, needsProjection, projection?.key]);

  function changeStat(position: number, stat: BaseStatKind, text: string) {
    setFailure(null);
    const units = baseline?.roster.value;
    const maximum =
      units?.state === 'known'
        ? units.value.find((unit) => unit.key === position)?.effective
            .breakdown?.[stat]?.maximum
        : undefined;
    const label = statFields.find(([kind]) => kind === stat)?.[1] ?? 'Stat';
    const error =
      !/^\d{1,3}$/.test(text) || Number(text) < 1
        ? 'Enter a positive whole number.'
        : maximum !== undefined && Number(text) > maximum
          ? `Enter ${label} from 1 to ${String(maximum)}.`
          : null;
    setInputs((current) => [
      ...current.filter(
        (entry) => entry.position !== position || entry.stat !== stat,
      ),
      { position, stat, text, error, ready: false },
    ]);
  }

  function commitStat(position: number, stat: BaseStatKind) {
    setInputs((current) =>
      current.map((input) =>
        input.position === position && input.stat === stat
          ? { ...input, ready: true }
          : input,
      ),
    );
  }
  function commitAll() {
    setInputs((current) => current.map((input) => ({ ...input, ready: true })));
  }
  function resetStat(position: number, stat: BaseStatKind) {
    const units = baseline?.roster.value;
    const original =
      units?.state === 'known'
        ? units.value.find((unit) => unit.key === position)
        : undefined;
    const base = original?.effective.breakdown?.[stat]?.base.value;
    if (
      transaction?.operations.some(
        (operation) =>
          operation.kind === 'base_stat' &&
          operation.unitPosition === position &&
          operation.stat === stat,
      ) &&
      base?.state === 'known'
    ) {
      setInputs((current) => [
        ...current.filter(
          (entry) => entry.position !== position || entry.stat !== stat,
        ),
        {
          position,
          stat,
          text: String(base.value),
          error: null,
          ready: true,
          reset: true,
        },
      ]);
    } else {
      setInputs((current) =>
        current.filter(
          (entry) => entry.position !== position || entry.stat !== stat,
        ),
      );
    }
    setFailure(null);
  }
  function discard() {
    setInputs([]);
    setProjection(null);
    setFailure(null);
  }
  const error = failure?.key === key ? failure.message : null;
  return {
    reader: required ? (projection?.reader ?? null) : null,
    busy,
    error,
    inputs,
    changeStat,
    commitStat,
    commitAll,
    resetStat,
    discard,
    hasErrors: inputs.some((input) => input.error !== null) || !!error,
    blocked: busy || inputs.length > 0 || !!error,
    current: projection?.key === key,
  };
}
