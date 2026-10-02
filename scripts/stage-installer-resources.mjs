// Stage only the four required, ignored runtime files for a tester installer.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const sources = [
  [
    'CompressDict.bin',
    ['.local/research-inputs/resources/ticsaveeditor-07ea857/CompressDict.bin'],
  ],
  [
    'reader-catalogue-v1.json',
    [
      '.local/reader-catalogue/reader-catalogue-with-growth-v3.json',
      '.local/reader-catalogue/reader-catalogue-with-item-details-v2.json',
      '.local/reader-catalogue/reader-catalogue-with-commands.json',
      '.local/reader-catalogue/reader-catalogue-with-mechanics.json',
      '.local/reader-catalogue/reader-catalogue-v1.json',
      '.local/d032-catalogue/run-1.json',
    ],
  ],
  [
    'job-requirements-v1.json',
    ['.local/job-eligibility/job-requirements-v1.json'],
  ],
  ['ability-flags-v1.json', ['.local/ability-flags/ability-flags-v1.json']],
];

export function stageInstallerResources(workspace) {
  const root = fs.realpathSync(workspace);
  const local = fs.realpathSync(path.join(root, '.local'));
  if (!local.startsWith(`${root}${path.sep}`)) {
    throw new Error('local resource directory is outside the workspace');
  }
  const destination = path.join(root, 'src-tauri', 'installer-resources');
  const selected = sources.map(([name, candidates]) => {
    const source = candidates
      .map((candidate) => path.join(root, candidate))
      .find((candidate) => fs.existsSync(candidate));
    if (!source) throw new Error(`${name}: required local resource is missing`);
    const info = fs.lstatSync(source);
    if (
      !info.isFile() ||
      info.size === 0 ||
      info.size > 32 * 1024 * 1024 ||
      !fs.realpathSync(source).startsWith(`${local}${path.sep}`)
    ) {
      throw new Error(`${name}: invalid local resource`);
    }
    return [name, source];
  });

  if (fs.existsSync(destination)) {
    const info = fs.lstatSync(destination);
    if (
      !info.isDirectory() ||
      info.isSymbolicLink() ||
      fs.realpathSync(destination) !== destination
    ) {
      throw new Error(
        'installer resource destination is not a plain directory',
      );
    }
    const entries = fs.readdirSync(destination);
    for (const entry of entries) {
      if (!sources.some(([name]) => name === entry)) {
        throw new Error(`unexpected installer resource: ${entry}`);
      }
      const file = path.join(destination, entry);
      if (!fs.lstatSync(file).isFile())
        throw new Error(`invalid installer resource: ${entry}`);
    }
    for (const entry of entries) {
      const file = path.join(destination, entry);
      fs.unlinkSync(file);
    }
  } else {
    fs.mkdirSync(destination);
  }

  for (const [name, source] of selected) {
    fs.copyFileSync(
      source,
      path.join(destination, name),
      fs.constants.COPYFILE_EXCL,
    );
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  stageInstallerResources(process.cwd());
  console.log('installer resources: staged four required local files');
}
