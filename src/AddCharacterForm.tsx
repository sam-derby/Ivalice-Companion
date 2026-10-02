import { useState } from 'react';
import type { EditContext } from './reader-ipc';
import type { EditorDraft } from './editor-draft';

export function AddCharacterForm({
  context,
  draft,
  kind,
  choices,
  busy,
  invalid,
  errors,
  onCreature,
  onKind,
  onName,
  onCharacter,
  onCancel,
  onAdd,
}: {
  context: EditContext;
  draft: EditorDraft;
  kind: 'generic' | 'named' | 'monster' | 'enemy';
  choices: NonNullable<EditContext['namedAddition']>['characters'];
  busy: boolean;
  invalid: boolean;
  errors: string[];
  onCreature: (creature: { formKey: string; name: string }) => void;
  onKind: (kind: 'generic' | 'named' | 'monster' | 'enemy') => void;
  onName: (name: string) => void;
  onCharacter: (key: string) => void;
  onCancel: () => void;
  onAdd: () => void;
}) {
  const [query, setQuery] = useState('');
  const creatureChoices =
    (kind === 'monster'
      ? context.creatureAddition?.monsters
      : context.creatureAddition?.enemies) ?? [];
  const visibleCreatures = creatureChoices.filter((choice) =>
    choice.label.toLowerCase().includes(query.toLowerCase()),
  );
  const full =
    context.genericCreation.targetPosition === null &&
    (context.namedAddition?.targetPosition ?? null) === null &&
    (context.creatureAddition?.targetPosition ?? null) === null;
  return (
    <>
      <h2>Add character</h2>
      <div className="workspace-filter" aria-label="Character type">
        <button
          type="button"
          aria-label="Create a generic character"
          aria-pressed={kind === 'generic'}
          disabled={busy}
          onClick={() => {
            onKind('generic');
          }}
        >
          Generic
        </button>
        <button
          type="button"
          aria-label="Add a named character"
          aria-pressed={kind === 'named'}
          disabled={busy}
          onClick={() => {
            onKind('named');
          }}
        >
          Named
        </button>
        <button
          type="button"
          aria-pressed={kind === 'monster'}
          disabled={busy}
          onClick={() => {
            setQuery('');
            onKind('monster');
          }}
        >
          Monsters
        </button>
        <button
          type="button"
          aria-pressed={kind === 'enemy'}
          disabled={busy}
          onClick={() => {
            setQuery('');
            onKind('enemy');
          }}
        >
          Enemies
        </button>
      </div>
      {full ? (
        <p>The permanent party is full.</p>
      ) : kind === 'monster' || kind === 'enemy' ? (
        <>
          <label className="workspace-field">
            <span>Search {kind === 'monster' ? 'monsters' : 'enemies'}</span>
            <input
              value={query}
              disabled={busy}
              onChange={(event) => {
                setQuery(event.target.value);
                const first = creatureChoices.find(
                  (choice) =>
                    choice.available &&
                    choice.label
                      .toLowerCase()
                      .includes(event.target.value.toLowerCase()),
                );
                if (first)
                  onCreature({
                    formKey: first.key,
                    name: draft.creature?.name ?? '',
                  });
              }}
            />
          </label>
          <label className="workspace-field">
            <span>{kind === 'monster' ? 'Monster' : 'Enemy'}</span>
            <select
              disabled={busy || !draft.creature}
              value={draft.creature?.formKey ?? ''}
              onChange={(event) => {
                onCreature({
                  formKey: event.target.value,
                  name: draft.creature?.name ?? '',
                });
              }}
            >
              {visibleCreatures.map((choice) => (
                <option
                  key={choice.key}
                  value={choice.key}
                  disabled={!choice.available}
                >
                  {choice.label}
                  {choice.available
                    ? ''
                    : ` (${choice.unavailableReason ?? 'already in party'})`}
                </option>
              ))}
            </select>
          </label>
          {visibleCreatures.length === 0 && (
            <p>No matching {kind === 'monster' ? 'monsters' : 'enemies'}.</p>
          )}
          <label className="workspace-field">
            <span>Name (optional)</span>
            <input
              maxLength={15}
              disabled={busy || !draft.creature}
              value={draft.creature?.name ?? ''}
              onChange={(event) => {
                if (draft.creature)
                  onCreature({ ...draft.creature, name: event.target.value });
              }}
            />
          </label>
          <p className="workspace-add-note">
            {kind === 'monster'
              ? 'Experimental level-1 creatures. Appearance and battle control may not work.'
              : 'Experimental level-1 enemies. Boss commands, appearance and battle control may not work. High Seraph uses an experimental Ultima secondary command.'}
          </p>
        </>
      ) : kind === 'generic' ? (
        draft.generic ? (
          <label className="workspace-field">
            <span>New name</span>
            <input
              type="text"
              maxLength={15}
              disabled={busy}
              value={draft.generic.name}
              onChange={(event) => {
                onName(event.target.value);
              }}
            />
          </label>
        ) : (
          <p>
            This file has no generic character to use as a starting template.
          </p>
        )
      ) : draft.named ? (
        <>
          <label className="workspace-field">
            <span>Character</span>
            <select
              disabled={busy}
              value={draft.named.characterKey}
              onChange={(event) => {
                onCharacter(event.target.value);
              }}
            >
              {choices.map((choice) => (
                <option
                  key={choice.key}
                  value={choice.key}
                  disabled={!choice.available}
                >
                  {choice.label}
                  {choice.available
                    ? ''
                    : ` (${choice.unavailableReason ?? 'already in party'})`}
                </option>
              ))}
            </select>
          </label>
          <p className="workspace-add-note">
            Named additions are experimental. Story events are unchanged. Base
            stats come from a saved starting record.
          </p>
        </>
      ) : (
        <p>No named character can be added to this save.</p>
      )}
      {errors.length > 0 &&
        (((kind === 'monster' || kind === 'enemy') && draft.creature) ||
          (kind === 'named' && draft.named) ||
          (kind === 'generic' && draft.generic?.name)) && (
          <div role="alert">
            {errors.map((error) => (
              <p key={error}>{error}</p>
            ))}
          </div>
        )}
      <div className="workspace-modal-actions">
        <button
          type="button"
          className="workspace-secondary"
          disabled={busy}
          onClick={onCancel}
        >
          Cancel
        </button>
        <button
          type="button"
          disabled={
            busy ||
            invalid ||
            full ||
            (kind === 'monster' || kind === 'enemy'
              ? !draft.creature ||
                !visibleCreatures.some(
                  (choice) => choice.key === draft.creature?.formKey,
                )
              : kind === 'generic'
                ? !draft.generic || draft.generic.name.trim() === ''
                : !draft.named ||
                  !choices.some(
                    (choice) =>
                      choice.key === draft.named?.characterKey &&
                      choice.available,
                  ))
          }
          onClick={onAdd}
        >
          {busy ? 'Adding…' : 'Add and edit'}
        </button>
      </div>
    </>
  );
}
