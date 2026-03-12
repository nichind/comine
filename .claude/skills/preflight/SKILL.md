---
name: preflight
description: Run all pre-commit checks — formatting, code generation, and type checking.
context: fork
agent: build-runner
---

Run the full preflight check suite for the Comine project:

1. Check Rust formatting: `cd src-tauri && cargo fmt -- --check`
2. Check frontend formatting: `cd /Users/rodolfo/Developer/comine && npx prettier --check "src/**/*.{ts,svelte,js}"`
3. Run Rust check: `cd src-tauri && cargo check`
4. Run TypeScript/Svelte check: `cd /Users/rodolfo/Developer/comine && pnpm check`

Run steps 1-2 in parallel (formatting checks), then steps 3-4 in parallel (type checks).

Report results for each step. For failures, include the specific errors with file paths and line numbers.
