import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import {
  verifyVersion,
  writeZip,
  readZip,
  unmarkWindowsExecutable,
  verifyBundled,
  writeChecksums,
} from './release-package.mjs';
import {
  expectedAssets,
  verifyAssets,
  requireDraft,
} from './release-publish.mjs';
import { runtimePackages } from './linux-notices.mjs';
import { inspectReaderArt, verifyReaderArt } from './reader-art-input.mjs';
import { latestBuild, requirePackages } from './release-source.mjs';

test('release reuses only the latest successful main build for the exact commit', () => {
  const success = {
    id: 1,
    head_sha: 'source',
    event: 'push',
    head_branch: 'main',
    status: 'completed',
    conclusion: 'success',
    created_at: '2026-10-01',
  };
  assert.equal(latestBuild([success], 'source'), 1);
  assert.equal(
    latestBuild([{ ...success, event: 'workflow_dispatch' }], 'source'),
    1,
  );
  assert.equal(
    latestBuild([{ ...success, head_branch: 'other' }], 'source'),
    null,
  );
  assert.equal(latestBuild([success], 'other'), null);
  assert.equal(
    latestBuild([{ ...success, event: 'pull_request' }], 'source'),
    null,
  );
  assert.equal(
    latestBuild([{ ...success, status: 'in_progress' }], 'source'),
    null,
  );
  assert.throws(
    () =>
      latestBuild(
        [
          success,
          {
            ...success,
            id: 2,
            created_at: '2026-10-02',
            conclusion: 'failure',
          },
        ],
        'source',
      ),
    /failed/,
  );
});

test('release rejects expired, duplicate or mismatched package artifacts', () => {
  const artifacts = ['packages-windows', 'packages-linux'].map((name) => ({
    name,
    expired: false,
    workflow_run: { head_sha: 'source' },
  }));
  requirePackages(artifacts, 'source');
  assert.throws(() => requirePackages(artifacts.slice(1), 'source'), /missing/);
  assert.throws(
    () => requirePackages([...artifacts, artifacts[0]], 'source'),
    /missing/,
  );
  assert.throws(() => requirePackages(artifacts, 'other'), /missing/);
  assert.throws(
    () =>
      requirePackages(
        artifacts.map((a) => ({ ...a, expired: true })),
        'source',
      ),
    /expired/,
  );
});

test('reader artwork integrity rejects missing or changed images', () =>
  temporary((root) => {
    write(root, 'manifest.json', '{"schema":"reader_art_v1"}');
    write(root, 'images/item-1.png', Buffer.from('89504e470d0a1a0a00', 'hex'));
    const expected = inspectReaderArt(root);
    assert.equal(expected.files, 2);
    verifyReaderArt(root, expected);
    write(root, 'images/item-1.png', Buffer.from('89504e470d0a1a0a01', 'hex'));
    assert.throws(() => verifyReaderArt(root, expected), /differs/);
    fs.unlinkSync(path.join(root, 'images/item-1.png'));
    assert.throws(() => verifyReaderArt(root, expected), /differs/);
  }));

test('Linux notices include the selected runtime tree and exclude unused optional crates', () => {
  const metadata = {
    packages: [
      { name: 'gtk', version: '0.18.2', source: 'registry' },
      { name: 'unused', version: '1.0.0', source: 'registry' },
      { name: 'ivalice-domain', version: '0.1.0', source: null },
    ],
  };
  assert.deepEqual(
    runtimePackages(
      metadata,
      'gtk v0.18.2\nivalice-domain v0.1.0 (local)\ngtk v0.18.2 (*)',
    ).map((pkg) => pkg.name),
    ['gtk'],
  );
});

function temporary(run) {
  const root = fs.mkdtempSync(
    path.join(os.tmpdir(), 'ivalice-release-controls-'),
  );
  try {
    run(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function write(root, file, bytes) {
  const target = path.join(root, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, bytes);
}

test('release version must agree with the tag and all application manifests', () =>
  temporary((root) => {
    write(root, 'package.json', '{"version":"0.1.5"}');
    write(root, 'src-tauri/tauri.conf.json', '{"version":"0.1.5"}');
    write(root, 'src-tauri/Cargo.toml', 'version = "0.1.5"');
    write(root, 'Cargo.lock', 'name = "ivalice-companion"\nversion = "0.1.5"');
    assert.equal(verifyVersion(root, 'v0.1.5'), '0.1.5');
    assert.throws(() => verifyVersion(root, 'v0.1.6'), /tag/);
    write(root, 'src-tauri/Cargo.toml', 'version = "0.1.6"');
    assert.throws(() => verifyVersion(root), /disagree/);
  }));

test('portable ZIP reads back exact executable and resource bytes and rejects corruption', () =>
  temporary((root) => {
    const file = path.join(root, 'portable.zip');
    const entries = [
      ['App/app.exe', Buffer.from('binary')],
      ['App/resources/catalogue.json', Buffer.from('{}')],
    ];
    writeZip(file, entries, new Date(2026, 0, 1));
    assert.deepEqual([...readZip(file)], entries);
    const bytes = fs.readFileSync(file);
    bytes[14] ^= 1;
    // Damage the payload so the CRC check has something to catch.
    bytes[30 + Buffer.byteLength(entries[0][0])] ^= 0xff;
    fs.writeFileSync(file, bytes);
    assert.throws(() => readZip(file));
  }));

test('Windows bundle marker normalization preserves other executable bytes', () => {
  assert.equal(
    unmarkWindowsExecutable(
      Buffer.from('prefixBUNDLE_TYPE_VAR_NSSsuffix'),
    ).toString(),
    'prefixBUNDLE_TYPE_VAR_UNKsuffix',
  );
  assert.deepEqual(
    unmarkWindowsExecutable(Buffer.from([0, 255, 128])),
    Buffer.from([0, 255, 128]),
  );
});

test('package checks require eight resources and five notices with exact bytes', () =>
  temporary((root) => {
    const resources = {};
    for (const [folder, count] of [
      ['resources', 8],
      ['licenses', 5],
    ]) {
      for (let index = 0; index < count; index++) {
        const target = `${folder}/file-${index}`;
        const source = `source-${folder}-${index}`;
        resources[source] = target;
        write(root, 'src-tauri/' + source, target);
        write(root, 'extracted/usr/lib/app/' + target, target);
      }
    }
    write(
      root,
      'src-tauri/tauri.linux.installer.conf.json',
      JSON.stringify({ bundle: { resources } }),
    );
    verifyBundled(
      root,
      path.join(root, 'extracted'),
      'tauri.linux.installer.conf.json',
      true,
    );
    write(root, 'extracted/usr/lib/app/resources/file-0', 'wrong');
    assert.throws(
      () =>
        verifyBundled(
          root,
          path.join(root, 'extracted'),
          'tauri.linux.installer.conf.json',
          true,
        ),
      /bytes differ/,
    );
  }));

test('release requires all four packages and their original build checksums', () =>
  temporary((root) => {
    const names = expectedAssets('0.1.5');
    for (const name of names) write(root, name, Buffer.from(name));
    writeChecksums(root, names.slice(0, 2), 'windows');
    writeChecksums(root, names.slice(2), 'linux');
    assert.equal(verifyAssets(root, '0.1.5').trim().split('\n').length, 4);
    const combined = verifyAssets(root, '0.1.5');
    write(root, 'SHA256SUMS.txt', combined);
    assert.equal(verifyAssets(root, '0.1.5'), combined);
    write(root, 'SHA256SUMS.txt', 'wrong');
    assert.throws(() => verifyAssets(root, '0.1.5'), /combined/);
    fs.unlinkSync(path.join(root, 'SHA256SUMS.txt'));
    write(root, names[2], 'modified');
    assert.throws(() => verifyAssets(root, '0.1.5'), /checksum/);
    fs.unlinkSync(path.join(root, names[3]));
    assert.throws(() => verifyAssets(root, '0.1.5'), /exactly four/);
  }));

test('published releases cannot be overwritten', () => {
  requireDraft({ isDraft: true });
  assert.throws(() => requireDraft({ isDraft: false }), /published/);
  assert.throws(() => requireDraft({}), /published/);
});
