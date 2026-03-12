---
name: review
description: Review recent code changes or specific files for quality, conventions, and correctness.
context: fork
agent: code-reviewer
argument-hint: [file or area to review, or blank for recent changes]
---

Review the following in the Comine project:

$ARGUMENTS

If no specific files were given, review recent uncommitted changes by running `git diff` and `git diff --cached`.

Follow your review checklist thoroughly. Check frontend conventions (Svelte 5 runes, accessibility, i18n, CSS variables), backend conventions (command signatures, error handling, type system, async patterns), and cross-cutting concerns (no hardcoded strings, no secrets, proper logging, consistent formatting).

Report findings with file:line references and severity levels.
