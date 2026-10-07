import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { stageRustNotices, appendNativeNotices } from './linux-notices.mjs';
import { stageReaderArt } from './reader-art-input.mjs';

// Pass bundle configs explicitly: cargo check runs before notices are generated.
const configs = {
  win32: 'src-tauri/tauri.installer.conf.json',
  linux: 'src-tauri/tauri.linux.installer.conf.json',
};
const config = configs[process.platform];
if (!config) throw new Error('Desktop builds support Windows and Linux only');
await stageReaderArt(process.cwd());
function tauri(args) {
  const result = spawnSync(
    process.execPath,
    [path.resolve('node_modules/@tauri-apps/cli/tauri.js'), ...args],
    { stdio: 'inherit' },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

if (process.platform === 'linux') {
  const notice = await stageRustNotices(process.cwd());
  tauri(['build', '--no-bundle', '--config', config, '--', '--locked']);
  appendNativeNotices(process.cwd(), notice);
  tauri(['bundle', '--config', config]);
} else {
  tauri(['build', '--config', config, '--', '--locked']);
}
