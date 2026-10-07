// Development-only notice staging. The installed app never downloads anything.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const licenceName = /^(licen[sc]e|copying|copyright|notice)([.-].*)?$/i;

export function runtimePackages(metadata, tree) {
  const selected = new Set(
    tree
      .split('\n')
      .map((line) =>
        line
          .match(/^(\S+) v([^\s]+)/)
          ?.slice(1)
          .join(' '),
      )
      .filter(Boolean),
  );
  return metadata.packages
    .filter((pkg) => pkg.source && selected.has(`${pkg.name} ${pkg.version}`))
    .sort((a, b) =>
      `${a.name} ${a.version}`.localeCompare(`${b.name} ${b.version}`, 'en'),
    );
}

async function licenceTexts(pkg) {
  const directory = path.dirname(pkg.manifest_path);
  const names = fs
    .readdirSync(directory)
    .filter(
      (name) =>
        licenceName.test(name) &&
        fs.statSync(path.join(directory, name)).isFile(),
    );
  if (names.length)
    return names
      .sort()
      .map(
        (name) =>
          `${name}\n\n${fs.readFileSync(path.join(directory, name), 'utf8')}`,
      )
      .join('\n\n');
  // Some workspace crates omit the shared licence from their registry archive.
  // Read it at the exact source revision recorded in that archive, never HEAD.
  const vcs = JSON.parse(
    fs.readFileSync(path.join(directory, '.cargo_vcs_info.json'), 'utf8'),
  );
  const repository = pkg.repository
    ?.match(/^https:\/\/github.com\/([\w.-]+\/[\w.-]+)(?:\.git)?\/?$/)?.[1]
    ?.replace(/\.git$/, '');
  const revision = vcs.git?.sha1;
  if (!repository || !/^[a-f0-9]{40}$/.test(revision))
    throw new Error(`Cannot resolve pinned licence: ${pkg.name}`);
  const texts = [];
  for (const name of [
    'LICENSE',
    'LICENSE-MIT',
    'LICENSE-APACHE',
    'LICENSE.txt',
  ]) {
    const url = `https://raw.githubusercontent.com/${repository}/${revision}/${name}`;
    const response = await fetch(url);
    if (response.ok)
      texts.push(`${name}\nSource: ${url}\n\n${await response.text()}`);
    else if (response.status !== 404)
      throw new Error(`Cannot download pinned licence: ${pkg.name}`);
  }
  if (!texts.length)
    throw new Error(`Missing upstream licence text: ${pkg.name}`);
  return texts.join('\n\n');
}

export async function stageRustNotices(root) {
  const options = { cwd: root, encoding: 'utf8', maxBuffer: 30 * 1024 * 1024 };
  const metadata = JSON.parse(
    execFileSync(
      'cargo',
      [
        'metadata',
        '--locked',
        '--filter-platform',
        'x86_64-unknown-linux-gnu',
        '--format-version',
        '1',
      ],
      options,
    ),
  );
  const tree = execFileSync(
    'cargo',
    [
      'tree',
      '-p',
      'ivalice-companion',
      '--locked',
      '--target',
      'x86_64-unknown-linux-gnu',
      '--edges',
      'normal',
      '--prefix',
      'none',
    ],
    options,
  );
  const baseline = fs.readFileSync(
    path.join(root, 'LICENSES/dependencies.txt'),
    'utf8',
  );
  const additions = [];
  for (const pkg of runtimePackages(metadata, tree)) {
    if (baseline.includes(`${pkg.name} ${pkg.version} (Rust)`)) continue;
    if (!pkg.license) throw new Error(`Missing declared licence: ${pkg.name}`);
    additions.push(
      `${pkg.name} ${pkg.version} (Rust)\nDeclared licence: ${pkg.license}\nSource: https://crates.io/api/v1/crates/${pkg.name}/${pkg.version}/download\n\n${await licenceTexts(pkg)}`,
    );
  }
  const destination = path.join(
    root,
    'src-tauri/installer-resources/linux-dependency-notices.txt',
  );
  fs.writeFileSync(
    destination,
    baseline +
      '\n\nLinux runtime dependency supplement\n\n' +
      additions.join('\n\n--------------------\n\n') +
      '\n',
  );
  return destination;
}

export function appendNativeNotices(root, destination) {
  const binary = path.join(root, 'target/release/ivalice-companion');
  const options = { encoding: 'utf8', maxBuffer: 20 * 1024 * 1024 };
  const linked = execFileSync('ldd', [binary], options);
  if (linked.includes('not found'))
    throw new Error('Missing native runtime library');
  const libraries = [...linked.matchAll(/(?:=>\s+)?(\/[^\s]+)\s+\(/g)].map(
    (match) => match[1],
  );
  const packages = new Set();
  for (const library of libraries) {
    const variants = [...new Set([library, fs.realpathSync(library)])];
    let owners;
    for (const variant of variants) {
      try {
        owners = execFileSync('dpkg-query', ['-S', variant], {
          ...options,
          stdio: ['ignore', 'pipe', 'ignore'],
        });
        break;
      } catch {
        /* Try the resolved library name. */
      }
    }
    if (!owners) throw new Error('Cannot identify native library package');
    for (const line of owners.trim().split('\n'))
      packages.add(line.slice(0, line.indexOf(': ')));
  }
  const notices = [];
  for (const pkg of [...packages].sort()) {
    const version = execFileSync(
      'dpkg-query',
      ['-W', '-f=${Version}', pkg],
      options,
    ).trim();
    const copyright = path.join(
      '/usr/share/doc',
      pkg.split(':')[0],
      'copyright',
    );
    if (!fs.existsSync(copyright))
      throw new Error(`Missing native package notice: ${pkg}`);
    notices.push(
      `${pkg} ${version} (distribution library)\nSource packages: https://packages.ubuntu.com/jammy/${pkg.split(':')[0]}\n\n${fs.readFileSync(copyright, 'utf8')}`,
    );
  }
  // Debian copyright files may refer to these shared verbatim terms.
  for (const name of fs.readdirSync('/usr/share/common-licenses').sort()) {
    const file = path.join('/usr/share/common-licenses', name);
    if (fs.statSync(file).isFile())
      notices.push(`${name}\n\n${fs.readFileSync(file, 'utf8')}`);
  }
  fs.appendFileSync(
    destination,
    '\nNative Linux dependency notices\n\n' +
      notices.join('\n\n--------------------\n\n') +
      '\n',
  );
}
