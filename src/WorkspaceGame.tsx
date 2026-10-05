import {
  storyGuestsLabel,
  storyPartyLabel,
  storyStepLabel,
} from './editor-draft';
import type { EditContext, ReaderDocument } from './reader-ipc';
import { displaySavedAt, NumericField, valueText } from './workspace-fields';
import { AchievementList, CalendarField } from './WorkspaceGameExtras';

export function WorkspaceGame({
  reader,
  gil,
  savedGil,
  onGilChange,
  context = null,
  storyStep,
  onStoryStepChange,
  calendar,
  onCalendarChange,
  achievements,
  onAchievementChange,
}: {
  reader: ReaderDocument | null;
  gil: string;
  savedGil: number | null;
  onGilChange: (value: string) => void;
  context?: EditContext | null;
  storyStep?: string | undefined;
  onStoryStepChange?: (value: string) => void;
  calendar?: { month: number; day: number } | undefined;
  onCalendarChange?: (value: { month: number; day: number }) => void;
  achievements?: Record<number, boolean> | undefined;
  onAchievementChange?: (index: number, unlocked: boolean) => void;
}) {
  const savedStep = context?.storyStep;
  const choices = context?.storyChoices ?? [];
  const storyEditable =
    context !== null &&
    savedStep !== undefined &&
    choices.length > 0 &&
    !!onStoryStepChange;
  const stepChanged =
    storyStep !== undefined && storyStep !== String(savedStep);
  const showAchievements =
    !!context?.achievements?.length && !!onAchievementChange;
  return (
    <section
      className="workspace-game"
      aria-labelledby="workspace-game-heading"
    >
      <h2 className="visually-hidden" id="workspace-game-heading">
        Game
      </h2>
      <div
        className="workspace-game-layout"
        data-columns={showAchievements ? 2 : 1}
      >
        <div className="workspace-game-column">
          <article className="workspace-paper workspace-game-main">
            <h3>Save overview</h3>
            {reader && (
              <dl className="workspace-facts workspace-game-facts">
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
                  <dt>Ramza level</dt>
                  <dd>{valueText(reader.progress.ramza_level.value)}</dd>
                </div>
                <div>
                  <dt>Story step</dt>
                  <dd>{valueText(reader.progress.story_progress.value)}</dd>
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
                <div className="workspace-game-objective">
                  <dt>Objective</dt>
                  <dd>{valueText(reader.progress.objective.value)}</dd>
                </div>
              </dl>
            )}
            <div className="workspace-game-edits">
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
              {context?.calendar && onCalendarChange && (
                <CalendarField
                  saved={context.calendar}
                  value={calendar}
                  onChange={onCalendarChange}
                />
              )}
              {storyEditable && (
                <div
                  className="workspace-field workspace-story-step"
                  data-changed={stepChanged ? '' : undefined}
                >
                  <label htmlFor="story-step-select">Story step</label>
                  <select
                    id="story-step-select"
                    value={storyStep ?? String(savedStep)}
                    onChange={(event) => {
                      onStoryStepChange(event.target.value);
                    }}
                  >
                    {!choices.some(
                      (choice) => choice.progress === savedStep,
                    ) && (
                      <option value={String(savedStep)}>
                        {String(savedStep)} · saved value
                      </option>
                    )}
                    {choices.map((choice) => (
                      <option
                        key={choice.progress}
                        value={String(choice.progress)}
                      >
                        {storyStepLabel(context, choice.progress)}
                      </option>
                    ))}
                  </select>
                  {stepChanged && (
                    <ul className="workspace-story-effects">
                      <li>
                        Guests: {storyGuestsLabel(context, Number(storyStep))}.
                        They replace the current guests.
                      </li>
                      <li>
                        Party:{' '}
                        {storyPartyLabel(context, Number(storyStep)) ??
                          'no story members join or leave'}
                        . Ramza takes this chapter&apos;s form.
                      </li>
                    </ul>
                  )}
                  <details className="workspace-story-note">
                    <summary>Experimental: the game may soft-lock</summary>
                    <p>
                      Also sets the replayed story variables, flags, game flags
                      and world map, clears Chapter 4 side quests before Chapter
                      4 and moves Ramza to the nearest shown place. Recruited
                      units, Gil and errands stay as they are.
                    </p>
                  </details>
                </div>
              )}
            </div>
          </article>
        </div>
        {showAchievements && (
          <AchievementList
            achievements={context.achievements ?? []}
            draft={achievements}
            onChange={onAchievementChange}
          />
        )}
      </div>
    </section>
  );
}
