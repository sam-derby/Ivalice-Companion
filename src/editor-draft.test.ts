import { describe, expect, test } from 'vitest';
import { collectDraft, emptyUnitDraft, type EditorDraft } from './editor-draft';
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
