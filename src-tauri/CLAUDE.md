# Backend — Rust + Tauri 2

## Tauri Command Pattern

All commands follow this signature:

```rust
#[tauri::command]
async fn command_name(
    app: AppHandle,
    state: State<'_, Arc<JobManager>>,  // or other managed state
    param: ParamType,
) -> Result<ReturnType, String> {
    // ...
    Ok(result)
}
```

- Commands return `Result<T, String>` — map errors with `.map_err(|e| e.to_string())`
- Register in `lib.rs` via `tauri::generate_handler![command_name]`
- Async commands run on a thread pool — never block the main thread
- Access managed state via `State<'_, T>` — Tauri wraps in Arc internally
- For spawned tasks, clone `AppHandle` (cheap) and use `app.state::<T>()`

## Module Structure

```
src/
├── lib.rs                  # Setup + all command registrations
├── orchestrator/
│   ├── mod.rs              # Command handlers (resolve_url, start_job, control_job, etc.)
│   ├── manager.rs          # JobManager — central download coordinator
│   ├── types.rs            # ALL shared types (Job, UrlInfo, DownloadRequest, etc.)
│   ├── store.rs            # JobStore — SQLite persistence for active jobs
│   ├── history.rs          # HistoryStore — completed download records
│   ├── stats.rs            # StatsStore — aggregate download statistics
│   ├── convert.rs          # FFmpeg conversion tasks
│   ├── thumbnail.rs        # Thumbnail caching
│   └── backends/
│       ├── mod.rs           # Backend trait + BackendRegistry
│       ├── common.rs        # Shared utils (URL parsing, MIME, proxy resolution)
│       ├── ytdlp/           # Primary backend (subprocess management)
│       ├── aria2.rs         # Parallel download backend
│       ├── gallery_dl/      # Image gallery backend
│       └── direct.rs        # Fallback HTTP download
├── deps/
│   ├── mod.rs, commands.rs  # Dependency check/install/uninstall commands
│   ├── error.rs             # DepsError, DownloadError, ExtractError
│   ├── updater.rs           # Auto-update checker for deps
│   ├── specs/               # Per-dependency: ytdlp, ffmpeg, aria2, gallery_dl, deno, quickjs, lux
│   └── engine/              # Download → extract → checksum → verify pipeline
└── [clipboard, database, proxy, relay, server, notifications, updater, ...]
```

## Type System

All cross-boundary types live in `orchestrator/types.rs`. Requirements:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct MyType {
    pub field_name: String,      // → camelCase in TS
    pub optional: Option<u64>,   // → number | null in TS
}
```

- `#[serde(rename_all = "camelCase")]` on ALL shared types
- `#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]` for TypeScript generation
- Tagged enums: `#[serde(tag = "type", content = "data")]` for Job status variants
- After modifying types, run `pnpm generate:bindings` to update TypeScript

## Error Handling

**Download errors** — `BackendError` enum in `orchestrator/types.rs`:
```rust
pub enum BackendError {
    NotFound(String),           // 404
    Forbidden(String),          // 403
    RateLimited { retry_after: Option<u64> }, // 429
    NetworkError(String),
    ProcessError { code: Option<i32>, stderr: String },
    // ...
}
```
Each variant has `is_retryable() -> bool`.

**Dependency errors** — `DepsError` in `deps/error.rs` with nested Download/Extract/Verification variants.

**Command errors** — always `Result<T, String>`. Convert with `.map_err(|e| e.to_string())`.

## Async Patterns

- `tokio::spawn()` — fire-and-forget async tasks (job execution, event emission)
- `tokio::task::spawn_blocking()` — DB operations, CPU-heavy work (thumbnail processing)
- `CancellationToken` — all long-running tasks check `token.is_cancelled()` in their progress loops
- `DashMap` — lock-free concurrent map for jobs, running tasks
- `RwLock` — backend registry, download settings
- `Atomic*` — simple counters (active_count, max_concurrent, speed_limit)
- `Notify` — signal settings changes, persistence triggers

## Backend Trait (Adding New Downloaders)

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> BackendCapabilities;
    fn priority(&self, url: &str) -> Priority;  // No network — fast URL matching only
    async fn resolve(&self, url: &str, settings: &ResolveSettings) -> Result<UrlInfo, BackendError>;
    async fn spawn(&self, ctx: SpawnContext) -> Result<String, BackendError>;
}
```

Register in `BackendRegistry` in `orchestrator/backends/mod.rs`.

## Event Emission

```rust
// Emit to frontend
app.emit("job-progress", &JobEvent::Progress { id, progress, speed, eta })?;

// Event naming: kebab-case
// Payload: serialize as JSON automatically
```

## Database

SQLite with WAL mode, 5s busy timeout, NORMAL synchronous. Tables: `history`, `jobs`, `stats`. Access via `Mutex<Connection>` with `lock_or_recover()` for poisoned mutex safety. All DB operations run in `spawn_blocking()`.

## Platform Conditionals

```rust
#[cfg(target_os = "android")]       // Android-only
#[cfg(not(target_os = "android"))]  // Desktop-only
#[cfg(target_os = "windows")]       // Windows-only
#[cfg(target_os = "macos")]         // macOS-only
#[cfg(target_os = "linux")]         // Linux-only
```

Desktop features not on Android: tray, window effects, autostart, Discord RPC, file reveal.
