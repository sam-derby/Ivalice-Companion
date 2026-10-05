import { achievementLabel } from './editor-draft';
import type { EditContext } from './reader-ipc';

type Calendar = NonNullable<EditContext['calendar']>;
type Achievement = NonNullable<EditContext['achievements']>[number];

export function CalendarField({
  saved,
  value,
  onChange,
}: {
  saved: Calendar;
  value: { month: number; day: number } | undefined;
  onChange: (value: { month: number; day: number }) => void;
}) {
  const date = value ?? saved;
  const days = saved.monthLengths[date.month - 1] ?? 0;
  const changed = date.month !== saved.month || date.day !== saved.day;
  return (
    <fieldset
      className="workspace-field workspace-calendar"
      data-changed={changed ? '' : undefined}
    >
      <legend>In-game date</legend>
      <div className="workspace-calendar-fields">
        <label>
          Month
          <select
            value={date.month}
            onChange={(event) => {
              const month = Number(event.target.value);
              const length = saved.monthLengths[month - 1] ?? 1;
              onChange({ month, day: Math.min(date.day, length) });
            }}
          >
            {saved.monthLengths.map((_, index) => (
              <option key={index} value={index + 1}>
                {index + 1}
              </option>
            ))}
          </select>
        </label>
        <label>
          Day
          <select
            value={date.day}
            onChange={(event) => {
              onChange({ month: date.month, day: Number(event.target.value) });
            }}
          >
            {Array.from({ length: days }, (_, index) => (
              <option key={index} value={index + 1}>
                {index + 1}
              </option>
            ))}
          </select>
        </label>
        {changed && (
          <button
            type="button"
            className="workspace-reset"
            onClick={() => {
              onChange({ month: saved.month, day: saved.day });
            }}
          >
            Reset
          </button>
        )}
      </div>
      <small>Effect on errands already under way is untested.</small>
    </fieldset>
  );
}

export function AchievementList({
  achievements,
  draft,
  onChange,
}: {
  achievements: Achievement[];
  draft: Record<number, boolean> | undefined;
  onChange: (index: number, unlocked: boolean) => void;
}) {
  const unlockedCount = achievements.filter(
    (achievement) => draft?.[achievement.index] ?? achievement.unlocked,
  ).length;
  return (
    <article
      className="workspace-paper workspace-achievements"
      aria-labelledby="workspace-achievements-heading"
    >
      <div className="workspace-panel-heading">
        <h3 id="workspace-achievements-heading">Achievements</h3>
        <span className="workspace-view-only">
          {unlockedCount} of {achievements.length} unlocked
        </span>
      </div>
      <p>
        Changes edit only this save. Whether the game or Steam then grants or
        withdraws the achievement is untested.
      </p>
      <ul>
        {achievements.map((achievement) => {
          const unlocked = draft?.[achievement.index] ?? achievement.unlocked;
          return (
            <li
              key={achievement.index}
              data-changed={unlocked !== achievement.unlocked ? '' : undefined}
            >
              <label>
                <input
                  type="checkbox"
                  checked={unlocked}
                  onChange={(event) => {
                    onChange(achievement.index, event.target.checked);
                  }}
                />
                {achievementLabel(achievement.description)}
              </label>
              {achievement.progress > 0 &&
                !(achievement.unlocked && achievement.progress === 1) && (
                  <small title="Progress counter saved for this achievement">
                    {achievement.progress}
                  </small>
                )}
            </li>
          );
        })}
      </ul>
    </article>
  );
}
