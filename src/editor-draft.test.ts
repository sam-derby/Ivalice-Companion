import { describe, expect, test } from 'vitest';
import {
  collectDraft,
  emptyUnitDraft,
  storyGuestsLabel,
  storyPartyLabel,
  type EditorDraft,
} from './editor-draft';
import {
  draftContext,
  draftDocument,
  draftUnit,
  knownFact,
} from './draft-test-fixture';

function draft() {
  return {
    gil: '100',
    inventory: {},
    units: { 0: emptyUnitDraft() },
    generic: null,
    story: null,
    guest: null,
  } satisfies EditorDraft;
}

describe('date and achievement review', () => {
  const context = () => ({
    ...draftContext(),
    calendar: {
      month: 3,
      day: 21,
      monthLengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    },
    achievements: [
      { index: 1, description: 'Prologue.', unlocked: true, progress: 0 },
      { index: 8, description: 'Ramza 50.', unlocked: false, progress: 12 },
    ],
  });

  test('changed date and unlock states become operations', () => {
    const staged = {
      ...draft(),
      calendar: { month: 2, day: 28 },
      achievements: { 1: true, 8: true },
    };
    const review = collectDraft(draftDocument(), context(), staged);
    expect(review.errors).toEqual([]);
    expect(review.operations).toEqual([
      { kind: 'calendar_date', month: 2, day: 28 },
      { kind: 'achievement', index: 8, unlocked: true },
    ]);
    expect(review.changes).toEqual([
      {
        group: 'Game',
        label: 'Date',
        before: 'Month 3, day 21',
        after: 'Month 2, day 28',
      },
      {
        group: 'Achievements',
        label: 'Ramza 50.',
        before: 'Locked',
        after: 'Unlocked',
      },
    ]);
  });

  test('an impossible date is an error, an unchanged one is no operation', () => {
    const invalid = { ...draft(), calendar: { month: 2, day: 29 } };
    expect(collectDraft(draftDocument(), context(), invalid).errors).toEqual([
      'Choose a valid in-game date.',
    ]);
    const unchanged = { ...draft(), calendar: { month: 3, day: 21 } };
    expect(
      collectDraft(draftDocument(), context(), unchanged).operations,
    ).toEqual([]);
  });
});

describe('story step review', () => {
  const context = () => ({
    ...draftContext(),
    storyStep: 940,
    storyChoices: [
      {
        progress: 465,
        chapter: 'Chapter 2',
        objective: 'Free the captive.',
        guests: ['Agrias', 'Gaffgarion'],
        joins: [],
        leaves: ['Boco', 'Mustadio'],
      },
      { progress: 940, chapter: 'Chapter 3', objective: null, guests: [] },
    ],
  });

  test('a listed step becomes one story operation with labelled review', () => {
    const staged = { ...draft(), storyStep: '465' };
    const review = collectDraft(draftDocument(), context(), staged);
    expect(review.operations).toEqual([
      { kind: 'story_step', progress: '465' },
    ]);
    expect(review.changes).toEqual([
      {
        group: 'Game',
        label: 'Story step',
        before: '940 · Chapter 3',
        after: '465 · Chapter 2 · Free the captive.',
      },
      {
        group: 'Game',
        label: 'Guests',
        before: 'Current guests',
        after: 'Agrias, Gaffgarion',
      },
      {
        group: 'Game',
        label: 'Party',
        before: 'Current party',
        after: 'Leaves: Boco, Mustadio',
      },
    ]);
  });

  test('steps without guests or party changes say so', () => {
    expect(storyGuestsLabel(context(), 940)).toBe('No guests');
    expect(storyGuestsLabel(context(), 1)).toBe('No guests');
    expect(storyPartyLabel(context(), 940)).toBeNull();
    const both = {
      ...context(),
      storyChoices: [
        {
          progress: 720,
          chapter: null,
          objective: null,
          joins: ['Agrias'],
          leaves: ['Rapha'],
        },
      ],
    };
    expect(storyPartyLabel(both, 720)).toBe('Joins: Agrias · Leaves: Rapha');
  });

  test('the saved step is no change and unlisted steps are errors', () => {
    expect(
      collectDraft(draftDocument(), context(), {
        ...draft(),
        storyStep: '940',
      }).operations,
    ).toEqual([]);
    const review = collectDraft(draftDocument(), context(), {
      ...draft(),
      storyStep: '466',
    });
    expect(review.operations).toEqual([]);
    expect(review.errors).toEqual(['Choose a story step from the list.']);
  });
});

describe('character progression review', () => {
  test('level and EXP are independent from bases, jobs and JP', () => {
    const staged = draft();
    staged.units[0].level = '10';
    expect(
      collectDraft(draftDocument(), draftContext(), staged).operations,
    ).toEqual([{ kind: 'character_level', unitPosition: 0, value: '10' }]);
    staged.units[0].exp = '64';
    const review = collectDraft(draftDocument(), draftContext(), staged);
    expect(review.operations).toEqual([
      { kind: 'character_level', unitPosition: 0, value: '10' },
      { kind: 'experience', unitPosition: 0, value: '64' },
    ]);
    expect(
      review.changes.map((entry) => [entry.label, entry.before, entry.after]),
    ).toEqual([
      ['Level', '37', '10'],
      ['EXP', '73', '64'],
    ]);
  });

  test.each(['0', '100', '-1', '', '3.5'])(
    'rejects invalid level %s',
    (value) => {
      const staged = draft();
      staged.units[0].level = value;
      expect(
        collectDraft(draftDocument(), draftContext(), staged).errors,
      ).toHaveLength(1);
    },
  );

  test('stat operations contain bases while review contains final visible values', () => {
    const staged = draft();
    staged.units[0].statBases = { hp: 9_830_400 };
    const projected = draftUnit();
    projected.effective.hp = knownFact(720);
    const review = collectDraft(
      draftDocument(),
      draftContext(),
      staged,
      draftDocument(projected),
    );
    expect(review.operations).toEqual([
      { kind: 'base_stat', unitPosition: 0, stat: 'hp', value: 9_830_400 },
    ]);
    expect(review.changes).toEqual([
      { group: 'Ramza', label: 'HP', before: '540', after: '720' },
    ]);
    expect(JSON.stringify(review.changes)).not.toContain('9830400');
    staged.units[0].statBases = { hp: 8_847_360 };
    expect(
      collectDraft(draftDocument(), draftContext(), staged).operations,
    ).toEqual([]);
  });

  test('a completed unknown total is not described as still calculating', () => {
    const staged = draft();
    staged.units[0].statBases = { hp: 9_830_400 };
    const projected = draftUnit();
    projected.effective.hp = { value: { state: 'unknown' } };
    expect(
      collectDraft(
        draftDocument(),
        draftContext(),
        staged,
        draftDocument(projected),
      ).changes[0]?.after,
    ).toBe('Unknown');
  });

  test.each([-1, 0x1000000, 1.5])(
    'rejects invalid base override %s',
    (base) => {
      const staged = draft();
      staged.units[0].statBases = { hp: base };
      expect(
        collectDraft(draftDocument(), draftContext(), staged).errors,
      ).toHaveLength(1);
    },
  );
});
