import type { BaseStatKind, EditContext, ReaderUnit } from './reader-ipc';
import type { UnitDraft } from './editor-draft';
import { jobName } from './ReaderRoster';
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
      <section className="workspace-paper">
        <h3>Level &amp; EXP</h3>
        <div className="workspace-field-grid">
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
        </div>
        <h3>Bravery &amp; Faith</h3>
        {editable ? (
          <div className="workspace-field-grid">
            {(
              [
                ['bravery', 'Bravery', unit.saved.start_bcp],
                ['faith', 'Faith', unit.saved.start_faith],
              ] as const
            ).map(([key, label, original]) => (
              <NumericField
                key={key}
                label={label}
                saved={original}
                value={draft[key] ?? String(original)}
                onChange={(value) => {
                  onChange(key, value);
                }}
              />
            ))}
          </div>
        ) : (
          <dl className="workspace-facts">
            <div>
              <dt>Bravery</dt>
              <dd>{valueText(projected.stored.brave.value)}</dd>
            </div>
            <div>
              <dt>Faith</dt>
              <dd>{valueText(projected.stored.faith.value)}</dd>
            </div>
          </dl>
        )}
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
      <section className="workspace-paper">
        <h3>Character details</h3>
        <dl className="workspace-facts">
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
      <section
        className="workspace-paper workspace-growth"
        aria-label={`${jobName(unit)} Growth`}
      >
        <h3>{jobName(unit)} Growth</h3>
        <dl className="workspace-facts">
          {(
            [
              ['HP', 'hp'],
              ['MP', 'mp'],
              ['Speed', 'speed'],
              ['PA', 'physical_attack'],
              ['MA', 'magical_attack'],
            ] as const
          ).map(([label, key]) => (
            <div key={key}>
              <dt>{label}</dt>
              <dd>
                {projected.growth.value.state === 'known'
                  ? projected.growth.value.value[key]
                  : valueText(projected.growth.value)}
              </dd>
            </div>
          ))}
        </dl>
      </section>
    </div>
  );
}
