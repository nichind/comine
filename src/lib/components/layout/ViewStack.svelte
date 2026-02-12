<script lang="ts" module>
  import { logs } from '$lib/stores/logs';

  export interface ViewInstance {
    id: string;
    type: 'home' | 'video' | 'playlist' | 'channel';
    url?: string;
    cachedData?: {
      title?: string;
      thumbnail?: string;
      author?: string;
      duration?: number;
      entryCount?: number;
    };
    mountedAt: number;
  }
</script>

<script lang="ts">
  import { navigation, type ViewState } from '$lib/stores/navigation';
  import { untrack } from 'svelte';

  interface Props {
    children: import('svelte').Snippet<
      [
        {
          views: ViewInstance[];
          currentId: string;
          isActive: (id: string) => boolean;
        },
      ]
    >;
  }

  let { children }: Props = $props();

  function getViewId(view: ViewState, index: number): string {
    if (view.type === 'home') return 'home-0';
    if (view.url) return `${view.type}-${view.url}`;
    return `${view.type}-${index}-${Date.now()}`;
  }

  function getCurrentView(): ViewInstance {
    const stack = navigation.getStack?.() ?? [{ type: 'home' }];
    const current = stack[stack.length - 1];
    return {
      id: getViewId(current, stack.length - 1),
      type: current.type,
      url: current.url,
      cachedData: current.cachedData,
      mountedAt: Date.now(),
    };
  }

  let currentView = $state<ViewInstance>(getCurrentView());

  $effect(() => {
    const stack = $navigation.stack;

    untrack(() => {
      const topView = stack[stack.length - 1];
      const newId = getViewId(topView, stack.length - 1);

      if (currentView.id !== newId) {
        logs.debug('ViewStack', `Switching view: ${currentView.id} → ${newId}`);
        currentView = {
          id: newId,
          type: topView.type,
          url: topView.url,
          cachedData: topView.cachedData,
          mountedAt: Date.now(),
        };
      }
    });
  });

  let mountedViews = $derived([currentView]);

  function isActive(id: string): boolean {
    return id === currentView.id;
  }
</script>

{@render children({ views: mountedViews, currentId: currentView.id, isActive })}
