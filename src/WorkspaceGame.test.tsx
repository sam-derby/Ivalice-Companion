import { fireEvent, render, screen, within } from '@testing-library/react';
import { expect, test, vi } from 'vitest';
import { draftContext, draftDocument, knownFact } from './draft-test-fixture';
import { WorkspaceGame } from './WorkspaceGame';

function fact(label: string) {
  const term = screen.getByText(label, { selector: 'dt' });
  const row = term.parentElement;
  if (!row) throw new Error(`Missing ${label} row`);
  return within(row).getByRole('definition').textContent;
}

test('shows the joined chapter, objective, area and saved story step', () => {
  const reader = draftDocument();
  reader.progress.chapter = knownFact('Chapter 3');
  reader.progress.objective = knownFact('Reach the monastery.');
  reader.progress.story_progress = knownFact(940);
  reader.progress.location = knownFact({
    id: 'area:24',
    label: { state: 'known', value: 'Open Plain' },
    description: { state: 'unknown' },
    asset_key: { state: 'unknown' },
  });
  render(
    <WorkspaceGame
      reader={reader}
      gil="100"
      savedGil={100}
      onGilChange={() => undefined}
    />,
  );
  expect(fact('Chapter')).toBe('Chapter 3');
  expect(fact('Objective')).toBe('Reach the monastery.');
  expect(fact('Current area')).toBe('Open Plain');
  expect(fact('Story step')).toBe('940');
});

test('keeps an uncaptioned story step visible without inventing an objective', () => {
  const reader = draftDocument();
  reader.progress.story_progress = knownFact(380);
  render(
    <WorkspaceGame
      reader={reader}
      gil="100"
      savedGil={100}
      onGilChange={() => undefined}
    />,
  );
  expect(fact('Objective')).toBe('Unknown');
  expect(fact('Story step')).toBe('380');
});

test('offers listed story steps and reports the chosen one', () => {
  const onStoryStepChange = vi.fn();
  render(
    <WorkspaceGame
      reader={draftDocument()}
      gil="100"
      savedGil={100}
      onGilChange={() => undefined}
      context={{
        ...draftContext(),
        storyStep: 940,
        storyChoices: [
          { progress: 465, chapter: 'Chapter 2', objective: 'Free them.' },
          { progress: 940, chapter: 'Chapter 3', objective: null },
        ],
      }}
      storyStep={undefined}
      onStoryStepChange={onStoryStepChange}
    />,
  );
  const select = screen.getByRole('combobox', { name: 'Story step' });
  expect(select).toHaveProperty('value', '940');
  expect(
    within(select)
      .getAllByRole('option')
      .map((option) => option.textContent),
  ).toEqual(['465 · Chapter 2 · Free them.', '940 · Chapter 3']);
  fireEvent.change(select, { target: { value: '465' } });
  expect(onStoryStepChange).toHaveBeenCalledWith('465');
  expect(screen.getByText('Other changes')).toBeTruthy();
});

test('edits the in-game date and achievement unlocks', () => {
  const onCalendarChange = vi.fn();
  const onAchievementChange = vi.fn();
  render(
    <WorkspaceGame
      reader={draftDocument()}
      gil="100"
      savedGil={100}
      onGilChange={() => undefined}
      context={{
        ...draftContext(),
        calendar: {
          month: 3,
          day: 31,
          monthLengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        },
        achievements: [
          {
            index: 8,
            description: 'Reach level 50.',
            unlocked: false,
            progress: 12,
          },
        ],
      }}
      calendar={undefined}
      onCalendarChange={onCalendarChange}
      achievements={undefined}
      onAchievementChange={onAchievementChange}
    />,
  );
  fireEvent.change(screen.getByRole('combobox', { name: 'Month' }), {
    target: { value: '2' },
  });
  expect(onCalendarChange).toHaveBeenCalledWith({ month: 2, day: 28 });
  expect(
    within(screen.getByRole('combobox', { name: 'Day' })).getAllByRole(
      'option',
    ),
  ).toHaveLength(31);
  expect(screen.getByTitle(/Progress counter/).textContent).toBe('12');
  expect(screen.getByText('0 of 1 unlocked')).toBeTruthy();
  fireEvent.click(screen.getByRole('checkbox', { name: 'Reach level 50.' }));
  expect(onAchievementChange).toHaveBeenCalledWith(8, true);
  expect(screen.getByText('Changes edit only this save.')).toBeTruthy();
});
