import type { BaseStatKind, EditContext, ReaderUnit } from './reader-ipc';
import type { UnitDraft } from './editor-draft';
import { NumericField, valueText } from './workspace-fields';
import { WorkspaceStatFields } from './WorkspaceStatFields';

export function WorkspaceStatus({
  unit,
  projected,
  statInputs,
  onStatChange,
  onStatCommit,
  onStatReset,
  projectionError,
  projecting,
  draft,
  editable,
  progressionEditable,
  zodiac,
  sex,
  onChange,
}: {
  unit: ReaderUnit;
  projected: ReaderUnit;
  statInputs: {
    stat: BaseStatKind;
    text: string;
    error: string | null;
    ready: boolean;
  }[];
  onStatChange: (stat: BaseStatKind, value: string) => void;
  onStatCommit: (stat: BaseStatKind) => void;
  onStatReset: (stat: BaseStatKind) => void;
  projectionError: string | null;
  projecting: boolean;
  draft: UnitDraft;
  editable: boolean;
  progressionEditable: boolean;
  zodiac: NonNullable<EditContext['zodiacOptions']>[number] | undefined;
  sex: NonNullable<EditContext['sexOptions']>[number] | undefined;
  onChange: (
    key: 'bravery' | 'faith' | 'zodiac' | 'sex' | 'level' | 'exp',
    value: string,
  ) => void;
}) {
  return (
    <div className="workspace-status-grid">
      <section className="workspace-paper workspace-character">
        <h3>Character details</h3>
        <div className="workspace-field-grid workspace-compact-fields">
          {(
            [
              ['level', 'Level', unit.saved.level],
              ['exp', 'EXP', unit.saved.exp],
            ] as const
          ).map(([key, label, original]) =>
            progressionEditable ? (
              <NumericField
                key={key}
                label={label}
                saved={original}
                value={draft[key] ?? String(original)}
                onChange={(value) => {
                  onChange(key, value);
                }}
              />
            ) : (
              <dl className="workspace-facts" key={key}>
                <div>
                  <dt>{label}</dt>
                  <dd>
                    {valueText(
                      projected.stored[key === 'level' ? 'level' : 'experience']
                        .value,
                    )}
                  </dd>
                </div>
              </dl>
            ),
          )}
          {(
            [
              ['bravery', 'Bravery', unit.saved.start_bcp, 'brave'],
              ['faith', 'Faith', unit.saved.start_faith, 'faith'],
            ] as const
          ).map(([key, label, original, stored]) =>
            editable ? (
              <NumericField
                key={key}
                label={label}
                saved={original}
                value={draft[key] ?? String(original)}
                onChange={(value) => {
                  onChange(key, value);
                }}
              />
            ) : (
              <dl className="workspace-facts" key={key}>
                <div>
                  <dt>{label}</dt>
                  <dd>{valueText(projected.stored[stored].value)}</dd>
                </div>
              </dl>
            ),
          )}
        </div>
        <dl className="workspace-facts workspace-identity">
          <div className="workspace-identity-sex">
            <dt>Sex</dt>
            <dd>
              {sex && sex.choices.length > 1 ? (
                <select
                  aria-label="Sex"
                  value={draft.sex ?? sex.current}
                  onChange={(event) => {
                    onChange('sex', event.target.value);
                  }}
                >
                  {sex.choices.map((choice) => (
                    <option key={choice.key} value={choice.key}>
                      {choice.label}
                    </option>
                  ))}
                </select>
              ) : (
                <>
                  {valueText(projected.stored.sex.value)}
                  {sex?.unavailableReason && (
                    <small>{sex.unavailableReason}</small>
                  )}
                </>
              )}
            </dd>
          </div>
          <div className="workspace-identity-zodiac">
            <dt>Zodiac</dt>
            <dd>
              {zodiac ? (
                <select
                  aria-label="Zodiac"
                  value={draft.zodiac ?? zodiac.current}
                  onChange={(event) => {
                    onChange('zodiac', event.target.value);
                  }}
                >
                  {zodiac.choices.map((choice) => (
                    <option key={choice.key} value={choice.key}>
                      {choice.label}
                    </option>
                  ))}
                </select>
              ) : (
                valueText(projected.stored.zodiac.value)
              )}
            </dd>
          </div>
        </dl>
      </section>
      <WorkspaceStatFields
        unit={unit}
        projected={projected}
        draft={draft}
        editable={progressionEditable}
        inputs={statInputs}
        onChange={onStatChange}
        onCommit={onStatCommit}
        onReset={onStatReset}
        error={projectionError}
        busy={projecting}
      />
    </div>
  );
}
