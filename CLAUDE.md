# Comine

Cross-platform media downloader. Tauri 2 + Svelte 5 + Rust. Targets Windows, macOS, Linux, Android.

## Dev Commands

```bash
pnpm dev                    # Frontend dev server (port 1420)
pnpm tauri dev              # Full app dev (frontend + Rust)
pnpm tauri dev --target android  # Android dev

# iOS build + deploy (MUST use this sequence, NOT `pnpm tauri ios dev`)
cd src-tauri/gen/apple && xcodegen generate  # 1. Regenerate Xcode project from project.yml
pnpm tauri ios build --debug                 # 2. Build debug IPA
ios-deploy --bundle src-tauri/gen/apple/build/arm64/comine.ipa  # 3. Deploy to device
pnpm build                  # Production frontend build
pnpm check                  # svelte-kit sync + svelte-check
pnpm format                 # Prettier (frontend)
pnpm format:rust            # cargo fmt (backend)
pnpm generate:bindings      # Rust types → TypeScript (src/lib/bindings/)
pnpm generate:i18n-keys     # en.json → TranslationKeys union type
pnpm preflight              # format + generate + check (run before committing)
pnpm version:set <ver>      # Bump version across all configs
cd src-tauri && cargo check  # Rust type checking
cd src-tauri && cargo test   # Rust tests
```

## Architecture

```
src/                          # SvelteKit frontend (Svelte 5 + TypeScript)
  routes/                     # Pages: home, downloads, settings, logs, info, notification
  lib/stores/                 # State: settings, history, queue, logs, navigation, deps
  lib/components/             # UI organized by feature: ui/, layout/, download/, settings/, resolve/, media/
  lib/backend/                # Tauri IPC bridge (invoke + listen)
  lib/bindings/               # Auto-generated TypeScript types from Rust (DO NOT edit manually)
  lib/i18n/                   # Translations (en, ru) + generated key types
  lib/utils/                  # Pure helpers (format, url, color, platform)
  lib/composables/            # Reactive logic (remoteSync, clipboardHandler, extensionBridge)
  lib/actions/                # Svelte actions (tooltip, spotlight, portal, edgeMask)

src-tauri/src/                # Rust backend
  lib.rs                      # App setup, command registration (~50 commands), plugin init
  orchestrator/               # Download job management (JobManager, JobStore, HistoryStore)
    backends/                 # Downloader impls: ytdlp/, aria2, gallery_dl/, direct
  deps/                       # External tool management (yt-dlp, ffmpeg, aria2, gallery-dl, etc.)
    specs/                    # Per-dependency install/update/check logic
    engine/                   # Download, extract, checksum, verify pipeline
  database.rs                 # SQLite (WAL mode): history, jobs, stats tables
  clipboard.rs                # Clipboard URL watcher (500ms poll)
  proxy.rs                    # System/custom proxy detection + caching
  relay.rs                    # WebSocket relay for remote pairing (AES-256-GCM)
  server.rs                   # Local HTTP server for browser extension
  notifications.rs            # Positioned notification windows
  updater.rs                  # GitHub releases auto-updater

scripts/                      # Build utilities (version bumping, binding gen, i18n key gen)
```

## IPC Contract

Frontend calls Rust via `invoke('command_name', { args })`. Backend emits events via `app.emit("event-name", data)`. All shared types live in `src-tauri/src/orchestrator/types.rs` and are exported to TypeScript via `ts-rs` with `#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]`.

**Type flow**: Rust struct → `pnpm generate:bindings` → `src/lib/bindings/index.ts` → frontend imports.

## Conventions

- **Formatting**: Prettier (frontend, 100 char width, single quotes, 2-space indent) + rustfmt (backend, 100 char width, 4-space indent)
- **Naming**: PascalCase for types/components, camelCase for functions/variables/stores, snake_case for Rust, kebab-case for CSS vars and events
- **Serde**: All cross-boundary types use `#[serde(rename_all = "camelCase")]`
- **Imports**: Use `$lib/` alias for all frontend library imports
- **Events**: kebab-case naming (`job-progress`, `history-item-added`)
- **Errors**: Tauri commands return `Result<T, String>`. Backend errors use `BackendError` enum with `is_retryable()`.
- **Platform guards**: `#[cfg(target_os = "android")]` / `#[cfg(not(target_os = "android"))]` for platform-specific code
- **State**: Frontend uses Svelte 5 runes ($state, $derived, $effect) + Svelte stores for global state. Backend uses DashMap, RwLock, Atomic* for concurrency.
- **No manual edits** to `src/lib/bindings/` — always regenerate with `pnpm generate:bindings`
- **No manual edits** to `src/lib/i18n/keys.ts` — always regenerate with `pnpm generate:i18n-keys`

## Platform Considerations

- Android: No tray, no window effects, no autostart, no Discord RPC. Uses JNI for file operations.
- Linux: Updater disabled (package manager handles updates).
- Windows: Set `PYTHONIOENCODING=utf-8` for yt-dlp subprocess. Acrylic/Mica window effects.
- macOS: Vibrancy effects. Universal binary (arm64 + x64).
- iOS: Do NOT use `pnpm tauri ios dev` — it produces a black screen. Always use `xcodegen generate` + `pnpm tauri ios build --debug` + `ios-deploy`. Clear DerivedData (`rm -rf ~/Library/Developer/Xcode/DerivedData/comine-*`) if builds behave unexpectedly. XcodeGen project config lives in `src-tauri/gen/apple/project.yml`.
