// Package only files from the curated checkout. No private workspace inputs.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import zlib from 'node:zlib';

const PRODUCT = 'Ivalice Companion';
const EXE = 'ivalice-companion.exe';
const PUBLIC_REPOSITORY = 'https://github.com/sam-derby/Ivalice-Companion';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));

function dosDateTime(date) {
  const time =
    (date.getHours() << 11) |
    (date.getMinutes() << 5) |
    Math.floor(date.getSeconds() / 2);
  const day =
    ((date.getFullYear() - 1980) << 9) |
    ((date.getMonth() + 1) << 5) |
    date.getDate();
  return { time, day };
}

/** Writes a deflated ZIP with forward-slash names; entries are [name, bytes]. */
export function writeZip(file, entries, date = new Date()) {
  const { time, day } = dosDateTime(date);
  const locals = [];
  const central = [];
  let offset = 0;
  for (const [name, bytes] of entries) {
    if (name.includes('\\')) throw new Error(`${name}: use forward slashes`);
    const nameBytes = Buffer.from(name, 'utf8');
    const data = zlib.deflateRawSync(bytes, { level: 9 });
    const crc = zlib.crc32(bytes);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(0x0800, 6);
    header.writeUInt16LE(8, 8);
    header.writeUInt16LE(time, 10);
    header.writeUInt16LE(day, 12);
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(data.length, 18);
    header.writeUInt32LE(bytes.length, 22);
    header.writeUInt16LE(nameBytes.length, 26);
    const record = Buffer.alloc(46);
    record.writeUInt32LE(0x02014b50, 0);
    record.writeUInt16LE(20, 4);
    record.writeUInt16LE(20, 6);
    record.writeUInt16LE(0x0800, 8);
    record.writeUInt16LE(8, 10);
    record.writeUInt16LE(time, 12);
    record.writeUInt16LE(day, 14);
    record.writeUInt32LE(crc, 16);
    record.writeUInt32LE(data.length, 20);
    record.writeUInt32LE(bytes.length, 24);
    record.writeUInt16LE(nameBytes.length, 28);
    record.writeUInt32LE(offset, 42);
    locals.push(header, nameBytes, data);
    central.push(record, nameBytes);
    offset += header.length + nameBytes.length + data.length;
  }
  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  fs.writeFileSync(file, Buffer.concat([...locals, directory, end]), {
    flag: 'wx',
  });
}

/** Reads every entry of a ZIP written by writeZip, checking CRCs and sizes. */
export function readZip(file) {
  const zip = fs.readFileSync(file);
  const end = zip.length - 22;
  if (zip.readUInt32LE(end) !== 0x06054b50) throw new Error('ZIP end missing');
  const count = zip.readUInt16LE(end + 10);
  let at = zip.readUInt32LE(end + 16);
  const entries = new Map();
  for (let index = 0; index < count; index += 1) {
    if (zip.readUInt32LE(at) !== 0x02014b50) throw new Error('ZIP directory');
    const crc = zip.readUInt32LE(at + 16);
    const size = zip.readUInt32LE(at + 24);
    const nameLength = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30) + zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const name = zip.toString('utf8', at + 46, at + 46 + nameLength);
    const start =
      local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
    const compressed = zip.subarray(start, start + zip.readUInt32LE(at + 20));
    const bytes = zlib.inflateRawSync(compressed);
    if (bytes.length !== size || zlib.crc32(bytes) !== crc) {
      throw new Error(`${name}: ZIP entry is corrupt`);
    }
    entries.set(name, bytes);
    at += 46 + nameLength + extra;
  }
  return entries;
}

/** The portable package's instructions for one version. */
export function portableText(version) {
  return [
    `${PRODUCT} ${version} - portable Windows x64 package`,
    '',
    `Extract the entire ZIP, then run ${EXE}. Keep the resources and`,
    'licenses folders beside the executable. Windows 10/11 and the Microsoft',
    'WebView2 runtime are required. This ZIP does not install WebView2.',
    '',
    "Settings are stored in the current user's Windows LocalAppData folder under",
    'local.ivalice.companion; moving this folder does not move those settings.',
    'Save files stay where you select them. The editor and the Utilities tab can',
    'replace the selected save after review and first create a recoverable backup;',
    'try changes on a copy first.',
    '',
    `Application version: ${version}`,
    `Build source: ${PUBLIC_REPOSITORY}/tree/v${version}`,
    'Licence and third-party notices: licenses/NOTICE.md and companion files.',
    '',
  ].join('\r\n');
}

export function verifyVersion(root, tag = '') {
  const version = readJson(path.join(root, 'package.json')).version;
  if (!/^\d+\.\d+\.\d+$/.test(version))
    throw new Error('Invalid application version');
  const tauri = readJson(path.join(root, 'src-tauri/tauri.conf.json')).version;
  const cargo = fs
    .readFileSync(path.join(root, 'src-tauri/Cargo.toml'), 'utf8')
    .match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const locked = fs
    .readFileSync(path.join(root, 'Cargo.lock'), 'utf8')
    .match(/name = "ivalice-companion"\r?\nversion = "([^"]+)"/)?.[1];
  if (tauri !== version || cargo !== version || locked !== version)
    throw new Error('Application versions disagree');
  if (tag && tag !== `v${version}`)
    throw new Error('Release tag does not match application version');
  return version;
}

function run(command, args, cwd) {
  const result = spawnSync(command, args, { cwd, encoding: 'utf8' });
  if (result.error || result.status !== 0)
    throw new Error(`Package tool failed: ${command}`);
}

function filesUnder(root) {
  return fs
    .readdirSync(root, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) =>
      path
        .relative(root, path.join(entry.parentPath, entry.name))
        .replaceAll('\\', '/'),
    );
}

export function bundledFiles(root, config) {
  const resources = readJson(path.join(root, 'src-tauri', config)).bundle
    .resources;
  if (
    Object.values(resources).filter((file) => file.startsWith('resources/'))
      .length !== 8 ||
    Object.values(resources).filter((file) => file.startsWith('licenses/'))
      .length !== 5
  ) {
    throw new Error(
      'Expected eight runtime resources and five licence notices',
    );
  }
  return Object.entries(resources).map(([source, target]) => {
    if (!/^(resources|licenses)\/[^/]+$/.test(target))
      throw new Error('Invalid bundled destination');
    return { source: path.resolve(root, 'src-tauri', source), target };
  });
}

export function verifyBundled(root, extracted, config, nested = false) {
  const files = filesUnder(extracted);
  for (const { source, target } of bundledFiles(root, config)) {
    const matches = files.filter((file) =>
      nested ? file.endsWith('/' + target) : file === target,
    );
    if (
      matches.length !== 1 ||
      !fs
        .readFileSync(source)
        .equals(fs.readFileSync(path.join(extracted, matches[0])))
    ) {
      throw new Error(`Bundled bytes differ: ${target}`);
    }
  }
}

export function unmarkWindowsExecutable(bytes) {
  return Buffer.from(
    bytes
      .toString('latin1')
      .replace('BUNDLE_TYPE_VAR_NSS', 'BUNDLE_TYPE_VAR_UNK'),
    'latin1',
  );
}

export function packageWindows(
  root,
  assets,
  sevenZip = process.env.SEVEN_ZIP || '7z',
) {
  const version = verifyVersion(root, process.env.RELEASE_TAG);
  const setupName = `Ivalice.Companion_${version}_x64-setup.exe`;
  const setup = path.join(assets, setupName);
  fs.copyFileSync(
    path.join(
      root,
      'target/release/bundle/nsis',
      `${PRODUCT}_${version}_x64-setup.exe`,
    ),
    setup,
    fs.constants.COPYFILE_EXCL,
  );
  const extracted = path.join(assets, 'installer-check');
  run(sevenZip, ['x', setup, `-o${extracted}`, '-y'], root);
  verifyBundled(root, extracted, 'tauri.installer.conf.json');
  const exe = fs.readFileSync(path.join(extracted, EXE));
  if (
    !unmarkWindowsExecutable(exe).equals(
      fs.readFileSync(path.join(root, 'target/release', EXE)),
    )
  )
    throw new Error('Installer executable differs from build');
  const expected = new Set([
    EXE,
    'uninstall.exe',
    ...bundledFiles(root, 'tauri.installer.conf.json').map(
      (entry) => entry.target,
    ),
  ]);
  if (
    filesUnder(extracted).some(
      (file) => !expected.has(file) && !file.startsWith('$PLUGINSDIR/'),
    )
  )
    throw new Error('Unexpected installed file');
  const folder = `${PRODUCT} ${version}`;
  const entries = [
    [`${folder}/${EXE}`, exe],
    [`${folder}/PORTABLE.txt`, Buffer.from(portableText(version))],
    ...bundledFiles(root, 'tauri.installer.conf.json').map(({ target }) => [
      `${folder}/${target}`,
      fs.readFileSync(path.join(extracted, target)),
    ]),
  ];
  const portableName = `Ivalice.Companion_${version}_x64-portable.zip`;
  const portable = path.join(assets, portableName);
  writeZip(portable, entries, new Date(2026, 0, 1));
  const contents = readZip(portable);
  if (
    contents.size !== entries.length ||
    entries.some(([name, bytes]) => !bytes.equals(contents.get(name)))
  )
    throw new Error('Portable verification failed');
  fs.rmSync(extracted, { recursive: true });
  return [setupName, portableName];
}

function oneBundle(root, kind, suffix) {
  const directory = path.join(root, 'target/release/bundle', kind);
  const files = fs
    .readdirSync(directory)
    .filter((file) => file.endsWith(suffix));
  if (files.length !== 1)
    throw new Error(
      `Expected exactly one ${kind} bundle; clean stale bundles first`,
    );
  return path.join(directory, files[0]);
}

export function packageLinux(root, assets) {
  const version = verifyVersion(root, process.env.RELEASE_TAG);
  const debName = `Ivalice.Companion_${version}_amd64.deb`;
  const imageName = `Ivalice.Companion_${version}_x86_64.AppImage`;
  const deb = path.join(assets, debName);
  const image = path.join(assets, imageName);
  fs.copyFileSync(
    oneBundle(root, 'deb', '.deb'),
    deb,
    fs.constants.COPYFILE_EXCL,
  );
  fs.copyFileSync(
    oneBundle(root, 'appimage', '.AppImage'),
    image,
    fs.constants.COPYFILE_EXCL,
  );
  fs.chmodSync(image, 0o755);
  const control = spawnSync(
    'dpkg-deb',
    ['-f', deb, 'Version', 'Architecture'],
    { encoding: 'utf8' },
  );
  if (
    control.status !== 0 ||
    !control.stdout.includes(`Version: ${version}`) ||
    !control.stdout.includes('Architecture: amd64')
  )
    throw new Error('Debian package metadata differs');
  const extracted = path.join(assets, 'deb-check');
  run('dpkg-deb', ['-x', deb, extracted], root);
  verifyBundled(root, extracted, 'tauri.linux.installer.conf.json', true);
  const appDir = path.join(assets, 'image-check');
  fs.mkdirSync(appDir);
  run(image, ['--appimage-extract'], appDir);
  verifyBundled(
    root,
    path.join(appDir, 'squashfs-root'),
    'tauri.linux.installer.conf.json',
    true,
  );
  fs.rmSync(extracted, { recursive: true });
  fs.rmSync(appDir, { recursive: true });
  return [debName, imageName];
}

export function writeChecksums(assets, files, platform) {
  const lines = files.map(
    (name) => `${hash(fs.readFileSync(path.join(assets, name)))}  ${name}`,
  );
  fs.writeFileSync(
    path.join(assets, `SHA256SUMS-${platform}.txt`),
    lines.join('\n') + '\n',
    { flag: 'wx' },
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  const root = process.cwd();
  const platform = process.argv[2];
  if (!['windows', 'linux'].includes(platform))
    throw new Error('Choose windows or linux');
  const assets = path.join(root, 'target/release-assets', platform);
  fs.mkdirSync(assets, { recursive: true });
  const files =
    platform === 'windows'
      ? packageWindows(root, assets)
      : packageLinux(root, assets);
  writeChecksums(assets, files, platform);
  console.log(
    `Verified ${platform} installer and portable: ${files.join(', ')}`,
  );
}
