---
name: new-component
description: Scaffold a new Svelte 5 component following project conventions.
argument-hint: <ComponentName> [feature-group]
---

Create a new Svelte 5 component in the Comine project.

## Arguments

Component: $ARGUMENTS

Parse the arguments:
- First word = ComponentName (PascalCase)
- Second word (optional) = feature group directory (ui, layout, download, settings, resolve, media, builders, providers)
- If no group specified, infer from the component's purpose or ask

## Template

Create `src/lib/components/{group}/{ComponentName}.svelte`:

```svelte
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    // Define typed props here
    children?: Snippet;
  }

  let { children }: Props = $props();
</script>

<div class="component-name">
  {#if children}
    {@render children()}
  {/if}
</div>

<style>
  .component-name {
    /* Use CSS variables: var(--accent), var(--radius-sm), var(--text-md), var(--surface-bg) */
  }
</style>
```

## After Scaffolding

1. Read 2-3 existing components in the same group to match patterns
2. Customize the template based on the component's purpose
3. Add appropriate ARIA attributes and keyboard handling
4. Add responsive styles if needed (`@media (max-width: 640px)`)
5. Explain the component's usage pattern
