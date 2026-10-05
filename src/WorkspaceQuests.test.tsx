import { fireEvent, render, screen, within } from '@testing-library/react';
import { expect, test } from 'vitest';
import { draftContext } from './draft-test-fixture';
import { WorkspaceQuests } from './WorkspaceQuests';

function context() {
  return {
    ...draftContext(),
    sideQuests: [
      {
        name: 'Lost Technology',
        scenes: [
          { label: 'The Metallic Sphere', seen: true },
          { label: 'Beowulf the Hunter', seen: false },
        ],
      },
      {
        name: 'Into the Dark',
        scenes: [{ label: 'Terminus', seen: false }],
        counter: { label: 'Deeper passages found', value: 0 },
      },
    ],
    errands: [
      {
        index: 0,
        title: 'The Fate of Our Company',
        client: 'Psalmba Exports',
        posting: 'Salvage the ship.',
      },
      {
        index: 1,
        title: 'Salvage Expedition Tour',
        client: 'Salvagers',
        posting: 'Join a tour.',
      },
    ],
    collection: [
      {
        index: 0,
        kind: 'wonder' as const,
        name: 'Chaos Shrine',
        description: 'Sealed away.',
      },
      {
        index: 16,
        kind: 'artefact' as const,
        name: 'Four-Deity Plate',
        description: 'Four brooches.',
      },
    ],
  };
}

test('shows read-only side-quest episodes and their status', () => {
  render(<WorkspaceQuests context={context()} />);
  const panel = screen.getByRole('article', { name: 'Side quests' });
  expect(within(panel).getByText('1 of 2 episodes')).toBeTruthy();
  expect(within(panel).getByText('Not started')).toBeTruthy();
  expect(within(panel).getByText('Deeper passages found: 0')).toBeTruthy();
  expect(
    within(panel).getByRole('list', { name: 'Lost Technology episodes' })
      .textContent,
  ).toContain('The Metallic Sphere, seen');
  expect(within(panel).queryByRole('checkbox')).toBeNull();
});

test('lists and searches errands, artefacts and wonders', () => {
  render(<WorkspaceQuests context={context()} />);
  const errands = screen.getByRole('article', { name: 'Errands' });
  expect(within(errands).getByText('2 errands')).toBeTruthy();
  fireEvent.change(within(errands).getByRole('searchbox'), {
    target: { value: 'psalmba' },
  });
  expect(within(errands).getByText('The Fate of Our Company')).toBeTruthy();
  expect(within(errands).queryByText('Salvage Expedition Tour')).toBeNull();
  const collection = screen.getByRole('article', {
    name: 'Artefacts and wonders',
  });
  expect(
    within(
      within(collection).getByRole('region', { name: 'Wonders' }),
    ).getByText('Chaos Shrine'),
  ).toBeTruthy();
  expect(
    within(
      within(collection).getByRole('region', { name: 'Artefacts' }),
    ).getByText('Four-Deity Plate'),
  ).toBeTruthy();
});

test('says when quest information is unavailable', () => {
  render(<WorkspaceQuests context={null} />);
  expect(screen.getByText(/unavailable for this save/)).toBeTruthy();
});
