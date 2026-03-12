# Scripts — Build Utilities

All scripts are Node.js, run via pnpm.

## Scripts

| Script | Command | What it does |
|--------|---------|--------------|
| `bump-version.js` | `pnpm version:set <semver>` | Updates version in package.json, Cargo.toml, tauri.conf.json, gradle.properties |
| `generate-bindings.js` | `pnpm generate:bindings` | Runs `cargo test --features ts-export`, collects ts-rs output, replaces `bigint` → `number`, generates barrel `index.ts` in `src/lib/bindings/` |
| `generate-i18n-keys.js` | `pnpm generate:i18n-keys` | Reads `src/lib/i18n/locales/en.json`, generates `TranslationKeys` union type in `src/lib/i18n/keys.ts` |

## Code Generation Pipeline

After modifying Rust types with `#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]`:
1. `pnpm generate:bindings` → updates `src/lib/bindings/`
2. `pnpm check` → verifies TypeScript still compiles

After modifying `src/lib/i18n/locales/en.json`:
1. `pnpm generate:i18n-keys` → updates `src/lib/i18n/keys.ts`

## Version Format

Semver: `MAJOR.MINOR.PATCH` or `MAJOR.MINOR.PATCH-prerelease`. Android versionCode = `MAJOR*1000000 + MINOR*1000 + PATCH`.

## Preflight (Pre-Commit)

`pnpm preflight` runs: format → generate bindings → generate i18n keys → svelte-check. Always run before committing.
