---
name: new-command
description: Scaffold a new Tauri command with proper Rust function, command registration, and TypeScript types.
argument-hint: <command_name> [module]
---

Create a new Tauri command in the Comine project.

## Arguments

Command: $ARGUMENTS

Parse the arguments:
- First word = command_name (snake_case)
- Second word (optional) = module to place it in (orchestrator, deps, or a top-level module like lib.rs)

## Steps

### 1. Create the Command Function

In the appropriate module:

```rust
#[tauri::command]
async fn command_name(
    app: AppHandle,
    // Add State<'_, Arc<T>> if needed
    // Add parameters
) -> Result<ReturnType, String> {
    // Implementation
    Ok(result)
}
```

### 2. Define Types (if needed)

In `src-tauri/src/orchestrator/types.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct NewType {
    pub field: String,
}
```

### 3. Register the Command

Add to `tauri::generate_handler![]` in `src-tauri/src/lib.rs`.

### 4. Verify

Run `cargo check` from `src-tauri/`.

### 5. Report

- Show the command signature
- Show TypeScript usage: `invoke<ReturnType>('command_name', { param: value })`
- Note if `pnpm generate:bindings` needs to run
