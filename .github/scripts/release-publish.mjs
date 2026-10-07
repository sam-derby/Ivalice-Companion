// Runs after both platform builds pass.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { verifyVersion } from './release-package.mjs';

export function expectedAssets(version) {
  return [
    `Ivalice.Companion_${version}_x64-setup.exe`,
    `Ivalice.Companion_${version}_x64-portable.zip`,
    `Ivalice.Companion_${version}_amd64.deb`,
    `Ivalice.Companion_${version}_x86_64.AppImage`,
  ];
}

export function verifyAssets(directory, version) {
  const names = expectedAssets(version);
  const expected = new Set([
    ...names,
    'SHA256SUMS-windows.txt',
    'SHA256SUMS-linux.txt',
  ]);
  const combined = path.join(directory, 'SHA256SUMS.txt');
  if (fs.existsSync(combined)) expected.add('SHA256SUMS.txt');
  const actual = fs.readdirSync(directory);
  if (
    actual.length !== expected.size ||
    actual.some((name) => !expected.has(name))
  )
    throw new Error(
      'Release must contain exactly four packages and two checksum manifests',
    );
  const recorded = ['windows', 'linux'].flatMap((platform) =>
    fs
      .readFileSync(path.join(directory, `SHA256SUMS-${platform}.txt`), 'utf8')
      .trim()
      .split('\n'),
  );
  const lines = names.map((name) => {
    const bytes = fs.readFileSync(path.join(directory, name));
    if (bytes.length === 0) throw new Error('Empty release package');
    const line = `${createHash('sha256').update(bytes).digest('hex')}  ${name}`;
    if (recorded.filter((entry) => entry === line).length !== 1)
      throw new Error('Package checksum differs from build job');
    return line;
  });
  if (recorded.length !== names.length)
    throw new Error('Unexpected checksum entries');
  const sums = lines.join('\n') + '\n';
  if (fs.existsSync(combined) && fs.readFileSync(combined, 'utf8') !== sums)
    throw new Error('Invalid combined checksum manifest');
  return sums;
}

export function requireDraft(release) {
  if (release.isDraft !== true)
    throw new Error(
      'Refusing to modify a published release; use a new version and tag',
    );
}

export function draftApiPath(release, repository) {
  requireDraft(release);
  const prefix = `https://api.github.com/repos/${repository}/releases/`;
  if (
    typeof release.apiUrl !== 'string' ||
    !release.apiUrl.startsWith(prefix) ||
    !/^\d+$/.test(release.apiUrl.slice(prefix.length))
  )
    throw new Error('Invalid draft release endpoint');
  return `repos/${repository}/releases/${release.apiUrl.slice(prefix.length)}`;
}

function gh(args) {
  const result = spawnSync('gh', args, { encoding: 'utf8' });
  if (result.error || result.status !== 0)
    throw new Error('GitHub release operation failed');
  return result.stdout;
}

export function publishDraft(root, tag) {
  if (!tag)
    throw new Error('A matching version tag is required to create a draft');
  const version = verifyVersion(root, tag);
  const directory = path.join(root, 'target/release-assets/combined');
  const sums = verifyAssets(directory, version);
  const checksums = path.join(directory, 'SHA256SUMS.txt');
  if (!fs.existsSync(checksums))
    fs.writeFileSync(checksums, sums, { flag: 'wx' });
  const existing = spawnSync(
    'gh',
    ['release', 'view', tag, '--json', 'isDraft'],
    { encoding: 'utf8' },
  );
  if (existing.error) throw existing.error;
  const assets = [
    ...expectedAssets(version).map((name) => path.join(directory, name)),
    checksums,
  ];
  if (existing.status === 0) {
    requireDraft(JSON.parse(existing.stdout));
    gh(['release', 'upload', tag, ...assets, '--clobber']);
  } else {
    // Let creation fail if the earlier lookup missed an existing release.
    gh([
      'release',
      'create',
      tag,
      '--verify-tag',
      '--draft',
      '--title',
      `Ivalice Companion v${version}`,
      '--notes',
      'Windows x64 installer and portable ZIP; Linux x86_64 .deb installer and AppImage portable. Linux / Steam Deck test builds. All downloads include runtime resources and licence notices. Extract the entire Windows ZIP. For Linux AppImage, make the file executable before opening it. Try edits on a copy of your save.',
      ...assets,
    ]);
  }
  // Check GitHub's digests against the files we uploaded.
  const repo = process.env.GH_REPO;
  if (!repo || !/^[\w.-]+\/[\w.-]+$/.test(repo))
    throw new Error('Invalid release repository');
  const draft = JSON.parse(
    gh(['release', 'view', tag, '--json', 'apiUrl,isDraft']),
  );
  const released = JSON.parse(gh(['api', draftApiPath(draft, repo)]));
  if (released.draft !== true) throw new Error('Release is no longer a draft');
  for (const file of assets) {
    const matches = released.assets.filter(
      (asset) => asset.name === path.basename(file),
    );
    const digest =
      'sha256:' +
      createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    if (matches.length !== 1 || matches[0].digest !== digest)
      throw new Error('Uploaded release digest differs');
  }
  console.log(
    `Verified draft release ${tag} with all four packages. Review and publish it on GitHub when ready.`,
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  publishDraft(process.cwd(), process.env.RELEASE_TAG);
}
