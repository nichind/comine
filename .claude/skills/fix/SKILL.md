---
name: fix
description: Diagnose and fix a bug. Investigates the issue, identifies root cause, implements the fix, and validates.
argument-hint: <bug description or error message>
---

You are fixing a bug in the Comine project — a cross-platform media downloader (Tauri 2 + Svelte 5 + Rust).

## Bug Report

$ARGUMENTS

## Process

### 1. Investigate

- Search for the error message, relevant function names, or affected UI elements
- Read the relevant source files to understand the current behavior
- Trace the data flow (frontend → IPC → backend or vice versa)
- Check if this is a platform-specific issue

### 2. Root Cause

Identify and clearly state:
- What is happening vs. what should happen
- The exact location(s) of the bug
- Why the current code produces the wrong behavior

### 3. Fix

- Make the minimal change to fix the bug
- Do NOT refactor surrounding code
- Do NOT add features while fixing
- Preserve existing patterns and conventions

### 4. Validate

- Run `cargo check` and/or `pnpm check` as appropriate
- Describe how to verify the fix manually
- Note any edge cases that should be tested

### 5. Summary

Report: root cause, what was changed, how to verify.
