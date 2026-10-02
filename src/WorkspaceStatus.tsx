import type { EditContext, ReaderUnit } from './reader-ipc';
import type { UnitDraft } from './editor-draft';
import { EvasionDetails, jobName } from './ReaderRoster';
import { NumericField, valueText } from './workspace-fields';

export function WorkspaceStatus({
  unit,
  draft,
  editable,
  zodiac,
  sex,
  onChange,
}: {
  unit: ReaderUnit;
  draft: UnitDraft;
  editable: boolean;
  zodiac: NonNullable<EditContext['zodiacOptions']>[number] | undefined;
  sex: NonNullable<EditContext['sexOptions']>[number] | undefined;
  onChange: (
    key: 'bravery' | 'faith' | 'zodiac' | 'sex',
    value: string,
  ) => void;
}) {
  return (
    <div className="workspace-status-grid">
      <section className="workspace-paper">
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
              <dd>{valueText(unit.stored.brave.value)}</dd>
            </div>
            <div>
              <dt>Faith</dt>
              <dd>{valueText(unit.stored.faith.value)}</dd>
            </div>
          </dl>
        )}
      </section>
      <section className="workspace-paper">
        <h3>Stats</h3>
        <dl className="workspace-facts workspace-stats">
          {(
            [
              ['HP', unit.effective.hp],
              ['MP', unit.effective.mp],
              ['PA', unit.effective.physical_attack],
              ['MA', unit.effective.magical_attack],
              ['Speed', unit.effective.speed],
              ['Move', unit.effective.movement_tiles],
              ['Jump', unit.effective.jump_tiles],
            ] as const
          ).map(([label, fact]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{valueText(fact.value)}</dd>
            </div>
          ))}
        </dl>
        <details className="workspace-more-data">
          <summary>Evasion</summary>
          <EvasionDetails unit={unit} />
        </details>
      </section>
      <section className="workspace-paper">
        <h3>Character details</h3>
        <dl className="workspace-facts">
          <div>
            <dt>XP</dt>
            <dd>{valueText(unit.stored.experience.value)}</dd>
          </div>
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
                  {valueText(unit.stored.sex.value)}
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
                valueText(unit.stored.zodiac.value)
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
                {unit.growth.value.state === 'known'
                  ? unit.growth.value.value[key]
                  : valueText(unit.growth.value)}
              </dd>
            </div>
          ))}
        </dl>
      </section>
    </div>
  );
}
