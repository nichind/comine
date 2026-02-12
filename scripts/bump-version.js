#!/usr/bin/env node

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(version)) {
  console.error('Usage: pnpm version:set <version>');
  process.exit(1);
}

const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'));
pkg.version = version;
fs.writeFileSync(path.join(root, 'package.json'), JSON.stringify(pkg, null, 2) + '\n');

const cargoPath = path.join(root, 'src-tauri', 'Cargo.toml');
fs.writeFileSync(
  cargoPath,
  fs.readFileSync(cargoPath, 'utf8').replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`)
);

const tauriPath = path.join(root, 'src-tauri', 'tauri.conf.json');
const tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));
tauri.version = version;
fs.writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + '\n');

const gradlePath = path.join(root, 'src-tauri', 'gen', 'android', 'gradle.properties');
if (fs.existsSync(gradlePath)) {
  const baseVersion = version.split('-')[0];
  const parts = baseVersion.split('.').map(Number);
  const versionCode = parts[0] * 1000000 + parts[1] * 1000 + parts[2];

  let gradle = fs.readFileSync(gradlePath, 'utf8');

  if (gradle.includes('tauri.android.versionName')) {
    gradle = gradle.replace(
      /tauri\.android\.versionName=.*/g,
      `tauri.android.versionName=${version}`
    );
    gradle = gradle.replace(
      /tauri\.android\.versionCode=.*/g,
      `tauri.android.versionCode=${versionCode}`
    );
  } else {
    gradle += `\ntauri.android.versionName=${version}\ntauri.android.versionCode=${versionCode}\n`;
  }

  fs.writeFileSync(gradlePath, gradle);
}

console.log(`v${version}`);
