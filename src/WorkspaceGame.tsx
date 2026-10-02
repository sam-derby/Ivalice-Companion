import type { ReaderDocument } from './reader-ipc';
import {
  displaySavedAt,
  displayTime,
  NumericField,
  valueText,
} from './workspace-fields';

export function WorkspaceGame({
  reader,
  gil,
  savedGil,
  onGilChange,
}: {
  reader: ReaderDocument | null;
  gil: string;
  savedGil: number | null;
  onGilChange: (value: string) => void;
}) {
  return (
    <section
      className="workspace-game"
      aria-labelledby="workspace-game-heading"
    >
      <h2 className="visually-hidden" id="workspace-game-heading">
        Game
      </h2>
      <article className="workspace-paper">
        <h3>Save overview</h3>
        <div className="workspace-game-grid">
          <div className="workspace-gil-card">
            {savedGil !== null ? (
              <NumericField
                label="Gil"
                id="gil-input"
                saved={savedGil}
                value={gil}
                onChange={onGilChange}
              />
            ) : (
              <dl className="workspace-facts">
                <div>
                  <dt>Gil</dt>
                  <dd>{reader ? valueText(reader.gil.value) : 'Unknown'}</dd>
                </div>
              </dl>
            )}
          </div>
          {reader && (
            <dl className="workspace-facts">
              <div>
                <dt>Chapter</dt>
                <dd>{valueText(reader.progress.chapter.value)}</dd>
              </div>
              <div>
                <dt>Current area</dt>
                <dd>
                  {valueText(reader.progress.location.value, (area) =>
                    valueText(area.label),
                  )}
                </dd>
              </div>
              <div>
                <dt>Play time</dt>
                <dd>
                  {valueText(
                    reader.progress.play_time_seconds.value,
                    displayTime,
                  )}
                </dd>
              </div>
              <div>
                <dt>Ramza level</dt>
                <dd>{valueText(reader.progress.ramza_level.value)}</dd>
              </div>
              <div>
                <dt>Saved at</dt>
                <dd>
                  {valueText(
                    reader.progress.saved_at_unix_seconds.value,
                    displaySavedAt,
                  )}
                </dd>
              </div>
            </dl>
          )}
        </div>
      </article>
    </section>
  );
}
