import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { execSync } from 'child_process';
import { readFileSync } from 'fs';

function getGitInfo() {
  try {
    return {
      hash: execSync('git rev-parse HEAD').toString().trim(),
      branch: execSync('git rev-parse --abbrev-ref HEAD').toString().trim(),
    };
  } catch {
    return { hash: 'unknown', branch: 'unknown' };
  }
}

function getVersion() {
  try {
    return JSON.parse(readFileSync('package.json', 'utf8')).version;
  } catch {
    return '0.0.0';
  }
}

// https://vite.dev/config/
export default defineConfig(async () => {
  const host = process.env.TAURI_DEV_HOST;
  const gitInfo = getGitInfo();

  return {
    plugins: [sveltekit()],

    define: {
      __DEV__: JSON.stringify(process.env.NODE_ENV !== 'production'),
      __COMMIT_HASH__: JSON.stringify(gitInfo.hash),
      __GIT_BRANCH__: JSON.stringify(gitInfo.branch),
      __APP_VERSION__: JSON.stringify(getVersion()),
      __BUILD_DATE__: JSON.stringify(new Date().toISOString().split('T')[0]),
    },

    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      host: host || '0.0.0.0',
      hmr: host
        ? {
            protocol: 'ws',
            host: 'localhost',
            port: 1421,
          }
        : true,
      watch: {
        ignored: ['**/src-tauri/**'],
      },
    },
  };
});
