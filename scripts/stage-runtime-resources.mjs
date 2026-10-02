// Copy ignored, previously generated resources next to the local release build.
// No game installation, save folder or user profile is written by this script.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';

const root = fs.realpathSync(process.cwd());
const baseTarget = path.join(root, 'target');
const tauriTarget = path.join(root, 'src-tauri', 'target');
const target = process.env.CARGO_TARGET_DIR
  ? path.resolve(root, 'src-tauri', process.env.CARGO_TARGET_DIR)
  : baseTarget;
if (
  target !== baseTarget &&
  !target.startsWith(`${baseTarget}${path.sep}`) &&
  target !== tauriTarget &&
  !target.startsWith(`${tauriTarget}${path.sep}`)
) {
  throw new Error('CARGO_TARGET_DIR must be inside a workspace target');
}
const release = path.join(target, 'release');
const resources = path.join(release, 'resources');
fs.mkdirSync(resources, { recursive: true });
const realRelease = fs.realpathSync(release);
const realResources = fs.realpathSync(resources);
if (
  !realResources.startsWith(`${realRelease}${path.sep}`) ||
  !realRelease.startsWith(`${root}${path.sep}`)
) {
  throw new Error('Release resource directory is outside the workspace.');
}

const inputs = [
  {
    name: 'CompressDict.bin',
    sources: [
      '.local/research-inputs/resources/ticsaveeditor-07ea857/CompressDict.bin',
    ],
  },
  {
    name: 'reader-catalogue-v1.json',
    sources: [
      '.local/reader-catalogue/reader-catalogue-with-growth-v3.json',
      '.local/reader-catalogue/reader-catalogue-with-item-details-v2.json',
      '.local/reader-catalogue/reader-catalogue-with-commands.json',
      '.local/reader-catalogue/reader-catalogue-with-mechanics.json',
      '.local/reader-catalogue/reader-catalogue-v1.json',
      '.local/d032-catalogue/run-1.json',
    ],
  },
  {
    name: 'job-requirements-v1.json',
    sources: ['.local/job-eligibility/job-requirements-v1.json'],
  },
  {
    name: 'ability-flags-v1.json',
    sources: ['.local/ability-flags/ability-flags-v1.json'],
  },
];

function digest(file) {
  return createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

for (const input of inputs) {
  const source = input.sources
    .map((candidate) => path.join(root, candidate))
    .find(
      (candidate) =>
        fs.existsSync(candidate) && fs.lstatSync(candidate).isFile(),
    );
  if (!source) {
    console.log(`${input.name}: no ignored local source available`);
    continue;
  }
  if (fs.statSync(source).size > 32 * 1024 * 1024) {
    throw new Error(`${input.name}: source exceeds the development bound`);
  }
  const destination = path.join(resources, input.name);
  if (
    fs.existsSync(destination) ||
    fs.lstatSync(destination, { throwIfNoEntry: false })
  ) {
    const info = fs.lstatSync(destination);
    if (!info.isFile() && !info.isSymbolicLink()) {
      throw new Error(`${input.name}: release destination is not a file`);
    }
    if (info.isFile() && digest(source) === digest(destination)) {
      console.log(`${input.name}: current`);
      continue;
    }
    fs.unlinkSync(destination);
  }
  fs.copyFileSync(source, destination, fs.constants.COPYFILE_EXCL);
  console.log(`${input.name}: staged for the local release build`);
}
