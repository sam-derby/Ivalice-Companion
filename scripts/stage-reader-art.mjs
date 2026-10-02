// Stage only validated, ignored local art into Vite's local desktop bundle.
import fs from 'node:fs';
import path from 'node:path';

const root = fs.realpathSync(process.cwd());
const source = path.join(root, '.local', 'reader-art');
const publicRoot = path.join(root, 'public');
fs.mkdirSync(publicRoot, { recursive: true });
const realPublic = fs.realpathSync(publicRoot);
if (!realPublic.startsWith(`${root}${path.sep}`)) {
  throw new Error('art destination outside workspace');
}
const destination = path.join(realPublic, 'reader-art');
if (fs.existsSync(destination)) {
  if (
    fs.lstatSync(destination).isSymbolicLink() ||
    fs.realpathSync(destination) !== destination
  ) {
    throw new Error('art destination is a link');
  }
  fs.rmSync(destination, { recursive: true });
}
fs.mkdirSync(path.join(destination, 'images'), { recursive: true });

let manifest = {
  schema: 'reader_art_v1',
  jobs: {},
  items: {},
  portraits: {},
  abilities: {},
  job_sprites: {},
  character_sprites: {},
  monster_sprites: {},
};
const input = path.join(source, 'manifest.json');
if (fs.existsSync(input)) {
  if (!fs.lstatSync(input).isFile() || fs.statSync(input).size > 256 * 1024) {
    throw new Error('invalid local art manifest');
  }
  manifest = JSON.parse(fs.readFileSync(input, 'utf8'));
}
if (
  manifest.schema !== 'reader_art_v1' ||
  typeof manifest.jobs !== 'object' ||
  typeof manifest.items !== 'object' ||
  typeof manifest.portraits !== 'object' ||
  manifest.jobs === null ||
  manifest.items === null ||
  manifest.portraits === null
) {
  throw new Error('unsupported art manifest');
}
if (manifest.abilities === undefined) manifest.abilities = {};
if (typeof manifest.abilities !== 'object' || manifest.abilities === null) {
  throw new Error('invalid ability art mapping');
}
for (const field of ['job_sprites', 'character_sprites', 'monster_sprites']) {
  if (manifest[field] === undefined) manifest[field] = {};
  if (typeof manifest[field] !== 'object' || manifest[field] === null)
    throw new Error('invalid sprite mapping');
}
const files = new Set();
for (const [key, value] of Object.entries(manifest.jobs)) {
  if (
    !/^job:[0-9]{1,3}$/.test(key) ||
    typeof value !== 'object' ||
    value === null
  ) {
    throw new Error('invalid job art mapping');
  }
  for (const [sex, file] of Object.entries(value)) {
    if (sex !== 'male' && sex !== 'female') throw new Error('invalid art sex');
    files.add(file);
  }
}
for (const [key, file] of Object.entries(manifest.items)) {
  if (!/^item:[0-9]{1,3}$/.test(key))
    throw new Error('invalid item art mapping');
  files.add(file);
}
for (const [key, file] of Object.entries(manifest.portraits)) {
  if (!/^character:[0-9]{1,3}$/.test(key))
    throw new Error('invalid portrait mapping');
  files.add(file);
}
for (const [key, file] of Object.entries(manifest.abilities)) {
  if (!/^ability:[0-9]{1,3}$/.test(key))
    throw new Error('invalid ability art mapping');
  files.add(file);
}
for (const [key, value] of Object.entries(manifest.job_sprites)) {
  if (
    !/^job:[0-9]{1,3}$/.test(key) ||
    typeof value !== 'object' ||
    value === null
  )
    throw new Error('invalid job sprite mapping');
  for (const [sex, file] of Object.entries(value)) {
    if (
      (sex !== 'male' && sex !== 'female') ||
      file !== `images/sprite-job-${key.slice(4)}-${sex}.png`
    )
      throw new Error('invalid job sprite');
    files.add(file);
  }
}
for (const [key, file] of Object.entries(manifest.character_sprites)) {
  if (
    !/^character:[0-9]{1,3}$/.test(key) ||
    file !== `images/sprite-character-${key.slice(10)}.png`
  )
    throw new Error('invalid character sprite');
  files.add(file);
}
for (const [key, file] of Object.entries(manifest.monster_sprites)) {
  if (
    !/^job:[0-9]{1,3}$/.test(key) ||
    !/^images\/sprite-monster-[0-9]{1,3}\.png$/.test(file)
  )
    throw new Error('invalid monster sprite');
  files.add(file);
}
for (const file of files) {
  if (
    typeof file !== 'string' ||
    (!/^images\/(?:job-visual|item|portrait|ability-icon)-[0-9]{1,3}\.png$/.test(
      file,
    ) &&
      !/^images\/sprite-(?:job-[0-9]{1,3}-(?:male|female)|character-[0-9]{1,3}|monster-[0-9]{1,3})\.png$/.test(
        file,
      ))
  ) {
    throw new Error('invalid art path');
  }
  const from = path.join(source, file);
  if (
    !fs.lstatSync(from).isFile() ||
    fs.statSync(from).size > 4 * 1024 * 1024
  ) {
    throw new Error('missing or oversized local art');
  }
  const png = fs.readFileSync(from);
  if (!png.subarray(0, 8).equals(Buffer.from('89504e470d0a1a0a', 'hex'))) {
    throw new Error('invalid local art image');
  }
  fs.copyFileSync(from, path.join(destination, file));
}
fs.writeFileSync(
  path.join(destination, 'manifest.json'),
  JSON.stringify(manifest),
);
console.log(`reader art: staged ${files.size} local images`);
