import { describe, expect, test } from 'vitest';
import {
  abilityArt,
  characterSpriteArt,
  itemArt,
  jobArt,
  jobSpriteArt,
  monsterSpriteArt,
  parseReaderArt,
  portraitArt,
} from './reader-art';

describe('exact reader art mapping', () => {
  const source = {
    schema: 'reader_art_v1',
    jobs: {
      'job:75': {
        male: 'images/job-visual-702.png',
        female: 'images/job-visual-703.png',
      },
    },
    items: { 'item:1': 'images/item-1.png' },
    portraits: {
      'character:4': 'images/portrait-4.png',
      'character:120': 'images/portrait-120.png',
    },
    abilities: { 'ability:427': 'images/ability-icon-50.png' },
    job_sprites: { 'job:75': { male: 'images/sprite-job-75-male.png' } },
    character_sprites: { 'character:4': 'images/sprite-character-4.png' },
    monster_sprites: { 'job:94': 'images/sprite-monster-94.png' },
  };

  test('uses exact IDs and a neutral missing state', () => {
    const art = parseReaderArt(source);
    expect(art).not.toBeNull();
    expect(itemArt(art, 'item:1')).toContain('images/item-1.png');
    expect(itemArt(art, 'item:2')).toBeNull();
    expect(jobArt(art, 'job:75', 'male')).toContain(
      'images/job-visual-702.png',
    );
    expect(jobArt(art, 'job:75', 'female')).toContain(
      'images/job-visual-703.png',
    );
    expect(jobArt(art, 'job:75', null)).toBeNull();
    expect(jobArt(art, 'job:76', 'male')).toBeNull();
    expect(portraitArt(art, 'character:4')).toContain('images/portrait-4.png');
    expect(portraitArt(art, 'character:120')).toContain(
      'images/portrait-120.png',
    );
    expect(portraitArt(art, 'character:5')).toBeNull();
    expect(abilityArt(art, 'ability:427')).toContain(
      'images/ability-icon-50.png',
    );
    expect(abilityArt(art, 'ability:428')).toBeNull();
    expect(jobSpriteArt(art, 'job:75', 'male')).toContain('sprite-job-75-male');
    expect(jobSpriteArt(art, 'job:75', 'female')).toBeNull();
    expect(characterSpriteArt(art, 'character:4')).toContain(
      'sprite-character-4',
    );
    expect(monsterSpriteArt(art, 'job:94')).toContain('sprite-monster-94');
    expect(monsterSpriteArt(art, 'job:95')).toBeNull();
  });

  test('rejects paths that could show an unrelated or external image', () => {
    expect(
      parseReaderArt({ ...source, items: { 'item:1': '../other.png' } }),
    ).toBeNull();
    expect(
      parseReaderArt({
        ...source,
        portraits: { Delita: 'images/portrait-4.png' },
      }),
    ).toBeNull();
    expect(
      parseReaderArt({
        ...source,
        abilities: { 'ability:427': 'images/item-1.png' },
      }),
    ).toBeNull();
    expect(
      parseReaderArt({
        ...source,
        job_sprites: { 'job:75': { male: 'images/sprite-job-75-female.png' } },
      }),
    ).toBeNull();
    expect(
      parseReaderArt({
        ...source,
        monster_sprites: { 'job:94': 'images/sprite-character-4.png' },
      }),
    ).toBeNull();
  });
});
