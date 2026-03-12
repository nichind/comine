# Frontend — SvelteKit + Svelte 5 + TypeScript

## Svelte 5 Patterns

This project uses **Svelte 5 runes** throughout. Never use Svelte 4 syntax (`export let`, `$:` reactive declarations).

```svelte
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    value: string;
    disabled?: boolean;
    children?: Snippet;
    onchange?: (value: string) => void;
  }

  let { value = $bindable(''), disabled = false, children, onchange }: Props = $props();

  let internal = $state('');
  let computed = $derived(value.length > 0);

  $effect(() => {
    // side effects tracking `value`
  });
</script>
```

**Key rules:**
- `$state()` for reactive local state. Use `$state.raw()` for large objects to avoid deep proxying.
- `$derived()` / `$derived.by()` for computed values — prefer over `$effect` when possible.
- `$effect()` for side effects only. Use `$effect.root()` in class-based stores for cleanup.
- `$props()` with destructuring and interface. Use `$bindable()` for two-way bound props.
- `type Snippet` for slot-like child content.

## Component Conventions

- **Location**: `lib/components/{feature}/ComponentName.svelte` — group by feature (ui, layout, download, settings, resolve, media, providers, builders)
- **Props**: Define `interface Props` in script block, destructure with defaults
- **Events**: Callback props (`onclick`, `onchange`) — not `createEventDispatcher`
- **Actions**: Apply with `use:actionName` — existing actions: tooltip, spotlight, portal, edgeMask, skeleton, preserveScroll
- **Accessibility**: Use semantic HTML, ARIA attributes (`role`, `aria-checked`, `aria-label`, `aria-modal`), keyboard support (Enter, Space, Escape)
- **Styling**: Scoped `<style>` blocks. Use CSS variables: `--accent`, `--radius-*`, `--text-*`, `--surface-*`. Responsive: `@media (max-width: 640px)`, `(pointer: coarse)` for touch.

## Store Patterns

Stores live in `lib/stores/`. Two patterns:

**1. Factory function (Svelte store):**
```typescript
// lib/stores/example.ts
function createExampleStore() {
  const { subscribe, set, update } = writable<ExampleState>(initialState);
  return {
    subscribe,
    async init() { /* invoke backend, set initial state */ },
    async doThing() { /* invoke backend, update state */ },
  };
}
export const example = createExampleStore();
```

**2. Class-based (Svelte 5 runes — use `.svelte.ts` extension):**
```typescript
// lib/stores/example.svelte.ts
export class ExampleState {
  items = $state<Item[]>([]);
  query = $state('');
  filtered = $derived.by(() => this.items.filter(i => i.name.includes(this.query)));
  constructor() { $effect.root(() => { /* setup */ }); }
}
```

## IPC Bridge

```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { SomeType } from '$lib/bindings';

// Request-response
const result = await invoke<SomeType>('command_name', { argName: value });

// Event streaming
const unlisten = await listen<EventPayload>('event-name', (event) => {
  // event.payload
});
```

## Bindings

All types in `lib/bindings/index.ts` are **auto-generated** from Rust. Import as:
```typescript
import type { UrlInfo, Job, JobStatus, DownloadSettings } from '$lib/bindings';
```

## Settings Store

The main settings store (`lib/stores/settings.ts`) uses `@tauri-apps/plugin-store` for persistence. Update via:
```typescript
import { updateSetting, updateSettings } from '$lib/stores/settings';
await updateSetting('propertyName', value);           // single
await updateSettings({ prop1: val1, prop2: val2 });   // batch
await updateSettingDotNotation('parent', parentObj, 'nested.key'); // nested
```

## i18n

```typescript
import { t } from '$lib/i18n';          // reactive (use in templates)
import { translate } from '$lib/i18n';   // non-reactive (use in scripts)

// Template: {$t('downloads.status.downloading')}
// Script:   translate('settings.general.title')
```

Add new keys to `lib/i18n/locales/en.json`, then run `pnpm generate:i18n-keys`.

## Navigation

View-stack based (not file-based routing for in-app navigation). Use the navigation store:
```typescript
import { navigation } from '$lib/stores/navigation';
navigation.push({ type: 'video', data: urlInfo });
navigation.pop();
navigation.replace({ type: 'home' });
```

Routes (`routes/`) are for top-level pages (home, downloads, settings, logs, info, notification).

## Import Conventions

```typescript
// Always use $lib alias
import Component from '$lib/components/ui/Component.svelte';
import { storeName } from '$lib/stores/storeName';
import type { TypeName } from '$lib/bindings';
import { utilFunction } from '$lib/utils/utilFile';
import { t } from '$lib/i18n';
```
