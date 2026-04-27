---
name: check
description: Run type checking for frontend (svelte-check) and backend (cargo check) in parallel.
context: fork
agent: build-runner
---

Run type checks for the Comine project in parallel:

1. Frontend: `cd /Users/rodolfo/Developer/comine && pnpm check`
2. Backend: `cd /Users/rodolfo/Developer/comine/src-tauri && cargo check`

Report results concisely. For failures, show the specific type errors with file paths.
