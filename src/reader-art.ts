import { useEffect, useState } from 'react';

export interface ReaderArt {
  schema: 'reader_art_v1';
  jobs: Record<string, { male?: string; female?: string }>;
  items: Record<string, string>;
  portraits: Record<string, string>;
  abilities: Record<string, string>;
  jobSprites: Record<string, { male?: string; female?: string }>;
  characterSprites: Record<string, string>;
  monsterSprites: Record<string, string>;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function imagePath(
  value: unknown,
  kind:
    | 'job-visual'
    | 'item'
    | 'portrait'
    | 'ability-icon'
    | 'sprite-job'
    | 'sprite-character'
    | 'sprite-monster',
): value is string {
  if (typeof value !== 'string' || !value.startsWith(`images/${kind}-`))
    return false;
  if (kind === 'sprite-job')
    return /^images\/sprite-job-[0-9]{1,3}-(?:male|female)\.png$/.test(value);
  if (kind === 'sprite-character')
    return /^images\/sprite-character-[0-9]{1,3}\.png$/.test(value);
  if (kind === 'sprite-monster')
    return /^images\/sprite-monster-[0-9]{1,3}\.png$/.test(value);
  return /^images\/(?:job-visual|item|portrait|ability-icon)-[0-9]{1,3}\.png$/.test(
    value,
  );
}

export function parseReaderArt(value: unknown): ReaderArt | null {
  if (
    !record(value) ||
    value.schema !== 'reader_art_v1' ||
    !record(value.jobs) ||
    !record(value.items) ||
    !record(value.portraits) ||
    (value.abilities !== undefined && !record(value.abilities)) ||
    (value.job_sprites !== undefined && !record(value.job_sprites)) ||
    (value.character_sprites !== undefined &&
      !record(value.character_sprites)) ||
    (value.monster_sprites !== undefined && !record(value.monster_sprites))
  ) {
    return null;
  }
  const jobs: ReaderArt['jobs'] = {};
  const items: ReaderArt['items'] = {};
  const portraits: ReaderArt['portraits'] = {};
  const abilities: ReaderArt['abilities'] = {};
  const jobSprites: ReaderArt['jobSprites'] = {};
  const characterSprites: ReaderArt['characterSprites'] = {};
  const monsterSprites: ReaderArt['monsterSprites'] = {};
  for (const [key, item] of Object.entries(value.jobs)) {
    if (!/^job:[0-9]{1,3}$/.test(key) || !record(item)) return null;
    const mapped: { male?: string; female?: string } = {};
    for (const [sex, file] of Object.entries(item)) {
      if (
        (sex !== 'male' && sex !== 'female') ||
        !imagePath(file, 'job-visual')
      )
        return null;
      mapped[sex] = file;
    }
    jobs[key] = mapped;
  }
  for (const [key, file] of Object.entries(value.items)) {
    if (!/^item:[0-9]{1,3}$/.test(key) || !imagePath(file, 'item')) return null;
    items[key] = file;
  }
  for (const [key, file] of Object.entries(value.portraits)) {
    if (!/^character:[0-9]{1,3}$/.test(key) || !imagePath(file, 'portrait'))
      return null;
    portraits[key] = file;
  }
  for (const [key, file] of Object.entries(value.abilities ?? {})) {
    if (!/^ability:[0-9]{1,3}$/.test(key) || !imagePath(file, 'ability-icon'))
      return null;
    abilities[key] = file;
  }
  for (const [key, item] of Object.entries(value.job_sprites ?? {})) {
    if (!/^job:[0-9]{1,3}$/.test(key) || !record(item)) return null;
    const mapped: { male?: string; female?: string } = {};
    for (const [sex, file] of Object.entries(item)) {
      if (
        (sex !== 'male' && sex !== 'female') ||
        !imagePath(file, 'sprite-job') ||
        file !== `images/sprite-job-${key.slice(4)}-${sex}.png`
      )
        return null;
      mapped[sex] = file;
    }
    jobSprites[key] = mapped;
  }
  for (const [key, file] of Object.entries(value.character_sprites ?? {})) {
    if (
      !/^character:[0-9]{1,3}$/.test(key) ||
      !imagePath(file, 'sprite-character') ||
      file !== `images/sprite-character-${key.slice(10)}.png`
    )
      return null;
    characterSprites[key] = file;
  }
  for (const [key, file] of Object.entries(value.monster_sprites ?? {})) {
    if (!/^job:[0-9]{1,3}$/.test(key) || !imagePath(file, 'sprite-monster'))
      return null;
    monsterSprites[key] = file;
  }
  return {
    schema: 'reader_art_v1',
    jobs,
    items,
    portraits,
    abilities,
    jobSprites,
    characterSprites,
    monsterSprites,
  };
}

function url(file: string | undefined): string | null {
  return file ? `${import.meta.env.BASE_URL}reader-art/${file}` : null;
}

export function portraitArt(
  art: ReaderArt | null,
  identity: string,
): string | null {
  return url(art?.portraits[identity]);
}

export function jobArt(
  art: ReaderArt | null,
  id: string,
  sex: 'male' | 'female' | null,
): string | null {
  const pair = art?.jobs[id];
  return url(
    sex === 'male' ? pair?.male : sex === 'female' ? pair?.female : undefined,
  );
}

export function itemArt(art: ReaderArt | null, id: string): string | null {
  return url(art?.items[id]);
}

export function abilityArt(art: ReaderArt | null, id: string): string | null {
  return url(art?.abilities[id]);
}

export function jobSpriteArt(
  art: ReaderArt | null,
  id: string,
  sex: 'male' | 'female' | null,
): string | null {
  const pair = art?.jobSprites[id];
  return url(
    sex === 'male' ? pair?.male : sex === 'female' ? pair?.female : undefined,
  );
}

export function characterSpriteArt(
  art: ReaderArt | null,
  identity: string,
): string | null {
  return url(art?.characterSprites[identity]);
}

export function monsterSpriteArt(
  art: ReaderArt | null,
  jobId: string,
): string | null {
  return url(art?.monsterSprites[jobId]);
}

export function useReaderArt(): ReaderArt | null {
  const [art, setArt] = useState<ReaderArt | null>(null);
  useEffect(() => {
    let active = true;
    void fetch(`${import.meta.env.BASE_URL}reader-art/manifest.json`)
      .then(async (response): Promise<unknown> => {
        if (!response.ok) return null;
        const value: unknown = await response.json();
        return value;
      })
      .then((value: unknown) => {
        if (active) setArt(parseReaderArt(value));
      })
      .catch(() => {
        if (active) setArt(null);
      });
    return () => {
      active = false;
    };
  }, []);
  return art;
}
