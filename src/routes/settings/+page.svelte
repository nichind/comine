<script lang="ts">
  import { t } from '$lib/i18n';
  import { settings, updateSetting, defaultSettings, type AppSettings } from '$lib/stores/settings';
  import { pushState } from '$app/navigation';
  import {
    SECTIONS,
    SETTINGS,
    getSettingValue,
    isVisibleOnPlatform,
    type SettingDef,
    type Platform,
    type PlatformGroup,
  } from '$lib/settings/schema';
  import { isAndroid, isDesktop } from '$lib/utils/android';
  import { onMount, onDestroy, tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { tooltip } from '$lib/actions/tooltip';
  import { open } from '@tauri-apps/plugin-dialog';
  import Icon from '$lib/components/Icon.svelte';
  import ScrollArea from '$lib/components/ScrollArea.svelte';
  import SettingsBlock from '$lib/components/SettingsBlock.svelte';
  import SettingItem from '$lib/components/SettingItem.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import Select from '$lib/components/Select.svelte';
  import Input from '$lib/components/Input.svelte';
  import Divider from '$lib/components/Divider.svelte';
  import ExtensionIntegrationSettings from '$lib/components/ExtensionIntegrationSettings.svelte';
  import AccentPicker from './components/AccentPicker.svelte';
  import AccentStyle from './components/AccentStyle.svelte';
  import Dependencies from './components/Dependencies.svelte';
  import DataActions from './components/DataActions.svelte';
  import ProxyConfig from './components/ProxyConfig.svelte';
  import NetworkCheck from './components/NetworkCheck.svelte';
  import AppUpdates from './components/AppUpdates.svelte';
  import { calculateMatchScore } from '$lib/utils/search';
  import {
    hasOpenAriaMenu,
    hasOpenAriaModal,
    isPrintableKey,
    isTypingTarget,
    matchesShortcut,
  } from '$lib/utils/keyboard';

  const APP_VERSION = __APP_VERSION__;
  const COMMIT_HASH = __COMMIT_HASH__;
  const BUILD_DATE = __BUILD_DATE__;

  const SCROLL_STORAGE_KEY = 'settings-scroll-positions';
  const SECTION_STORAGE_KEY = 'settings-active-section';
  const DEFAULT_SUBSECTION = '__default__';

  const SUBSECTION_TITLES: Record<string, string> = {
    'general:language': 'settings.general.language',
    'general:startup': 'settings.general.startOnBoot',
    'general:clipboard': 'settings.general.watchClipboard',
    'general:closeBehavior': 'settings.general.closeBehavior',
    'downloads:paths': 'settings.downloads.downloadPath',
    'downloads:folders': 'settings.downloads.usePlaylistFolders',
    'downloads:concurrency': 'settings.downloads.concurrentDownloads',
    'downloads:notifications': 'settings.notifications.title',
    'downloads:aria2': 'aria2',
    'downloads:network': 'settings.network.title',
    'notifications:general': 'settings.notifications.groups.general',
    'notifications:layout': 'settings.notifications.groups.layout',
    'notifications:style': 'settings.notifications.groups.style',
    'notifications:timing': 'settings.notifications.groups.timing',
    'processing:backend': 'settings.processing.groups.backend',
    'processing:youtube': 'ytdlp.advanced.youtube.title',
    'processing:extraction': 'ytdlp.advanced.extraction.title',
    'processing:download': 'ytdlp.advanced.download.title',
    'processing:output': 'ytdlp.advanced.output.title',
    'processing:postProcess': 'ytdlp.advanced.postProcess.title',
  };

  function getSubsectionTitle(sectionId: string, subsection: string): string | undefined {
    const key = `${sectionId}:${subsection}`;
    return SUBSECTION_TITLES[key];
  }

  interface ScrollAreaInstance {
    restoreScroll(position: number): void;
    getScroll(): number;
    scrollToTop(smooth?: boolean): void;
  }

  let activeSection = $state('general');
  let sectionTransitionDir = $state(1);
  let searchQuery = $state('');
  let searchExpanded = $state(false);
  let onDesktop = $state(true);
  let platform = $state<Platform>('windows');

  let scrollAreaRef: ScrollAreaInstance | undefined = $state(undefined);
  let searchInputRef: HTMLInputElement | null = $state(null);
  let initialScrollTop: number | undefined = $state(undefined);

  let scrollCache: Record<string, number> = {};

  const initializeState = (() => {
    if (typeof window === 'undefined') return;

    const desktop = isDesktop();
    onDesktop = desktop;

    if (desktop && typeof navigator !== 'undefined') {
      const userAgent = navigator.userAgent.toLowerCase();
      if (userAgent.includes('mac')) platform = 'macos';
      else if (userAgent.includes('linux') && !userAgent.includes('android')) platform = 'linux';
      else platform = 'windows';
    } else {
      platform = 'android';
    }

    try {
      const raw = sessionStorage.getItem(SCROLL_STORAGE_KEY);
      if (raw) scrollCache = JSON.parse(raw);
    } catch {}

    const initial = getHashSection() ?? getSavedSection();
    const section = desktop ? (initial ?? 'general') : (initial ?? '');
    activeSection = section;

    if (desktop && !getHashSection()) {
      setHashSection(section);
    }

    if (section) {
      saveActiveSection(section);
      initialScrollTop = scrollCache[section] || 0;
    }
  })();

  let scrollSaveTimer: ReturnType<typeof setTimeout> | null = null;
  function handleScrollChange(position: number) {
    if (!activeSection) return;
    scrollCache[activeSection] = position;

    if (scrollSaveTimer) clearTimeout(scrollSaveTimer);
    scrollSaveTimer = setTimeout(() => {
      try {
        sessionStorage.setItem(SCROLL_STORAGE_KEY, JSON.stringify(scrollCache));
      } catch (e) {
        console.warn('Failed to save scroll position:', e);
      }
    }, 500);
  }

  function getSavedScrollPosition(section: string): number {
    return scrollCache[section] || 0;
  }

  function saveActiveSection(section: string) {
    if (!section) return;
    try {
      sessionStorage.setItem(SECTION_STORAGE_KEY, section);
    } catch {}
  }

  function getSavedSection(): string | null {
    try {
      const saved = sessionStorage.getItem(SECTION_STORAGE_KEY);
      return saved && SECTIONS.some((s) => s.id === saved) ? saved : null;
    } catch {
      return null;
    }
  }

  let sidebarSectionsEl: HTMLDivElement | null = $state(null);
  let sidebarIndicatorStyle = $state('');
  let sidebarIndicatorVisible = $state(false);
  const sidebarItemEls = new Map<string, HTMLElement>();

  let sidebarIndicatorRaf: number | null = null;
  function queueSidebarIndicatorUpdate() {
    if (!onDesktop) return;
    if (sidebarIndicatorRaf !== null) cancelAnimationFrame(sidebarIndicatorRaf);
    sidebarIndicatorRaf = requestAnimationFrame(() => {
      sidebarIndicatorRaf = null;
      updateSidebarIndicator();
    });
  }

  function updateSidebarIndicator() {
    if (!onDesktop) return;
    if (!sidebarSectionsEl || !activeSection) {
      sidebarIndicatorVisible = false;
      sidebarIndicatorStyle = '';
      return;
    }

    const activeEl = sidebarItemEls.get(activeSection);
    if (!activeEl) {
      sidebarIndicatorVisible = false;
      sidebarIndicatorStyle = '';
      return;
    }

    const containerRect = sidebarSectionsEl.getBoundingClientRect();
    const activeRect = activeEl.getBoundingClientRect();
    const top = Math.round(activeRect.top - containerRect.top + sidebarSectionsEl.scrollTop);
    const height = Math.round(activeRect.height);

    sidebarIndicatorVisible = true;
    sidebarIndicatorStyle = `transform: translateY(${top}px); height: ${height}px;`;
  }

  function registerSidebarItem(node: HTMLElement, sectionId: string) {
    sidebarItemEls.set(sectionId, node);
    queueSidebarIndicatorUpdate();
    return {
      destroy() {
        sidebarItemEls.delete(sectionId);
        queueSidebarIndicatorUpdate();
      },
    };
  }

  function getHashSection(): string | null {
    if (typeof window === 'undefined') return null;
    const h = window.location.hash.slice(1);
    return h && SECTIONS.some((s) => s.id === h) ? h : null;
  }

  function setHashSection(section: string) {
    if (typeof window === 'undefined') return;
    if (window.location.hash === `#${section}`) return;
    const url = new URL(window.location.href);
    url.hash = section;
    pushState(url, {});
  }

  onMount(() => {
    tick().then(() => {
      queueSidebarIndicatorUpdate();
    });

    const onHashChange = () => {
      const next = getHashSection();
      if (onDesktop) {
        activeSection = next ?? 'general';
        if (!next) setHashSection(activeSection);
      } else {
        activeSection = next ?? '';
      }
      const savedPos = getSavedScrollPosition(activeSection);
      tick().then(() => scrollAreaRef?.restoreScroll?.(savedPos));
      queueSidebarIndicatorUpdate();
      if (activeSection) saveActiveSection(activeSection);
    };

    const onPopState = () => {
      onHashChange();
    };

    window.addEventListener('hashchange', onHashChange);
    window.addEventListener('popstate', onPopState);
    window.addEventListener('resize', queueSidebarIndicatorUpdate);

    const onKeyDown = (e: KeyboardEvent) => {
      if (!onDesktop) return;
      if (e.defaultPrevented) return;
      if (isTypingTarget(document.activeElement)) return;

      if (hasOpenAriaModal()) return;

      if (hasOpenAriaMenu()) return;

      const bindings: Array<{
        match: (e: KeyboardEvent) => boolean;
        run: (e: KeyboardEvent) => void | Promise<void>;
        preventDefault?: boolean;
      }> = [
        {
          match: (e) => matchesShortcut(e, { key: 'f', mod: true }),
          run: async () => {
            searchExpanded = true;
            await tick();
            searchInputRef?.focus();
          },
        },
        {
          match: (e) => !e.ctrlKey && !e.metaKey && !e.altKey && e.key === 'Escape',
          run: () => {
            if (searchQuery.trim() || searchExpanded) {
              searchQuery = '';
              searchExpanded = false;
              queueSidebarIndicatorUpdate();
            }
          },
        },
        {
          match: (e) => isPrintableKey(e, { excludeSpace: true }),
          run: (e) => {
            if (!searchQuery.trim() && e.key.trim() === '') return;
            void openSearch(e.key);
          },
        },
      ];

      for (const binding of bindings) {
        if (!binding.match(e)) continue;
        if (binding.preventDefault !== false) e.preventDefault();
        void binding.run(e);
        return;
      }
    };

    window.addEventListener('keydown', onKeyDown, true);
    return () => {
      if (activeSection) saveActiveSection(activeSection);
      window.removeEventListener('keydown', onKeyDown, true);
      window.removeEventListener('hashchange', onHashChange);
      window.removeEventListener('popstate', onPopState);
      window.removeEventListener('resize', queueSidebarIndicatorUpdate);
      if (sidebarIndicatorRaf !== null) cancelAnimationFrame(sidebarIndicatorRaf);
    };
  });

  async function openSearch(initialText?: string) {
    if (!onDesktop) return;
    searchExpanded = true;
    if (typeof initialText === 'string' && initialText.length) {
      searchQuery = (searchQuery ?? '') + initialText;
    }
    await tick();
    searchInputRef?.focus();
    queueSidebarIndicatorUpdate();
    try {
      const len = searchQuery.length;
      searchInputRef?.setSelectionRange(len, len);
    } catch {}
  }

  function collapseSearchIfEmpty() {
    if (!searchQuery.trim()) {
      searchExpanded = false;
      queueSidebarIndicatorUpdate();
    }
  }

  let visibleSections = $derived(
    SECTIONS.filter((s) =>
      isVisibleOnPlatform('platforms' in s ? s.platforms : undefined, platform)
    )
  );

  let filteredSettings = $derived(
    SETTINGS.filter((def) => {
      if (searchQuery.trim()) {
        const query = searchQuery.trim();
        const title = $t(def.titleKey);
        const desc = def.descriptionKey ? $t(def.descriptionKey) : '';
        const keywords = 'keywords' in def && def.keywords ? def.keywords.join(' ') : '';

        if ('platforms' in def && !isVisibleOnPlatform(def.platforms, platform)) return false;
        if (def.visible && !def.visible($settings)) return false;

        const titleScore = calculateMatchScore(title, query);
        const descScore = calculateMatchScore(desc, query) * 0.5;
        const keywordScore = calculateMatchScore(keywords, query) * 0.8;

        return Math.max(titleScore, descScore, keywordScore) > 0;
      }

      if (def.section !== activeSection) return false;
      if ('platforms' in def && !isVisibleOnPlatform(def.platforms, platform)) return false;
      if (def.visible && !def.visible($settings)) return false;

      return true;
    })
  );

  type SubsectionGroup = {
    subsection: string | undefined;
    titleKey: string | undefined;
    items: SettingDef[];
  };
  type SectionGroup = { id: string; title: string; icon: string; subsections: SubsectionGroup[] };

  let groupedSettings = $derived.by(() => {
    const sections: Record<string, SectionGroup> = {};

    for (const def of filteredSettings) {
      const secId = def.section;
      const subId = def.subsection || DEFAULT_SUBSECTION;

      if (!sections[secId]) {
        const secDef = SECTIONS.find((s) => s.id === secId);
        sections[secId] = {
          id: secId,
          title: secDef ? $t(secDef.titleKey) : secId,
          icon: secDef ? secDef.icon : 'settings',
          subsections: [],
        };
      }

      let subsec = sections[secId].subsections.find((s) => s.subsection === subId);
      if (!subsec) {
        const titleKey = def.subsection ? getSubsectionTitle(secId, def.subsection) : undefined;
        subsec = {
          subsection: subId === DEFAULT_SUBSECTION ? undefined : subId,
          titleKey,
          items: [],
        };
        sections[secId].subsections.push(subsec);
      }
      subsec.items.push(def);
    }
    return Object.values(sections);
  });

  function handleSectionChange(id: string) {
    const sectionIds = visibleSections.map((s) => s.id as string);
    const oldIndex = sectionIds.indexOf(activeSection);
    const newIndex = sectionIds.indexOf(id);
    sectionTransitionDir = newIndex >= oldIndex ? 1 : -1;

    activeSection = id;
    setHashSection(id);
    saveActiveSection(id);
    const savedPos = getSavedScrollPosition(id);
    tick().then(() => scrollAreaRef?.restoreScroll?.(savedPos));
    searchQuery = '';
    queueSidebarIndicatorUpdate();
  }

  const pendingSliderUpdates = new Map<string, { key: string; value: unknown; def: SettingDef }>();
  let sliderDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  function flushSliderUpdates() {
    for (const { key, value, def } of pendingSliderUpdates.values()) {
      updateSetting(key as keyof AppSettings, value as AppSettings[keyof AppSettings]);
      if ('onSet' in def && def.onSet) def.onSet(value);
    }
    pendingSliderUpdates.clear();
  }

  function handleSliderInput(def: SettingDef, value: number) {
    const key = def.key;
    settings.update((s) => {
      if (key.includes('.')) {
        const [parent, child] = key.split('.');
        const parentObj = s[parent as keyof AppSettings];
        if (typeof parentObj === 'object' && parentObj !== null) {
          return { ...s, [parent]: { ...parentObj, [child]: value } };
        }
        return s;
      }
      return { ...s, [key]: value };
    });
    pendingSliderUpdates.set(key, { key, value, def });
    if (sliderDebounceTimer) clearTimeout(sliderDebounceTimer);
    sliderDebounceTimer = setTimeout(flushSliderUpdates, 150);
  }

  function handleSliderCommit(def: SettingDef, value: number) {
    if (sliderDebounceTimer) {
      clearTimeout(sliderDebounceTimer);
      sliderDebounceTimer = null;
    }
    pendingSliderUpdates.delete(def.key);
    updateSetting(def.key as keyof AppSettings, value as AppSettings[keyof AppSettings]);
    if ('onSet' in def && def.onSet) def.onSet(value);
  }

  onDestroy(() => {
    if (scrollSaveTimer) clearTimeout(scrollSaveTimer);
    if (sliderDebounceTimer) clearTimeout(sliderDebounceTimer);
    if (pendingSliderUpdates.size > 0) flushSliderUpdates();
    if (sidebarIndicatorRaf !== null) cancelAnimationFrame(sidebarIndicatorRaf);
  });

  function handleSettingChange(def: SettingDef, value: unknown) {
    if ('type' in def && (def.type === 'action' || def.type === 'custom')) return;
    const key = def.key;
    settings.update((s) => {
      if (key.includes('.')) {
        const [parent, child] = key.split('.');
        const parentObj = s[parent as keyof AppSettings];
        if (typeof parentObj === 'object' && parentObj !== null) {
          return { ...s, [parent]: { ...parentObj, [child]: value } };
        }
        return s;
      }
      return { ...s, [key]: value };
    });
    updateSetting(key as keyof AppSettings, value as AppSettings[keyof AppSettings]);
    if ('onSet' in def && def.onSet) def.onSet(value);
  }

  async function pickPath(def: SettingDef) {
    if (!('type' in def) || def.type !== 'path') return;
    try {
      const isFolder = def.pickType === 'folder';
      const result = await open({
        multiple: false,
        directory: isFolder,
        filters: isFolder ? undefined : getFileFilters(def.key),
      });
      if (result) {
        handleSettingChange(def, result as string);
      }
    } catch (e) {
      console.error('Failed to pick path:', e);
    }
  }

  function getFileFilters(key: string) {
    if (key === 'backgroundVideo') {
      return [{ name: 'Video', extensions: ['mp4', 'webm', 'mkv', 'mov', 'avi'] }];
    }
    if (key === 'backgroundImage') {
      return [{ name: 'Image', extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp'] }];
    }
    return undefined;
  }

  function clearHashSection() {
    if (typeof window === 'undefined') return;
    if (window.location.hash) {
      window.history.back();
    } else {
      activeSection = '';
    }
  }

  function titleKeyForSection(sectionId: string): string {
    const sec = SECTIONS.find((s) => s.id === sectionId);
    return sec ? sec.titleKey : 'settings.title';
  }

  function isSubsectionModified(items: SettingDef[]): boolean {
    return items.some((def) => {
      if (def.type === 'action' || def.type === 'custom') return false;
      const current = getSettingValue($settings, def.key);
      const defaultVal = getSettingValue(defaultSettings, def.key);
      return current !== defaultVal;
    });
  }

  function resetSubsection(items: SettingDef[]) {
    for (const def of items) {
      if (def.type === 'action' || def.type === 'custom') continue;
      const defaultVal = getSettingValue(defaultSettings, def.key);
      updateSetting(def.key as any, defaultVal as any);
      if ('onSet' in def && def.onSet) def.onSet(defaultVal);
    }
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>{$t('settings.title')}</h1>
    <p class="subtitle">{$t('settings.subtitle')}</p>
  </div>

  <Divider my={20} />

  <div
    class="settings-layout"
    class:mobile={!onDesktop}
    class:mobile-home={!onDesktop && !activeSection}
    class:mobile-drill={!onDesktop && !!activeSection}
  >
    {#if onDesktop}
      <aside class="settings-sidebar">
        <div class="search-bar" class:collapsed={!searchExpanded && !searchQuery.trim()}>
          <Icon name="search" size={18} />
          <input
            bind:this={searchInputRef}
            type="text"
            placeholder={$t('settings.search.placeholder')}
            bind:value={searchQuery}
            onfocus={() => (searchExpanded = true)}
            onblur={collapseSearchIfEmpty}
          />
          {#if searchQuery.trim()}
            <button
              type="button"
              class="search-clear"
              onclick={(e) => {
                e.stopPropagation();
                searchQuery = '';
                collapseSearchIfEmpty();
              }}
              aria-label={$t('common.clear')}
            >
              <Icon name="cross" size={14} />
            </button>
          {/if}
        </div>

        <div
          class="sidebar-sections"
          bind:this={sidebarSectionsEl}
          onscroll={() => queueSidebarIndicatorUpdate()}
        >
          {#if sidebarIndicatorVisible}
            <div class="sidebar-active-indicator" style={sidebarIndicatorStyle}></div>
          {/if}
          {#each visibleSections as section (section.id)}
            {@const hasMatches =
              !searchQuery.trim() || groupedSettings.some((g) => g.id === section.id)}
            <button
              class="sidebar-item"
              class:active={activeSection === section.id}
              class:dimmed={searchQuery.trim() && !hasMatches}
              onclick={() => handleSectionChange(section.id)}
              use:registerSidebarItem={section.id}
            >
              <Icon name={section.icon as any} size={16} />
              <span class="sidebar-label">{$t(section.titleKey)}</span>
              {#if searchQuery.trim() && hasMatches}
                <span class="sidebar-dot"></span>
              {/if}
            </button>
          {/each}

          <div class="sidebar-build-info">
            <span>v{APP_VERSION}</span>
            <span>{typeof COMMIT_HASH === 'string' ? COMMIT_HASH.slice(0, 7) : ''}</span>
            <span>{BUILD_DATE}</span>
          </div>
        </div>
      </aside>
    {:else if !activeSection}
      <ScrollArea>
        <div class="mobile-sections">
          {#each visibleSections as section (section.id)}
            <button class="mobile-section-btn" onclick={() => handleSectionChange(section.id)}>
              <Icon name={section.icon as any} size={18} />
              <span class="mobile-section-label">{$t(section.titleKey)}</span>
              <Icon name="arrow_right" size={16} />
            </button>
          {/each}
        </div>
      </ScrollArea>
    {:else}
      <div class="mobile-section-topbar">
        <button class="mobile-back-btn" onclick={clearHashSection}>
          <span class="back-icon"><Icon name="arrow_right" size={16} /></span>
        </button>
        <h2 class="mobile-section-title">{$t(titleKeyForSection(activeSection))}</h2>
      </div>
    {/if}

    <div class="settings-pane">
      <ScrollArea bind:this={scrollAreaRef} onscroll={handleScrollChange} {initialScrollTop}>
        <div class="settings-content">
          {#key activeSection}
            <div
              class="settings-content-page"
              in:fly={{ x: 22 * sectionTransitionDir, duration: 220, easing: cubicOut }}
              out:fly={{ x: -18 * sectionTransitionDir, duration: 160, easing: cubicOut }}
            >
              {#if searchQuery}
                <div class="search-results-header">
                  <h2>{$t('settings.searchResults')} "{searchQuery}"</h2>
                </div>
              {/if}

              {#each groupedSettings as group}
                <SettingsBlock
                  title={group.title}
                  icon={group.icon as any}
                  showHeader={!!searchQuery || !onDesktop}
                >
                  {#each group.subsections as subsec}
                    {#if subsec.titleKey}
                      <div class="settings-subsection">
                        <div class="settings-subsection-header">
                          <div class="settings-subsection-title">
                            {subsec.titleKey.includes('.') ? $t(subsec.titleKey) : subsec.titleKey}
                          </div>
                          {#if isSubsectionModified(subsec.items)}
                            <button
                              class="subsection-reset-btn"
                              onclick={() => resetSubsection(subsec.items)}
                              use:tooltip={$t('settings.resetSectionTooltip')}
                            >
                              <Icon name="undo" size={14} />
                              <span class="subsection-reset-text"
                                >{$t('settings.resetSection')}</span
                              >
                            </button>
                          {/if}
                        </div>
                        <div class="settings-subsection-body">
                          {#each subsec.items as def}
                            {@render settingRenderer(def)}
                          {/each}
                        </div>
                      </div>
                    {:else}
                      {#each subsec.items as def}
                        {@render settingRenderer(def)}
                      {/each}
                    {/if}
                  {/each}
                </SettingsBlock>
              {/each}

              {#if groupedSettings.length === 0}
                <div class="no-results">
                  <Icon name="search" size={32} />
                  <p>{$t('settings.noResults')} "{searchQuery}"</p>
                </div>
              {/if}
            </div>
          {/key}
        </div>
      </ScrollArea>
    </div>
  </div>
</div>

{#snippet settingRenderer(def: SettingDef)}
  {@const isDisabled = 'disabled' in def && def.disabled ? def.disabled($settings) : false}
  {#if def.type === 'custom'}
    <div class="setting-wrapper" class:disabled={isDisabled}>
      {#if def.key === 'accent-picker'}
        <AccentPicker {searchQuery} />
      {:else if def.key === 'accent-style'}
        <AccentStyle {searchQuery} />
      {:else if def.key === 'deps-manager'}
        <Dependencies {searchQuery} />
      {:else if def.key === 'data-actions'}
        <DataActions {searchQuery} />
      {:else if def.key === 'proxy-config'}
        <ProxyConfig {searchQuery} />
      {:else if def.key === 'network-check'}
        <NetworkCheck {searchQuery} />
      {:else if def.key === 'app-updates'}
        <AppUpdates {searchQuery} />
      {:else if def.key === 'integration-settings'}
        <ExtensionIntegrationSettings {searchQuery} />
      {/if}
    </div>
  {:else if def.type === 'action'}
    <div class="setting-wrapper" class:disabled={isDisabled}>
      <SettingItem
        title={$t(def.titleKey)}
        description={def.descriptionKey ? $t(def.descriptionKey) : undefined}
        icon={def.icon}
        highlight={searchQuery}
      >
        <button class="action-btn" onclick={() => def.action()} disabled={isDisabled}>
          {#if def.loading && def.loading()}
            <span class="btn-spinner"></span>
          {:else}
            {$t(def.buttonKey)}
          {/if}
        </button>
      </SettingItem>
    </div>
  {:else}
    <div class="setting-wrapper" class:disabled={isDisabled}>
      <SettingItem
        title={$t(def.titleKey)}
        description={def.descriptionKey ? $t(def.descriptionKey) : undefined}
        icon={def.icon}
        highlight={searchQuery}
        value={getSettingValue($settings, def.key)}
        defaultValue={getSettingValue(defaultSettings, def.key)}
        onReset={() => handleSettingChange(def, getSettingValue(defaultSettings, def.key))}
      >
        {#if def.type === 'toggle'}
          <Toggle
            checked={getSettingValue($settings, def.key) as boolean}
            onchange={(v) => handleSettingChange(def, v)}
            disabled={isDisabled}
          />
        {:else if def.type === 'select'}
          <div class="w-200">
            <Select
              options={typeof def.options === 'function' ? def.options(platform) : def.options}
              value={getSettingValue($settings, def.key) as string}
              onchange={(v) => handleSettingChange(def, v)}
              disabled={isDisabled}
            />
          </div>
        {:else if def.type === 'slider'}
          <div class="slider-with-value">
            <input
              class="blur-slider"
              type="range"
              min={def.min}
              max={def.max}
              step={def.step}
              value={getSettingValue($settings, def.key) as number}
              oninput={(e) =>
                handleSliderInput(def, parseFloat((e.currentTarget as HTMLInputElement).value))}
              onchange={(e) =>
                handleSliderCommit(def, parseFloat((e.currentTarget as HTMLInputElement).value))}
              disabled={isDisabled}
            />
            <span class="slider-value">
              {getSettingValue($settings, def.key)}{def.suffix || ''}
            </span>
          </div>
        {:else if def.type === 'input'}
          <div class="input-container" style:width={def.width || '200px'}>
            <Input
              value={getSettingValue($settings, def.key) as string}
              placeholder={def.placeholder}
              oninput={(e) => handleSettingChange(def, (e.currentTarget as HTMLInputElement).value)}
              disabled={isDisabled}
            />
          </div>
        {:else if def.type === 'color'}
          <div class="color-controls">
            <input
              type="color"
              class="color-picker"
              value={getSettingValue($settings, def.key) as string}
              oninput={(e) => handleSettingChange(def, (e.currentTarget as HTMLInputElement).value)}
              disabled={isDisabled}
            />
            <input
              type="text"
              class="color-text-input"
              value={getSettingValue($settings, def.key) as string}
              oninput={(e) => handleSettingChange(def, (e.currentTarget as HTMLInputElement).value)}
              disabled={isDisabled}
            />
          </div>
        {:else if def.type === 'path'}
          <div class="path-controls">
            <Input
              value={getSettingValue($settings, def.key) as string}
              placeholder={def.pickType === 'folder' ? $t('settings.general.browse') : ''}
              oninput={(e) => handleSettingChange(def, (e.currentTarget as HTMLInputElement).value)}
              disabled={isDisabled}
            />
            <button
              class="picker-btn"
              onclick={() => pickPath(def)}
              disabled={isDisabled}
              use:tooltip={$t('settings.general.browse')}
            >
              <Icon name="folder" size={16} />
            </button>
          </div>
        {/if}
      </SettingItem>
    </div>
  {/if}
{/snippet}

<style>
  .w-200 {
    width: 200px;
  }

  .setting-wrapper {
    display: contents;
    transition: opacity 0.2s ease;
  }

  .setting-wrapper.disabled {
    display: flex;
    flex-direction: column;
    gap: 10px;
    opacity: 0.45;
    pointer-events: none;
    user-select: none;
  }

  .page {
    padding: 0 4px 0 var(--page-padding-inline);
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .page-header {
    flex-shrink: 0;
  }

  h1 {
    font-size: 28px;
    font-weight: 700;
    margin-bottom: 6px;
  }

  .subtitle {
    color: rgba(255, 255, 255, 0.6);
    font-size: 14px;
  }

  .settings-layout {
    display: grid;
    grid-template-columns: 220px 1fr;
    gap: 24px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .settings-layout.mobile {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .settings-layout.mobile .settings-pane {
    flex: 1;
    min-height: 0;
  }

  .settings-layout.mobile.mobile-home .settings-pane {
    display: none;
  }

  .settings-sidebar {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow: hidden;
  }

  .sidebar-sections {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding-right: 4px;
  }

  .sidebar-active-indicator {
    position: absolute;
    top: 0;
    left: 0;
    right: 4px;
    border-radius: var(--radius, 10px);
    background: rgba(255, 255, 255, 0.12);
    transition:
      transform 220ms cubic-bezier(0.2, 0.9, 0.2, 1),
      height 220ms cubic-bezier(0.2, 0.9, 0.2, 1),
      opacity 180ms ease;
    will-change: transform, height;
    z-index: 0;
    pointer-events: none;
  }

  .sidebar-item {
    width: 100%;
    position: relative;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius, 10px);
    background: transparent;
    border: 1px solid transparent;
    color: rgba(255, 255, 255, 0.75);
    cursor: pointer;
    text-align: left;
    transition:
      background 120ms ease,
      border-color 120ms ease,
      color 120ms ease,
      opacity 120ms ease;
  }

  .sidebar-item:hover {
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.9);
  }

  .sidebar-item.active {
    color: rgba(255, 255, 255, 1);
  }

  .sidebar-item.dimmed {
    opacity: 0.45;
  }

  .sidebar-label {
    font-size: var(--text-base, 13px);
    font-weight: 450;
    letter-spacing: 0.1px;
  }

  .sidebar-dot {
    margin-left: auto;
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.6);
  }

  .sidebar-build-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    margin-top: auto;
    padding: 12px 4px 16px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    font-weight: 400;
    letter-spacing: 0.2px;
  }

  .search-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 10px);
    flex-shrink: 0;
    overflow: hidden;
    max-height: 44px;
    opacity: 1;
    transform: translateY(0);
    transition:
      max-height 180ms cubic-bezier(0.2, 0.9, 0.2, 1),
      opacity 160ms ease,
      transform 180ms cubic-bezier(0.2, 0.9, 0.2, 1),
      padding 180ms cubic-bezier(0.2, 0.9, 0.2, 1),
      margin 180ms cubic-bezier(0.2, 0.9, 0.2, 1);
  }

  .search-bar.collapsed {
    max-height: 0;
    opacity: 0;
    transform: translateY(-6px);
    padding-top: 0;
    padding-bottom: 0;
    margin-top: -4px;
    pointer-events: none;
  }

  .search-bar :global(svg) {
    color: rgba(255, 255, 255, 0.4);
    flex-shrink: 0;
  }

  .search-bar input {
    flex: 1;
    background: transparent;
    border: none;
    color: white;
    font-size: var(--text-md, 14px);
    outline: none;
  }

  .search-bar input::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  .search-clear {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius, 8px);
    border: none;
    background: transparent;
    color: rgba(255, 255, 255, 0.45);
    cursor: pointer;
    flex-shrink: 0;
  }

  .search-clear:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.85);
  }

  .settings-pane {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .settings-pane :global(.scroll-area-wrapper) {
    flex: 1;
    min-height: 0;
  }

  .settings-content {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 24px;
    margin-top: 0px;
    min-height: 0;
  }

  .settings-content-page {
    display: flex;
    flex-direction: column;
    gap: 24px;
    min-width: 0;
    width: 100%;
  }

  .settings-content > :global(*) {
    position: absolute;
    width: 100%;
    left: 0;
    top: 0;
  }

  .settings-content > :global(*:last-child) {
    position: relative;
  }

  .search-results-header h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 500;
    opacity: 0.9;
  }

  .mobile-sections {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 4px 0 24px;
  }

  .mobile-section-btn {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 18px;
    border-radius: var(--radius-xl, 14px);
    background: rgba(255, 255, 255, 0.05);
    border: none;
    color: rgba(255, 255, 255, 0.9);
    cursor: pointer;
    transition: background 0.15s ease;
  }

  .mobile-section-btn:hover,
  .mobile-section-btn:active {
    background: rgba(255, 255, 255, 0.08);
  }

  .mobile-section-btn :global(svg) {
    color: rgba(255, 255, 255, 0.5);
    flex-shrink: 0;
  }

  .mobile-section-label {
    flex: 1;
    text-align: left;
    font-size: var(--text-lg, 15px);
    font-weight: 500;
  }

  .mobile-section-topbar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
    padding: 0;
  }

  .mobile-back-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.06);
    border: none;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
    transition: background 0.15s ease;
    flex-shrink: 0;
  }

  .mobile-back-btn:active {
    background: rgba(255, 255, 255, 0.1);
  }

  .mobile-section-title {
    flex: 1;
    min-width: 0;
    font-size: var(--text-lg, 16px);
    font-weight: 650;
    color: rgba(255, 255, 255, 0.95);
    text-align: left;
    margin: 0;
    line-height: 1.1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .back-icon {
    display: inline-flex;
    transform: rotate(180deg);
  }

  .settings-subsection {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 0px;
  }

  .settings-subsection + .settings-subsection {
    margin-top: 14px;
  }

  .settings-subsection-header {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: space-between;
    padding: 0px;
    border-radius: 0px;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.75);
    margin-top: 2px;
    margin-bottom: 6px;
  }

  .settings-subsection-title {
    font-size: 17px;
    font-weight: 500;
    letter-spacing: 0.01em;
    text-transform: none;
    flex: 1;
  }

  .subsection-reset-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: var(--radius-sm, 6px);
    background: rgba(255, 255, 255, 0.05);
    border: none;
    color: rgba(255, 255, 255, 0.55);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .subsection-reset-btn:hover {
    background: rgba(255, 255, 255, 0.09);
    color: rgba(255, 255, 255, 0.85);
  }

  @media (max-width: 640px) {
    .subsection-reset-text {
      display: none;
    }
  }

  .settings-subsection-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .slider-with-value {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 180px;
  }

  .blur-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 6px;
    background: rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    outline: none;
    cursor: pointer;
  }

  .blur-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    background: var(--accent, #6366f1);
    border-radius: 50%;
    cursor: pointer;
    transition:
      background 0.15s,
      transform 0.15s;
  }

  .blur-slider::-webkit-slider-thumb:hover {
    background: #818cf8;
    transform: scale(1.1);
  }

  .blur-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    background: var(--accent, #6366f1);
    border: none;
    border-radius: 50%;
    cursor: pointer;
    transition:
      background 0.15s,
      transform 0.15s;
  }

  .blur-slider::-moz-range-thumb:hover {
    background: #818cf8;
    transform: scale(1.1);
  }

  .slider-value {
    font-size: 13px;
    font-family: 'JetBrains Mono', monospace;
    color: rgba(255, 255, 255, 0.7);
    min-width: 40px;
    text-align: right;
  }

  .color-controls {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .color-picker {
    width: 40px;
    height: 32px;
    padding: 2px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-sm, 6px);
    background: rgba(255, 255, 255, 0.05);
    cursor: pointer;
  }

  .color-picker::-webkit-color-swatch-wrapper {
    padding: 2px;
  }

  .color-picker::-webkit-color-swatch {
    border: none;
    border-radius: var(--radius-sm, 4px);
  }

  .color-text-input {
    width: 90px;
    padding: 6px 10px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-sm, 6px);
    color: white;
    outline: none;
    transition: all 0.2s;
  }

  .color-text-input:focus {
    border-color: rgba(99, 102, 241, 0.5);
  }

  .path-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    max-width: 350px;
  }

  .picker-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    padding: 0;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius, 8px);
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.15s;
    flex-shrink: 0;
  }

  .picker-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    color: white;
  }

  .picker-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius, 8px);
    color: white;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.12);
    border-color: rgba(255, 255, 255, 0.15);
  }

  .btn-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 1s linear infinite;
    display: inline-block;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .no-results {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 48px 24px;
    color: rgba(255, 255, 255, 0.4);
    text-align: center;
  }

  .no-results p {
    font-size: 14px;
    margin: 0;
  }
</style>
