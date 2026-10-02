import { useId } from 'react';
import type { ValueState } from './ipc';

export function valueText<T>(
  value: ValueState<T>,
  format: (value: T) => string = String,
): string {
  switch (value.state) {
    case 'known':
      return format(value.value);
    case 'absent':
      return 'None';
    case 'unsupported':
      return 'Unsupported';
    case 'unknown':
      return 'Unknown';
  }
}

export function displayTime(seconds: number): string {
  return `${String(Math.floor(seconds / 3600))}h`;
}

export function displaySavedAt(seconds: number): string {
  if (!Number.isSafeInteger(seconds) || seconds <= 0 || seconds > 2_147_483_647)
    return 'Unknown';
  return new Intl.DateTimeFormat(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(seconds * 1000));
}

export function NumericField({
  label,
  accessibleLabel = label,
  id,
  saved,
  value,
  disabled = false,
  onChange,
}: {
  label: string;
  accessibleLabel?: string;
  id?: string;
  saved: number;
  value: string;
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const changed = value !== String(saved);
  return (
    <div className="workspace-field" data-changed={changed || undefined}>
      <label htmlFor={inputId}>{label}</label>
      <div className="workspace-input-row">
        <input
          id={inputId}
          aria-label={accessibleLabel}
          type="text"
          inputMode="numeric"
          autoComplete="off"
          disabled={disabled}
          value={value}
          onChange={(event) => {
            onChange(event.target.value);
          }}
        />
        {changed && (
          <button
            type="button"
            className="workspace-reset"
            aria-label={`Reset ${accessibleLabel}`}
            disabled={disabled}
            onClick={() => {
              onChange(String(saved));
            }}
          >
            Reset
          </button>
        )}
      </div>
    </div>
  );
}

// Draft numbers affect presentation only; collectDraft still validates writes.
export function stagedNumber(value: string | undefined, saved: number): number {
  if (value === undefined || !/^\d+$/.test(value)) return saved;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : saved;
}
