---
name: feature
description: Implement a new feature end-to-end. Researches, plans, implements across frontend and backend, reviews, and validates.
argument-hint: <feature description>
---

You are implementing a new feature in the Comine project — a cross-platform media downloader (Tauri 2 + Svelte 5 + Rust).

## Feature Request

$ARGUMENTS

## Process

### 1. Research Phase

Use the Explore agent to understand:
- Existing related functionality (search for similar features)
- Files that will be affected
- Patterns used in similar features
- Backend types and commands available
- Frontend components and stores involved

### 2. Design Phase

Outline the implementation:
- **Backend changes** (if any): new types, commands, modifications to orchestrator/deps
- **Frontend changes** (if any): new components, store updates, route changes, i18n keys
- **Type bridge**: any new types that need Rust → TypeScript bindings
- **Edge cases**: platform differences, error states, loading states

### 3. Implementation Phase

Execute in order:
1. **Backend types** (if new types needed) — add to `orchestrator/types.rs` with serde + ts-rs derives
2. **Backend logic** — new commands, business logic. Use `rust-dev` agent.
3. **Generate bindings** — note that `pnpm generate:bindings` needs to run if types changed
4. **Frontend implementation** — components, stores, i18n. Use `svelte-dev` agent.
5. **Generate i18n keys** — note if `pnpm generate:i18n-keys` is needed

### 4. Quality Phase

- Use `code-reviewer` agent to review all changes
- Use `build-runner` agent to verify compilation
- List any manual testing steps needed

### 5. Summary

Report: files changed, what was implemented, any follow-up needed.
