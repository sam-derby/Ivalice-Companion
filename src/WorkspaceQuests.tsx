import { useState } from 'react';
import type { EditContext } from './reader-ipc';

type SideQuest = NonNullable<EditContext['sideQuests']>[number];
type Errand = NonNullable<EditContext['errands']>[number];
type CollectionEntry = NonNullable<EditContext['collection']>[number];

function questStatus(quest: SideQuest): string {
  const seen = quest.scenes.filter((scene) => scene.seen).length;
  if (seen === 0) return 'Not started';
  if (seen === quest.scenes.length) return 'Done';
  return `${String(seen)} of ${String(quest.scenes.length)} episodes`;
}

export function SideQuestList({ quests }: { quests: SideQuest[] }) {
  return (
    <article
      className="workspace-paper workspace-side-quests"
      aria-labelledby="workspace-side-quests-heading"
    >
      <h3 id="workspace-side-quests-heading">Side quests</h3>
      <p>
        Read from the save&apos;s event flags. Worked out from the game&apos;s
        scripts and not yet checked against a Chapter 4 save. Battles without a
        scene of their own, such as the Gollund colliery, aren&apos;t listed.
      </p>
      <ul>
        {quests.map((quest) => (
          <li key={quest.name}>
            <div className="workspace-side-quest-heading">
              <strong>{quest.name}</strong>
              <span>{questStatus(quest)}</span>
            </div>
            <ul aria-label={`${quest.name} episodes`}>
              {quest.scenes.map((scene) => (
                <li key={scene.label} data-seen={scene.seen || undefined}>
                  <span aria-hidden="true">{scene.seen ? '✓' : '○'}</span>{' '}
                  {scene.label}
                  <span className="visually-hidden">
                    {scene.seen ? ', seen' : ', not seen'}
                  </span>
                </li>
              ))}
            </ul>
            {quest.counter && (
              <small>
                {quest.counter.label}: {quest.counter.value}
              </small>
            )}
          </li>
        ))}
      </ul>
    </article>
  );
}

function ErrandList({ errands }: { errands: Errand[] }) {
  const [search, setSearch] = useState('');
  const needle = search.trim().toLocaleLowerCase();
  const shown = errands.filter((errand) =>
    `${errand.title} ${errand.client}`.toLocaleLowerCase().includes(needle),
  );
  return (
    <article
      className="workspace-paper workspace-errands"
      aria-labelledby="workspace-errands-heading"
    >
      <div className="workspace-panel-heading">
        <h3 id="workspace-errands-heading">Errands</h3>
        <span className="workspace-view-only">{errands.length} errands</span>
      </div>
      <p>
        Every tavern errand in the game. Which ones are available, under way or
        finished isn&apos;t decoded from the save yet.
      </p>
      <label className="workspace-quest-search">
        Search errands
        <input
          type="search"
          value={search}
          onChange={(event) => {
            setSearch(event.target.value);
          }}
        />
      </label>
      <ul>
        {shown.map((errand) => (
          <li key={errand.index}>
            <details>
              <summary>
                <strong>{errand.title}</strong>
                <small>{errand.client}</small>
              </summary>
              <p>{errand.posting}</p>
            </details>
          </li>
        ))}
      </ul>
      {shown.length === 0 && <p>No errands match.</p>}
    </article>
  );
}

function CollectionList({
  entries,
  kind,
  title,
}: {
  entries: CollectionEntry[];
  kind: CollectionEntry['kind'];
  title: string;
}) {
  const shown = entries.filter((entry) => entry.kind === kind);
  return (
    <section aria-label={title}>
      <h4>
        {title} <small>({shown.length})</small>
      </h4>
      <ul>
        {shown.map((entry) => (
          <li key={entry.index}>
            <details>
              <summary>{entry.name}</summary>
              <p>{entry.description}</p>
            </details>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function WorkspaceQuests({ context }: { context: EditContext | null }) {
  const quests = context?.sideQuests ?? [];
  const errands = context?.errands ?? [];
  const collection = context?.collection ?? [];
  return (
    <section
      className="workspace-quests"
      aria-labelledby="workspace-quests-heading"
    >
      <h2 className="visually-hidden" id="workspace-quests-heading">
        Quests
      </h2>
      <div className="workspace-quests-layout">
        <div className="workspace-quests-column">
          {quests.length > 0 && <SideQuestList quests={quests} />}
          {collection.length > 0 && (
            <article
              className="workspace-paper workspace-collection"
              aria-labelledby="workspace-collection-heading"
            >
              <h3 id="workspace-collection-heading">Artefacts and wonders</h3>
              <p>
                Found through errands. Which ones you have found isn&apos;t
                decoded from the save yet.
              </p>
              <CollectionList
                entries={collection}
                kind="artefact"
                title="Artefacts"
              />
              <CollectionList
                entries={collection}
                kind="wonder"
                title="Wonders"
              />
            </article>
          )}
        </div>
        {errands.length > 0 && <ErrandList errands={errands} />}
      </div>
      {quests.length === 0 && errands.length === 0 && (
        <p className="workspace-inline-note">
          Quest information is unavailable for this save.
        </p>
      )}
    </section>
  );
}
