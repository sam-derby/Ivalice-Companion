// Recover only the artwork already shipped in the pinned public release.
// Tauri Dumper v0.2.2, MIT, source revision ca887d212f5963baad85589baaabd086bb08c3f4:
// https://github.com/Mas0nShi/tauri-dumper/tree/ca887d212f5963baad85589baaabd086bb08c3f4
// This tool and the extracted game artwork remain ignored development inputs.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { readZip } from './zip.mjs';

const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const pngSignature = Buffer.from('89504e470d0a1a0a', 'hex');

export function inspectReaderArt(directory) {
  if (fs.lstatSync(directory).isSymbolicLink())
    throw new Error('Artwork directory is a link');
  const files = [];
  for (const entry of fs.readdirSync(directory, {
    recursive: true,
    withFileTypes: true,
  })) {
    if (entry.isSymbolicLink()) throw new Error('Artwork contains a link');
    if (!entry.isFile()) continue;
    const file = path.join(entry.parentPath, entry.name);
    const relative = path.relative(directory, file).replaceAll('\\', '/');
    const bytes = fs.readFileSync(file);
    if (relative === 'manifest.json') {
      if (
        bytes.length > 256 * 1024 ||
        JSON.parse(bytes.toString('utf8')).schema !== 'reader_art_v1'
      )
        throw new Error('Invalid artwork manifest');
    } else {
      if (
        !/^images\/[a-z0-9-]+\.png$/.test(relative) ||
        bytes.length > 4 * 1024 * 1024 ||
        !bytes.subarray(0, 8).equals(pngSignature)
      )
        throw new Error('Invalid artwork image');
    }
    files.push({ name: relative, sha: digest(bytes) });
  }
  if (!files.some((file) => file.name === 'manifest.json'))
    throw new Error('Missing artwork manifest');
  files.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  return {
    files: files.length,
    treeSha256: digest(
      files.map((file) => `${file.sha}  ${file.name}\n`).join(''),
    ),
  };
}

export function verifyReaderArt(directory, expected) {
  const actual = inspectReaderArt(directory);
  if (
    actual.files !== expected.files ||
    actual.treeSha256 !== expected.treeSha256
  )
    throw new Error('Reader artwork differs from the published build');
  return actual;
}

function run(command, args, options) {
  const result = spawnSync(command, args, options);
  if (result.error || result.status !== 0)
    throw new Error(
      `Artwork tool ${path.basename(command)} failed (${result.status ?? result.error?.code ?? 'unknown'})`,
    );
}

export async function stageReaderArt(root) {
  const source = JSON.parse(
    fs.readFileSync(path.join(root, '.github/reader-art-source.json'), 'utf8'),
  );
  const publicRoot = path.join(root, 'public');
  const destination = path.join(publicRoot, 'reader-art');
  if (fs.existsSync(destination)) {
    verifyReaderArt(destination, source);
    return;
  }
  const local = path.join(root, '.local');
  fs.mkdirSync(local, { recursive: true });
  if (fs.lstatSync(local).isSymbolicLink())
    throw new Error('Build inputs directory is a link');
  const scratch = fs.mkdtempSync(path.join(local, 'reader-art-input-'));
  const response = await fetch(source.url);
  if (!response.ok) throw new Error('Published artwork source download failed');
  const archive = Buffer.from(await response.arrayBuffer());
  if (archive.length > 100 * 1024 * 1024 || digest(archive) !== source.sha256)
    throw new Error('Published artwork source checksum mismatch');
  const zip = path.join(scratch, 'source.zip');
  fs.writeFileSync(zip, archive, { flag: 'wx' });
  const exe = readZip(zip).get(source.executable);
  if (!exe) throw new Error('Published artwork executable missing');
  const executable = path.join(scratch, 'source.exe');
  fs.writeFileSync(executable, exe, { flag: 'wx' });
  const toolRoot = path.join(local, 'tauri-dumper');
  const tool = path.join(
    toolRoot,
    'bin',
    process.platform === 'win32' ? 'tauri-dumper.exe' : 'tauri-dumper',
  );
  // Compile on the host's baseline, avoiding a newer prebuilt tool's glibc floor.
  run(
    'cargo',
    [
      'install',
      '--git',
      'https://github.com/Mas0nShi/tauri-dumper',
      '--rev',
      source.extractorRevision,
      '--locked',
      '--root',
      toolRoot,
      '--target-dir',
      path.join(root, 'target/tauri-dumper'),
      'tauri-dumper',
    ],
    { cwd: root, stdio: 'pipe', encoding: 'utf8' },
  );
  const extracted = path.join(scratch, 'extracted');
  run(tool, ['extract', executable, '-o', extracted, '--quiet'], {
    cwd: root,
    stdio: 'pipe',
    encoding: 'utf8',
  });
  const art = path.join(extracted, 'reader-art');
  verifyReaderArt(art, source);
  fs.mkdirSync(publicRoot, { recursive: true });
  if (fs.lstatSync(publicRoot).isSymbolicLink())
    throw new Error('Public directory is a link');
  fs.cpSync(art, destination, {
    recursive: true,
    errorOnExist: true,
    force: false,
  });
  verifyReaderArt(destination, source);
  console.log(`Verified published reader artwork: ${source.files - 1} images`);
}
