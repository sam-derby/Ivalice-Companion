import { statFields, type BaseStatKind, type ReaderUnit } from './reader-ipc';
import type { UnitDraft } from './editor-draft';
import { EvasionDetails } from './ReaderRoster';
import { NumericField, valueText } from './workspace-fields';

export function WorkspaceStatFields({
  unit,
  projected,
  draft,
  editable,
  inputs,
  onChange,
  onCommit,
  onReset,
  error,
  busy,
}: {
  unit: ReaderUnit;
  projected: ReaderUnit;
  draft: UnitDraft;
  editable: boolean;
  inputs: {
    stat: BaseStatKind;
    text: string;
    error: string | null;
    ready: boolean;
  }[];
  onChange: (stat: BaseStatKind, value: string) => void;
  onCommit: (stat: BaseStatKind) => void;
  onReset: (stat: BaseStatKind) => void;
  error: string | null;
  busy: boolean;
}) {
  return (
    <section className="workspace-paper">
      <h3>Base stats</h3>
      <p className="workspace-stat-help">
        Equipment bonuses are added to these values. Press Enter or leave a
        field to apply it.
      </p>
      {busy && <p role="status">Calculating stats…</p>}
      <div className="workspace-field-grid" aria-busy={busy}>
        {statFields.map(([stat, label]) => {
          const detail = projected.effective.breakdown?.[stat];
          const fact = detail?.base.value ?? projected.effective[stat].value;
          const saved =
            unit.effective.breakdown?.[stat]?.base.value ??
            unit.effective[stat].value;
          const input = inputs.find((entry) => entry.stat === stat);
          const changed = draft.statBases?.[stat] !== undefined || !!input;
          return (
            <div key={stat}>
              {editable && fact.state === 'known' ? (
                <>
                  <NumericField
                    label={label}
                    saved={saved.state === 'known' ? saved.value : fact.value}
                    value={input?.text ?? String(fact.value)}
                    onChange={(value) => {
                      onChange(stat, value);
                    }}
                    onCommit={() => {
                      onCommit(stat);
                    }}
                    onReset={() => {
                      onReset(stat);
                    }}
                    isChanged={changed}
                    invalid={!!input?.error}
                  />
                  {detail && (
                    <small className="workspace-stat-breakdown">
                      Equipment{' '}
                      {valueText(detail.equipment_bonus.value, (value) =>
                        value >= 0 ? `+${String(value)}` : String(value),
                      )}{' '}
                      · Total {valueText(projected.effective[stat].value)}
                    </small>
                  )}
                  {saved.state === 'known' && fact.value !== saved.value && (
                    <small>
                      Saved base {saved.value} → Base {fact.value}
                    </small>
                  )}
                </>
              ) : (
                <>
                  <dl className="workspace-facts">
                    <div>
                      <dt>{label}</dt>
                      <dd>{valueText(fact)}</dd>
                    </div>
                  </dl>
                  {changed && (
                    <button
                      type="button"
                      className="workspace-reset"
                      aria-label={`Reset ${label}`}
                      onClick={() => {
                        onReset(stat);
                      }}
                    >
                      Reset
                    </button>
                  )}
                </>
              )}
              {input?.error && <p role="alert">{input.error}</p>}
              {input && !input.error && !input.ready && (
                <small>Waiting to apply</small>
              )}
            </div>
          );
        })}
      </div>
      <dl className="workspace-facts workspace-stats">
        <div>
          <dt>Move</dt>
          <dd>{valueText(projected.effective.movement_tiles.value)}</dd>
        </div>
        <div>
          <dt>Jump</dt>
          <dd>{valueText(projected.effective.jump_tiles.value)}</dd>
        </div>
      </dl>
      {error && <p role="alert">{error}</p>}
      <details className="workspace-more-data">
        <summary>Evasion</summary>
        <EvasionDetails unit={projected} />
      </details>
    </section>
  );
}
