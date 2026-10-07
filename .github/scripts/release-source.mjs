import fs from 'node:fs';
import { pathToFileURL } from 'node:url';
import path from 'node:path';

export function latestBuild(runs, sha) {
  const matches = runs
    .filter(
      (run) =>
        run.head_sha === sha &&
        ['push', 'workflow_dispatch'].includes(run.event) &&
        run.head_branch === 'main',
    )
    .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
  const run = matches[0];
  if (!run || run.status !== 'completed') return null;
  if (run.conclusion !== 'success')
    throw new Error('Main validation failed for this commit');
  return run.id;
}

export function requirePackages(artifacts, sha) {
  for (const name of ['packages-windows', 'packages-linux']) {
    const matches = artifacts.filter((artifact) => artifact.name === name);
    if (
      matches.length !== 1 ||
      matches[0].expired ||
      matches[0].workflow_run?.head_sha !== sha
    )
      throw new Error(
        'Verified packages are missing, expired or from another commit',
      );
  }
}

async function findBuild() {
  const sha = process.env.SOURCE_SHA;
  const repository = process.env.GITHUB_REPOSITORY;
  if (!/^[a-f0-9]{40}$/.test(sha ?? '') || !repository)
    throw new Error('Missing release source');
  const request = async (endpoint) => {
    const response = await fetch(
      `https://api.github.com/repos/${repository}/actions/${endpoint}`,
      {
        headers: {
          Authorization: `Bearer ${process.env.GH_TOKEN}`,
          Accept: 'application/vnd.github+json',
        },
      },
    );
    if (!response.ok) throw new Error('Cannot verify the main build');
    return response.json();
  };
  for (let attempt = 0; attempt < 120; attempt++) {
    const runs = await request(
      `workflows/validate.yml/runs?head_sha=${sha}&per_page=100`,
    );
    const id = latestBuild(runs.workflow_runs, sha);
    if (id !== null) {
      const artifacts = await request(`runs/${id}/artifacts?per_page=100`);
      requirePackages(artifacts.artifacts, sha);
      fs.appendFileSync(process.env.GITHUB_OUTPUT, `run_id=${id}\n`);
      console.log(`Reusing verified packages from Actions run ${id}`);
      return;
    }
    if (attempt % 4 === 0) console.log('Waiting for the matching main build');
    await new Promise((resolve) => setTimeout(resolve, 15_000));
  }
  throw new Error('No successful main build for this commit');
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
)
  await findBuild();
