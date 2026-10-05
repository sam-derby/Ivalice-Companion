import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, test } from 'vitest';
import { ReaderRoster, readerIdentityKey, rosterArt } from './ReaderRoster';
import { parseReaderArt } from './reader-art';
import type {
  CatalogueRef,
  Fact,
  ReaderDocument,
  ReaderUnit,
} from './reader-ipc';
import { savedUnitRecord } from './reader-test-record';

function unknown<T>(): Fact<T> {
  return { value: { state: 'unknown' } };
}

function known<T>(value: T): Fact<T> {
  return { value: { state: 'known', value } };
}

function reference(id: string, label: string): CatalogueRef {
  return {
    id,
    label: { state: 'known', value: label },
    description: {
      state: 'known',
      value: 'First line.\nSecond line <b>plain text</b>.',
    },
    asset_key: { state: 'unknown' },
  };
}

function unit(key: number, name = 'Étoile 太陽'): ReaderUnit {
  const bases = {
    hp: unknown<number>(),
    mp: unknown<number>(),
    speed: unknown<number>(),
    physical_attack: unknown<number>(),
    magical_attack: unknown<number>(),
  };
  const job = known(reference('job:88', 'Synthetic 月'));
  return {
    key,
    name: known({ text: name, origin: 'localized_catalogue' }),
    persistent_identity: unknown(),
    kind: unknown(),
    membership: known('party'),
    sprite: unknown(),
    portrait: unknown(),
    stored: {
      level: known(12),
      experience: unknown(),
      brave: unknown(),
      faith: unknown(),
      sex: unknown(),
      birthday: unknown(),
      zodiac: unknown(),
      statuses: unknown(),
      bases,
    },
    effective: {
      ...bases,
      movement_tiles: unknown(),
      jump_tiles: unknown(),
      evasion: unknown(),
    },
    current_job: job,
    growth: unknown(),
    jobs: known([
      {
        slot: 0,
        job,
        level: known(0),
        current_jp: known(0),
        total_jp: unknown(),
      },
    ]),
    learned_abilities: unknown(),
    saved_ability_flags: unknown(),
    abilities: {
      primary_command: unknown(),
      secondary_command: unknown(),
      reaction: known(reference('ability:441', 'Reaction 雪')),
      support: known(reference('ability:463', 'Support 月')),
      movement: known(reference('ability:487', 'Movement 星')),
    },
    equipment: {
      head: unknown(),
      body: unknown(),
      accessory: unknown(),
      right_weapon: unknown(),
      right_shield: unknown(),
      left_weapon: unknown(),
      left_shield: unknown(),
    },
    combat_sets: unknown(),
    selected_combat_set: unknown(),
    guidance: {
      target_job: unknown(),
      samurai_prerequisites: unknown(),
      steel_cost: unknown(),
      job_eligible: unknown(),
      ability_purchasable: unknown(),
    },
    saved: savedUnitRecord(),
  };
}

function document(units = [unit(7)]): ReaderDocument {
  return {
    schema: 'reader_v2',
    profile: 'english_steam_enhanced_manual',
    identity: {
      session: 'synthetic',
      snapshot_generation: 1,
      resource_generation: 1,
      resource_token: { state: 'known', value: 'a'.repeat(64) },
      manual_slot: 0,
    },
    roster: known(units),
    inventory: unknown(),
    gil: unknown(),
    progress: {
      title: unknown(),
      saved_at_unix_seconds: unknown(),
      hero_name: unknown(),
      location: unknown(),
      difficulty: unknown(),
      difficulty_code: unknown(),
      chapter: unknown(),
      story_progress: unknown(),
      objective: unknown(),
      area_index: unknown(),
      ramza_level: unknown(),
      story: unknown(),
      play_time_seconds: unknown(),
      next_event_id: unknown(),
      unnamed_event_values: unknown(),
      errands: unknown(),
      events: unknown(),
      recruitment: unknown(),
    },
  };
}

function view(
  reader: ReaderDocument,
  spoilerLevel: 'minimal' | 'hints' | 'gameplay' | 'full' = 'full',
) {
  // Match App's scope boundary: a changed snapshot/resource/visibility resets selection.
  return (
    <ReaderRoster
      key={`${readerIdentityKey(reader.identity)}:${spoilerLevel}`}
      reader={reader}
    />
  );
}

function chooseMember(position = 1) {
  fireEvent.click(
    screen.getByRole('button', {
      name: new RegExp(`^Member ${String(position)} `),
    }),
  );
}

function chooseTab(name: string) {
  fireEvent.click(screen.getByRole('tab', { name }));
  return screen.getByRole('tabpanel', { name });
}

function chooseSection(name: 'Party' | 'Inventory' | 'Save overview') {
  fireEvent.click(screen.getByRole('button', { name }));
}

function saveSection() {
  chooseSection('Save overview');
  return screen.getByRole('region', { name: 'Save overview' });
}

function inventorySection() {
  chooseSection('Inventory');
  return screen.getByRole('region', { name: 'Party inventory' });
}

function detail(panel: HTMLElement, label: string) {
  return within(panel).getByText(label, { selector: 'dt' }).nextElementSibling
    ?.textContent;
}

describe('selected unit detail tabs', () => {
  test('shows distinct neutral and monster avatars when no exact portrait exists', () => {
    const generic = unit(7, 'Generic recruit');
    const monster = unit(8, 'Monster recruit');
    monster.kind = known('monster');
    monster.portrait = known('character:4');
    const art = parseReaderArt({
      schema: 'reader_art_v1',
      jobs: {},
      items: {},
      portraits: { 'character:4': 'images/portrait-4.png' },
    });
    expect(rosterArt(art, monster).src).toBeNull();
    render(view(document([generic, monster])));
    const genericAvatar = screen.getByRole('img', {
      name: 'Generic recruit portrait represented by a neutral silhouette',
    });
    const monsterAvatar = screen.getByRole('img', {
      name: 'Monster recruit portrait represented by a monster silhouette',
    });
    expect(genericAvatar.querySelector('svg')).not.toBeNull();
    expect(monsterAvatar.querySelector('svg')).not.toBeNull();
    expect(genericAvatar.querySelector('img')).toBeNull();
    expect(monsterAvatar.querySelector('img')).toBeNull();
    chooseMember(2);
    expect(
      screen.getAllByRole('img', {
        name: 'Monster recruit portrait represented by a monster silhouette',
      }),
    ).toHaveLength(2);
  });

  test('keeps roster tags but omits raw membership and learned flags from unit tabs', () => {
    const member = unit(7);
    member.membership = known('guest');
    member.kind = known('monster');
    member.saved_ability_flags = known([
      { slot: 0, active_positions: [0, 15], passive_positions: [0, 7] },
    ]);
    render(view(document([member])));
    const button = screen.getByRole('button', {
      name: /Member 1.*Guest Monster/,
    });
    expect(button.textContent).toContain('Guest · Monster');
    fireEvent.click(button);
    const overview = screen.getByRole('tabpanel', { name: 'Overview' });
    expect(within(overview).queryByText('Roster status')).toBeNull();
    expect(within(overview).queryByText('More unit information')).toBeNull();
    expect(detail(overview, 'Sex')).toBe('Unknown');
    expect(detail(overview, 'Zodiac')).toBe('Unknown');
    const panel = chooseTab('Abilities');
    expect(panel.textContent).not.toContain('Saved learned flags');
    expect(panel.textContent).not.toContain('Storage slot');
  });
  test('stored status belongs to the selected member and preserves zero and unknown', () => {
    const first = unit(7);
    first.stored.experience = known(0);
    first.stored.brave = known(100);
    first.stored.faith = known(0);
    const second = unit(21, 'Second');
    second.stored.experience = known(170);
    second.stored.faith = known(53);
    render(view(document([first, second])));
    chooseMember();
    let panel = screen.getByRole('tabpanel', { name: 'Overview' });
    expect(detail(panel, 'XP')).toBe('0');
    expect(detail(panel, 'Bravery')).toBe('100');
    expect(detail(panel, 'Faith')).toBe('0');
    chooseMember(2);
    panel = screen.getByRole('tabpanel', { name: 'Overview' });
    expect(detail(panel, 'XP')).toBe('170');
    expect(detail(panel, 'Bravery')).toBe('Unknown');
    expect(detail(panel, 'Faith')).toBe('53');
  });

  test('shows game-displayed stats without raw saved bases', () => {
    const member = unit(7);
    member.stored.bases.hp = known(1_234_567);
    member.effective.hp = known(42);
    member.effective.movement_tiles = known(5);
    member.effective.evasion = known([
      {
        source: 'character',
        physical_basis_points: known(1000),
        magical_basis_points: known(0),
      },
    ]);
    render(view(document([member])));
    chooseMember();
    const panel = screen.getByRole('tabpanel', { name: 'Overview' });
    expect(detail(panel, 'HP')).toBe('42');
    expect(detail(panel, 'Move')).toBe('5');
    expect(detail(panel, 'Character')).toBe('Physical 10% · Magical 0%');
    expect(panel.textContent).not.toContain('Raw saved base values');
    expect(panel.textContent).not.toContain('1234567');
  });

  test.each(['minimal', 'hints', 'gameplay'] as const)(
    'stored status remains visible at %s',
    (ceiling) => {
      const member = unit(7);
      for (const field of ['experience', 'brave', 'faith'] as const) {
        member.stored[field] = known(97);
      }
      const reader = document([member]);
      render(view(reader, ceiling));
      chooseMember();
      const panel = screen.getByRole('tabpanel', { name: 'Overview' });
      for (const label of ['XP', 'Bravery', 'Faith']) {
        expect(detail(panel, label)).toBe('97');
      }
    },
  );

  test('tabs use arrow, Home and End focus with one selected panel and preserve Overview selectors', () => {
    render(view(document()));
    chooseMember();
    const overview = screen.getByRole('tab', { name: 'Overview' });
    expect(detail(screen.getByRole('tabpanel'), 'Name')).toBe('Étoile 太陽');
    expect(detail(screen.getByRole('tabpanel'), 'Current job')).toBe(
      'Synthetic 月',
    );
    expect(detail(screen.getByRole('tabpanel'), 'Level')).toBe('12');
    overview.focus();
    fireEvent.keyDown(overview, { key: 'ArrowRight' });
    expect(globalThis.document.activeElement).toBe(
      screen.getByRole('tab', { name: 'Jobs' }),
    );
    expect(screen.getByRole('tabpanel', { name: 'Jobs' }).id).toBe(
      'reader-panel-jobs',
    );
    fireEvent.keyDown(globalThis.document.activeElement ?? overview, {
      key: 'End',
    });
    expect(globalThis.document.activeElement).toBe(
      screen.getByRole('tab', { name: 'Equipment' }),
    );
    fireEvent.keyDown(globalThis.document.activeElement ?? overview, {
      key: 'ArrowRight',
    });
    expect(globalThis.document.activeElement).toBe(overview);
    fireEvent.keyDown(overview, { key: 'ArrowLeft' });
    expect(globalThis.document.activeElement).toBe(
      screen.getByRole('tab', { name: 'Equipment' }),
    );
    fireEvent.keyDown(globalThis.document.activeElement ?? overview, {
      key: 'Home',
    });
    expect(globalThis.document.activeElement).toBe(overview);
    expect(screen.getAllByRole('tabpanel')).toHaveLength(1);
    expect(
      screen.getAllByRole('tab').filter((tab) => tab.tabIndex === 0),
    ).toEqual([overview]);
  });

  test('partial jobs preserve zero, unknown total and distinct current/total values', () => {
    const first = unit(7);
    const second = unit(19);
    second.jobs = known([
      {
        slot: 0,
        job: second.current_job,
        level: unknown(),
        current_jp: known(11),
        total_jp: known(907),
      },
    ]);
    render(view(document([first, second])));
    chooseMember();
    let panel = chooseTab('Jobs');
    expect(
      panel.querySelector('.job-progress-card h4')?.textContent,
    ).toBeTruthy();
    expect(panel.textContent).not.toContain('Storage slot');
    expect(detail(panel, 'Job level')).toBe('0');
    expect(detail(panel, 'Spendable JP')).toBe('0');
    expect(detail(panel, 'Job EXP')).toBe('Unknown');
    chooseMember(2);
    expect(screen.getByRole('tabpanel', { name: 'Overview' })).toBeTruthy();
    panel = chooseTab('Jobs');
    expect(detail(panel, 'Spendable JP')).toBe('11');
    expect(detail(panel, 'Job EXP')).toBe('907');
    expect(detail(panel, 'Job level')).toBe('Unknown');
  });

  test('passive labels and plain descriptions do not assert learned state or commands', () => {
    render(view(document()));
    chooseMember();
    const panel = chooseTab('Abilities');
    expect(detail(panel, 'Reaction')).toBe('Reaction 雪');
    expect(detail(panel, 'Support')).toBe('Support 月');
    expect(detail(panel, 'Movement')).toBe('Movement 星');
    expect(detail(panel, 'Primary command')).toBe('Command not available yet');
    expect(detail(panel, 'Secondary command')).toBe('Unknown');
    expect(within(panel).queryByText('Learned abilities')).toBeNull();
    expect(panel.textContent).toContain('do not establish learned state');
    expect(
      within(panel).getByLabelText('Reaction description').textContent,
    ).toBe('First line.\nSecond line <b>plain text</b>.');
    const reactionCard = within(panel)
      .getByLabelText('Reaction description')
      .closest('.ability-detail');
    expect(reactionCard?.querySelector('dt')?.textContent).toBe('Reaction');
    expect(reactionCard?.querySelector('dd')?.textContent).toBe('Reaction 雪');
    expect(panel.querySelector('b')).toBeNull();
  });

  test('strips catalogue color codes from a nearby ability description', () => {
    const member = unit(7);
    const reaction = member.abilities.reaction.value;
    if (reaction.state !== 'known') throw new Error('Missing reaction fixture');
    reaction.value.description = known('Range: <color=151>2</color>').value;
    render(view(document([member])));
    chooseMember();
    const panel = chooseTab('Abilities');
    expect(
      within(panel).getByLabelText('Reaction description').textContent,
    ).toBe('Range: 2');
  });

  test('absent, unsupported and unknown passives stay distinct without raw-ID fallback', () => {
    const member = unit(7);
    member.abilities.reaction = { ...unknown(), value: { state: 'absent' } };
    member.abilities.support = {
      ...unknown(),
      value: { state: 'unsupported' },
    };
    member.abilities.movement = known({
      ...reference('ability:65000', 'unused'),
      label: { state: 'unknown' },
    });
    render(view(document([member])));
    chooseMember();
    const panel = chooseTab('Abilities');
    expect(detail(panel, 'Reaction')).toBe('None');
    expect(detail(panel, 'Support')).toBe('Unsupported');
    expect(detail(panel, 'Movement')).toBe('Unknown');
    expect(panel.textContent).not.toContain('65000');
    expect(within(panel).queryByLabelText('Movement description')).toBeNull();
  });

  test('unknown and empty job collections make no full recorded-job claim', () => {
    const first = unit(7);
    first.jobs = unknown();
    const second = unit(19);
    second.jobs = known([]);
    render(view(document([first, second])));
    chooseMember();
    expect(chooseTab('Jobs').textContent).toContain('Job progress: Unknown');
    chooseMember(2);
    expect(chooseTab('Jobs').textContent).toContain('No job progress yet');
  });

  test('switching duplicate-named units resets the tab and never retains previous details', () => {
    const second = unit(19);
    second.abilities.reaction = known(
      reference('ability:442', 'Different reaction'),
    );
    const longName = '太陽'.repeat(100);
    const first = unit(7, longName);
    second.name = first.name;
    render(view(document([first, second])));
    chooseMember();
    expect(detail(chooseTab('Abilities'), 'Reaction')).toBe('Reaction 雪');
    chooseMember(2);
    expect(screen.queryByRole('tabpanel', { name: 'Abilities' })).toBeNull();
    expect(
      detail(screen.getByRole('tabpanel', { name: 'Overview' }), 'Name'),
    ).toBe(longName);
    expect(detail(chooseTab('Abilities'), 'Reaction')).toBe(
      'Different reaction',
    );
    expect(screen.queryByText('Reaction 雪')).toBeNull();
  });

  test.each(['minimal', 'hints', 'gameplay'] as const)(
    'shows player save details at %s',
    (ceiling) => {
      const member = unit(7);
      const reader = document([member]);
      const rendered = render(view(reader));
      chooseMember();
      chooseTab('Abilities');
      rendered.rerender(view(reader, ceiling));
      expect(screen.queryByRole('tabpanel')).toBeNull();
      chooseMember();
      expect(
        chooseTab('Jobs').querySelector('.job-progress-card'),
      ).toBeTruthy();
      expect(chooseTab('Abilities').textContent).toContain('Reaction');
      for (const text of [
        'Étoile 太陽',
        'Synthetic 月',
        'Reaction 雪',
        'Support 月',
        'Movement 星',
        'First line.',
      ]) {
        expect(rendered.container.textContent).toContain(text);
      }
    },
  );

  test('a replaced resource or snapshot clears selection rather than transferring by unit key', () => {
    const reader = document();
    const rendered = render(view(reader));
    chooseMember();
    chooseTab('Jobs');
    const replacement = {
      ...reader,
      identity: {
        ...reader.identity,
        resource_generation: 2,
        resource_token: { state: 'known' as const, value: 'b'.repeat(64) },
      },
    };
    rendered.rerender(view(replacement));
    expect(screen.queryByRole('tabpanel')).toBeNull();
    chooseMember();
    chooseTab('Abilities');
    rendered.rerender(
      view({
        ...replacement,
        identity: {
          ...replacement.identity,
          snapshot_generation: 2,
          manual_slot: 1,
        },
      }),
    );
    expect(screen.queryByRole('tabpanel')).toBeNull();
  });
});

function equippedUnit(key: number): ReaderUnit {
  const member = unit(key);
  member.equipment.body = known(reference('item:193', 'Synthetic tunic 雪'));
  member.equipment.accessory = known(
    reference('item:215', 'Synthetic armlet 月'),
  );
  member.equipment.right_weapon = { ...unknown(), value: { state: 'absent' } };
  member.equipment.right_shield = { ...unknown(), value: { state: 'absent' } };
  member.equipment.left_weapon = { ...unknown(), value: { state: 'absent' } };
  member.equipment.left_shield = { ...unknown(), value: { state: 'absent' } };
  return member;
}

describe('partial selected-unit equipment', () => {
  test('shows five ordered positions and keeps descriptions beside their items', () => {
    render(view(document([equippedUnit(0)])));
    const button = screen.getByRole('button', { name: /^Member 1 / });
    expect(button.getAttribute('data-unit-key')).toBe('0');
    fireEvent.click(button);
    const panel = chooseTab('Equipment');
    expect(panel.id).toBe('reader-panel-equipment');
    expect(screen.getByRole('tab', { name: 'Equipment' }).id).toBe(
      'reader-tab-equipment',
    );
    expect(panel.textContent).toContain('Saved equipment slots');
    expect(detail(panel, 'Head')).toBe('Unknown');
    expect(detail(panel, 'Body')).toBe('Synthetic tunic 雪');
    expect(detail(panel, 'Accessory')).toBe('Synthetic armlet 月');
    for (const slot of ['Right arm', 'Left arm']) {
      expect(detail(panel, slot)).toBe('Empty');
    }
    expect(
      Array.from(panel.querySelectorAll('.equipment-detail dt')).map(
        (item) => item.textContent,
      ),
    ).toEqual(['Right arm', 'Left arm', 'Head', 'Body', 'Accessory']);
    expect(within(panel).getByLabelText('Body description').textContent).toBe(
      'First line.\nSecond line <b>plain text</b>.',
    );
    expect(
      within(panel).getByLabelText('Accessory description').closest('dd'),
    ).toBeNull();
    expect(panel.querySelector('b')).toBeNull();
    expect(panel.textContent).toContain('Saved equipment sets: Unknown');
    expect(panel.textContent).not.toContain('item:');
    expect(screen.queryByRole('tab', { name: 'Inventory' })).toBeNull();
  });

  test('unmapped or missing names remain unknown and never become numeric item names', () => {
    const member = equippedUnit(13);
    member.equipment.body = known({
      ...reference('item:65000', 'unused'),
      label: { state: 'unknown' },
    });
    member.equipment.accessory = unknown();
    member.equipment.right_weapon = {
      ...unknown(),
      value: { state: 'unsupported' },
    };
    render(view(document([member])));
    chooseMember();
    const panel = chooseTab('Equipment');
    expect(detail(panel, 'Body')).toBe('Unknown');
    expect(detail(panel, 'Accessory')).toBe('Unknown');
    expect(detail(panel, 'Right arm')).toBe('Unsupported');
    expect(panel.textContent).not.toContain('65000');
    expect(within(panel).queryByLabelText('Body description')).toBeNull();
    expect(within(panel).queryByLabelText('Accessory description')).toBeNull();
  });

  test('shows both saved arm items if a conflicting record contains both', () => {
    const member = equippedUnit(13);
    member.equipment.right_weapon = known(reference('item:1', 'Sword'));
    member.equipment.right_shield = known(reference('item:2', 'Shield'));
    render(view(document([member])));
    chooseMember();
    const panel = chooseTab('Equipment');
    expect(detail(panel, 'Right arm')).toBe('Sword / Shield');
    expect(
      within(panel).getByLabelText('Right arm shield description'),
    ).toBeTruthy();
  });

  test('unit and resource changes reset the equipment tab and clear the prior item details', () => {
    const reader = document([equippedUnit(0), unit(23)]);
    const rendered = render(view(reader));
    chooseMember();
    chooseTab('Equipment');
    const second = screen.getByRole('button', { name: /^Member 2 / });
    expect(second.getAttribute('data-unit-key')).toBe('23');
    fireEvent.click(second);
    expect(screen.getByRole('tabpanel', { name: 'Overview' })).toBeTruthy();
    expect(detail(chooseTab('Equipment'), 'Body')).toBe('Unknown');
    expect(rendered.container.textContent).not.toContain('Synthetic tunic 雪');
    chooseMember();
    chooseTab('Equipment');
    rendered.rerender(
      view({
        ...reader,
        identity: {
          ...reader.identity,
          resource_generation: 2,
          snapshot_generation: 2,
          manual_slot: 1,
        },
      }),
    );
    expect(screen.queryByRole('tabpanel')).toBeNull();
    expect(rendered.container.textContent).not.toContain('Synthetic tunic 雪');
  });

  test.each(['minimal', 'hints', 'gameplay'] as const)(
    'shows player equipment at %s',
    (ceiling) => {
      const member = equippedUnit(0);
      const reader = document([member]);
      const rendered = render(view(reader));
      chooseMember();
      chooseTab('Equipment');
      rendered.rerender(view(reader, ceiling));
      expect(screen.queryByRole('tabpanel')).toBeNull();
      chooseMember();
      const panel = chooseTab('Equipment');
      expect(panel.textContent).toContain('Saved equipment slots');
      expect(panel.querySelector('dl')).not.toBeNull();
      for (const text of [
        'Synthetic tunic',
        'Synthetic armlet',
        'First line.',
        'Empty',
      ]) {
        expect(panel.textContent).toContain(text);
      }
    },
  );
});

describe('party counts and saved combat sets', () => {
  test('omits unverified playtime from the compact save overview', () => {
    const reader = document();
    reader.progress.chapter = known('Chapter III');
    reader.progress.location = known(reference('area:7', 'Zeltennia'));
    reader.progress.play_time_seconds = known(5_400);
    reader.progress.ramza_level = known(29);
    reader.progress.next_event_id = known(101);
    render(view(reader));
    const section = saveSection();
    const main = section.querySelector('.overview-facts');
    if (!(main instanceof HTMLElement)) throw new Error('Missing overview');
    expect(
      [...main.querySelectorAll('dt')].map((entry) => entry.textContent),
    ).toEqual(['Chapter', 'Current area', 'Ramza level']);
    expect(detail(main, 'Chapter')).toBe('Chapter III');
    expect(detail(main, 'Current area')).toBe('Zeltennia');
    expect(main.textContent).not.toContain('Play time');
    expect(detail(main, 'Ramza level')).toBe('29');
    expect(
      section.querySelector('.save-development-details')?.hasAttribute('open'),
    ).toBe(false);
    expect(detail(section, 'Next event ID')).toBe('101');
  });

  test.each([0, 1, 2, 3, 255])(
    'hides difficulty code %s and keeps chapter/area unknown',
    (code) => {
      const reader = document();
      reader.progress.difficulty_code = known(code);
      reader.progress.next_event_id = known(451);
      render(view(reader));
      const section = saveSection();
      const main = section.querySelector('.overview-facts');
      if (!(main instanceof HTMLElement)) throw new Error('Missing overview');
      expect(detail(main, 'Chapter')).toBe('Not available yet');
      expect(detail(main, 'Current area')).toBe('Not available yet');
      expect(main.textContent).not.toContain('451');
      expect(screen.queryByText('Difficulty')).toBeNull();
      expect(screen.queryByText('Stored difficulty code')).toBeNull();
    },
  );

  test('shows decoded gil, including zero, in the save overview', () => {
    const reader = document();
    reader.gil = known(42_500);
    const rendered = render(view(reader));
    expect(detail(saveSection(), 'Gil')).toBe('42500');
    rendered.rerender(view({ ...reader, gil: known(0) }));
    expect(detail(saveSection(), 'Gil')).toBe('0');
  });

  test('shows nonzero party counts and identifies unresolved positions', () => {
    const reader = document();
    reader.inventory = known([
      { key: 0, item: unknown(), category: unknown(), quantity: known(0) },
      { key: 1, item: unknown(), category: unknown(), quantity: known(255) },
      { key: 260, item: unknown(), category: unknown(), quantity: known(1) },
    ]);
    render(view(reader));
    const section = inventorySection();
    expect(section.textContent).toContain(
      '2 inventory entries with saved quantities',
    );
    expect(section.textContent).toContain('position 1');
    expect(section.textContent).toContain('position 260');
    expect(section.textContent).toContain('255');
    expect(section.textContent).not.toContain('position 0');
    expect(detail(saveSection(), 'Gil')).toBe('Unknown');
  });

  test('searches party holdings and filters verified categories without assigning unknown ones', () => {
    const reader = document();
    reader.inventory = known([
      {
        key: 1,
        item: known(reference('item:1', 'Potion')),
        category: known('Consumable'),
        quantity: known(4),
      },
      {
        key: 2,
        item: known(reference('item:2', 'Bronze Sword')),
        category: known('Weapon'),
        quantity: known(1),
      },
      { key: 3, item: unknown(), category: unknown(), quantity: known(2) },
    ]);
    render(view(reader));
    const section = inventorySection();
    const search = screen.getByRole('searchbox', {
      name: 'Search party inventory',
    });
    const category = screen.getByRole('combobox', { name: 'Category' });
    fireEvent.change(search, { target: { value: 'sword' } });
    expect(section.textContent).toContain('Bronze Sword');
    expect(section.textContent).not.toContain('Potion');
    fireEvent.change(search, { target: { value: '' } });
    fireEvent.change(category, { target: { value: 'category:Consumable' } });
    expect(section.textContent).toContain('Potion');
    expect(section.textContent).not.toContain('Bronze Sword');
    fireEvent.change(category, { target: { value: 'unknown' } });
    expect(section.textContent).toContain('position 3');
    expect(section.textContent).not.toContain('Potion');
    fireEvent.change(search, { target: { value: 'missing' } });
    expect(section.textContent).toContain('No matching holdings.');
  });

  test('keeps each saved set separate from the equipped slots, including an empty name', () => {
    const member = equippedUnit(0);
    member.combat_sets = known([
      {
        key: 0,
        assigned: known(true),
        name: { value: { state: 'absent' } },
        job: member.current_job,
        head: known(reference('item:1', 'Set helm')),
        body: unknown(),
        accessory: unknown(),
        right_hand: unknown(),
        left_hand: unknown(),
        abilities: member.abilities,
        double_hand: known(true),
      },
    ]);
    render(view(document([member])));
    chooseMember();
    const panel = chooseTab('Sets');
    expect(
      within(panel).getByText('Set 1', { selector: 'summary' }),
    ).toBeTruthy();
    expect(detail(panel, 'Head')).toBe('Set helm');
    expect(detail(panel, 'Double hand')).toBe('Yes');
    expect(chooseTab('Equipment').textContent).not.toContain('Set helm');
  });

  test('collapses unassigned sets to one line', () => {
    const member = unit(7);
    member.combat_sets = known([
      {
        key: 0,
        assigned: known(false),
        name: { value: { state: 'absent' } },
        job: unknown(),
        head: unknown(),
        body: unknown(),
        accessory: unknown(),
        right_hand: unknown(),
        left_hand: unknown(),
        abilities: member.abilities,
        double_hand: known(false),
      },
    ]);
    render(view(document([member])));
    chooseMember();
    const panel = chooseTab('Sets');
    expect(panel.textContent).toBe('Unassigned');
    expect(panel.querySelector('details')).toBeNull();
  });
});

describe('save-level timestamp', () => {
  test.each([
    [1, '1970-01-01T00:00:01.000Z', '1970-01-01 00:00:01 UTC'],
    [2_147_483_647, '2038-01-19T03:14:07.000Z', '2038-01-19 03:14:07 UTC'],
  ])(
    'renders accepted endpoint %i in UTC without a local-time adjustment',
    (seconds, iso, text) => {
      const reader = document([]);
      reader.progress.saved_at_unix_seconds = known(seconds);
      const rendered = render(view(reader));
      saveSection();
      const time = rendered.container.querySelector('#reader-saved-at');
      expect(time?.tagName).toBe('TIME');
      expect(time?.getAttribute('datetime')).toBe(iso);
      expect(time?.textContent).toBe(text);
      expect(detail(saveSection(), 'Saved at (UTC)')).toBe(text);
      expect(screen.queryByRole('tabpanel')).toBeNull();
    },
  );

  test.each([
    0,
    -1,
    -2_147_483_648,
    2_147_483_648,
    1.5,
    Number.NaN,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
    Number.MAX_SAFE_INTEGER,
  ])(
    'rejects unaccepted or malformed timestamp %s without a time element',
    (seconds) => {
      const reader = document();
      reader.progress.saved_at_unix_seconds = known(seconds);
      const rendered = render(view(reader));
      expect(rendered.container.querySelector('#reader-saved-at')).toBeNull();
      expect(detail(saveSection(), 'Saved at (UTC)')).toBe('Unknown');
    },
  );

  test.each([
    ['unknown', 'Unknown'],
    ['absent', 'None'],
    ['unsupported', 'Unsupported'],
  ] as const)('preserves the explicit %s state', (state, text) => {
    const reader = document();
    reader.progress.saved_at_unix_seconds = { ...unknown(), value: { state } };
    const rendered = render(view(reader));
    expect(rendered.container.querySelector('time')).toBeNull();
    expect(detail(saveSection(), 'Saved at (UTC)')).toBe(text);
  });

  test.each(['minimal', 'hints', 'gameplay'] as const)(
    'shows a known player save time at %s',
    (ceiling) => {
      const reader = document();
      reader.progress.saved_at_unix_seconds = known(1);
      const rendered = render(view(reader, ceiling));
      saveSection();
      expect(rendered.container.querySelector('time')).not.toBeNull();
      expect(rendered.container.textContent).toContain('1970');
      expect(detail(saveSection(), 'Saved at (UTC)')).toBe(
        '1970-01-01 00:00:01 UTC',
      );
    },
  );

  test('slot and resource reloads render only the current response time, then clear it for unknown', () => {
    const reader = document();
    reader.progress.saved_at_unix_seconds = known(1);
    const rendered = render(view(reader));
    saveSection();
    const next = {
      ...reader,
      identity: { ...reader.identity, manual_slot: 1, snapshot_generation: 2 },
      progress: { ...reader.progress, saved_at_unix_seconds: known(2) },
    };
    rendered.rerender(view(next));
    saveSection();
    expect(
      rendered.container.querySelector('time')?.getAttribute('datetime'),
    ).toBe('1970-01-01T00:00:02.000Z');
    expect(rendered.container.textContent).not.toContain('00:00:01');
    rendered.rerender(
      view({
        ...next,
        identity: { ...next.identity, resource_generation: 2 },
        progress: { ...next.progress, saved_at_unix_seconds: unknown() },
      }),
    );
    saveSection();
    expect(rendered.container.querySelector('time')).toBeNull();
    expect(rendered.container.textContent).not.toContain('1970');
  });
});
