import { useState } from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { AddCharacterForm } from './AddCharacterForm';
import type { EditorDraft } from './editor-draft';
import type { EditContext } from './reader-ipc';

const context: EditContext = {
  snapshotGeneration: 1,
  manualSlotId: 0,
  gil: 0,
  genericCreation: { targetPosition: null, donors: [] },
  storyAddition: { targetPosition: null, donors: [] },
  guestAddition: { targetPosition: null, donors: [] },
  creatureAddition: {
    targetPosition: 1,
    monsters: [
      { key: 'creature:94', label: 'Chocobo', available: true },
      {
        key: 'creature:125',
        label: 'Treant',
        available: false,
        unavailableReason: 'game roster crash reported',
      },
    ],
    enemies: [
      { key: 'creature:65', label: 'Ultima · High Seraph', available: true },
      { key: 'creature:73', label: 'Ultima · Arch Seraph', available: false },
    ],
  },
};

it('searches distinct forms, accepts a name and creates without a donor', () => {
  const add = vi.fn();
  function Harness() {
    const [kind, setKind] = useState<'monster' | 'enemy'>('monster');
    const [draft, setDraft] = useState<EditorDraft>({
      gil: '0',
      inventory: {},
      units: {},
      generic: null,
      story: null,
      guest: null,
      creature: { formKey: 'creature:94', name: '' },
    });
    return (
      <AddCharacterForm
        context={context}
        draft={draft}
        kind={kind}
        choices={[]}
        busy={false}
        invalid={false}
        errors={[]}
        onCreature={(creature) => {
          setDraft({ ...draft, creature });
        }}
        onKind={(next) => {
          if (next === 'monster' || next === 'enemy') {
            setKind(next);
            setDraft({
              ...draft,
              creature: {
                formKey: next === 'monster' ? 'creature:94' : 'creature:65',
                name: '',
              },
            });
          }
        }}
        onName={() => {}}
        onCharacter={() => {}}
        onCancel={() => {}}
        onAdd={() => {
          add(draft.creature);
        }}
      />
    );
  }
  render(<Harness />);
  expect(screen.getByRole('option', { name: 'Chocobo' })).toBeTruthy();
  expect(
    screen.getByRole('option', {
      name: 'Treant (game roster crash reported)',
    }),
  ).toHaveProperty('disabled', true);
  expect(screen.queryByRole('option', { name: /Ultima/ })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Enemies' }));
  expect(screen.queryByRole('option', { name: 'Chocobo' })).toBeNull();
  fireEvent.change(screen.getByRole('textbox', { name: 'Search enemies' }), {
    target: { value: 'Ultima' },
  });
  expect(screen.queryByRole('option', { name: 'Chocobo' })).toBeNull();
  expect(screen.getByRole('combobox', { name: 'Enemy' })).toHaveProperty(
    'value',
    'creature:65',
  );
  expect(
    screen.getByRole('option', {
      name: 'Ultima · Arch Seraph (already in party)',
    }),
  ).toHaveProperty('disabled', true);
  fireEvent.change(screen.getByRole('textbox', { name: 'Name (optional)' }), {
    target: { value: 'Nova' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Add and edit' }));
  expect(add).toHaveBeenCalledWith({ formKey: 'creature:65', name: 'Nova' });
  fireEvent.change(screen.getByRole('textbox', { name: 'Search enemies' }), {
    target: { value: 'missing species' },
  });
  expect(screen.getByText('No matching enemies.')).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Add and edit' })).toHaveProperty(
    'disabled',
    true,
  );
});
