<script lang="ts">
  import { t } from '$lib/i18n';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Icon from '$lib/components/ui/Icon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import PageShell from '$lib/components/layout/PageShell.svelte';

  const APP_VERSION = __APP_VERSION__;
  const GIT_BRANCH = __GIT_BRANCH__;
  const COMMIT_HASH = __COMMIT_HASH__;
  const BUILD_DATE = __BUILD_DATE__;

  let versionCopied = $state(false);

  async function copyVersion() {
    try {
      const hash = typeof COMMIT_HASH === 'string' ? COMMIT_HASH.slice(0, 7) : 'unknown';
      const info = `comine v${APP_VERSION} (${hash}) [${GIT_BRANCH}]`;
      await navigator.clipboard.writeText(info);
      versionCopied = true;
      setTimeout(() => (versionCopied = false), 2000);
    } catch (err) {
      console.error('Failed to copy version:', err);
    }
  }

  async function openLink(url: string) {
    try {
      await openUrl(url);
    } catch (err) {
      console.error('Failed to open URL:', err);
      window.open(url, '_blank');
    }
  }
</script>

<PageShell
  title={$t('info.title')}
  subtitle={$t('info.subtitle')}
  scrollMode="scroll-area"
  preserveScrollKey="info"
>
  <div class="info-content">
    <section class="info-section">
      <div class="setting-item">
        <span class="setting-label">{$t('info.version')}</span>
        <button class="version-btn" onclick={copyVersion} use:tooltip={$t('info.clickToCopy')}>
          <span>v{APP_VERSION}</span>
          <Icon name={versionCopied ? 'check' : 'copy'} size={14} />
        </button>
      </div>
      <div class="setting-item">
        <span class="setting-label">{$t('info.branch')}</span>
        <span class="setting-value mono">{GIT_BRANCH}</span>
      </div>
      <div class="setting-item">
        <span class="setting-label">{$t('info.commit')}</span>
        <span class="setting-value mono"
          >{typeof COMMIT_HASH === 'string' ? COMMIT_HASH.slice(0, 7) : 'unknown'}</span
        >
      </div>
      <div class="setting-item">
        <span class="setting-label">{$t('info.buildDate')}</span>
        <span class="setting-value mono">{BUILD_DATE}</span>
      </div>
      <p class="setting-description">{$t('app.description')}</p>
    </section>

    <section class="info-section">
      <h2 class="section-title">{$t('info.links')}</h2>
      <button class="setting-item clickable" onclick={() => openLink('https://comine.app')}>
        <span class="setting-label">{$t('info.website')}</span>
        <span class="setting-value link">comine.app</span>
      </button>
      <button
        class="setting-item clickable"
        onclick={() => openLink('https://github.com/nichind/comine')}
      >
        <span class="setting-label">GitHub</span>
        <span class="setting-value link">nichind/comine</span>
      </button>
      <button
        class="setting-item clickable"
        onclick={() => openLink('https://github.com/nichind/comine-extension')}
      >
        <span class="setting-label">{$t('info.browserExtension')}</span>
        <span class="setting-value link">{$t('info.getExtension')}</span>
      </button>
      <button
        class="setting-item clickable"
        onclick={() => openLink('https://discord.gg/8sfk33Kr2A')}
      >
        <span class="setting-label">Discord</span>
        <span class="setting-value link">{$t('info.joinCommunity')}</span>
      </button>
      <button class="setting-item clickable" onclick={() => openLink('https://t.me/comineapp')}>
        <span class="setting-label">Telegram</span>
        <span class="setting-value link">{$t('info.joinCommunity')}</span>
      </button>
    </section>

    <section class="info-section">
      <h2 class="section-title">{$t('info.developer')}</h2>
      <div class="dev-row">
        <button class="dev-icon" onclick={() => openLink('https://nichind.dev')}>
          <img src="/nichind.svg" alt="nichind" />
        </button>
        <div class="dev-info">
          <button class="dev-name" onclick={() => openLink('https://nichind.dev')}>nichind</button>
          <span class="dev-role">{$t('info.madeWith')}</span>
        </div>
        <div class="dev-links">
          <button
            class="icon-btn"
            onclick={() => openLink('https://nichind.dev')}
            use:tooltip={'nichind.dev'}
          >
            <Icon name="globe" size={16} />
          </button>
          <button
            class="icon-btn"
            onclick={() => openLink('https://github.com/nichind')}
            use:tooltip={'GitHub'}
          >
            <Icon name="github" size={16} />
          </button>
        </div>
      </div>
    </section>

    <section class="info-section">
      <h2 class="section-title">{$t('info.legal')}</h2>
      <button
        class="setting-item clickable"
        onclick={() => openLink('https://github.com/nichind/comine/blob/main/LICENSE')}
      >
        <span class="setting-label">{$t('info.license')}</span>
        <span class="setting-value link">GPL-3.0</span>
      </button>
    </section>
  </div>
</PageShell>

<style>
  .info-content {
    display: flex;
    flex-direction: column;
    gap: 32px;
  }

  .info-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .section-title {
    font-size: 17px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
    margin-bottom: 4px;
  }

  .setting-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-left: 4px;
    width: 100%;
    background: none;
    border: none;
    text-align: left;
  }

  button.setting-item.clickable {
    cursor: pointer;
    border-radius: var(--radius-sm, 6px);
    margin-left: -8px;
    padding: 8px;
    padding-right: 12px;
    transition: background 0.15s;
    width: calc(100% + 8px);
  }

  button.setting-item.clickable:hover {
    background: linear-gradient(to right, transparent, rgba(255, 255, 255, 0.06) 50%);
  }

  .setting-label {
    font-size: var(--text-md, 14px);
    color: rgba(255, 255, 255, 0.85);
  }

  .setting-value {
    font-size: var(--text-md, 14px);
    color: rgba(255, 255, 255, 0.5);
  }

  .setting-value.mono {
    font-family: 'JetBrains Mono', monospace;
    font-size: var(--text-base, 13px);
  }

  .setting-value.link {
    color: var(--accent, #6366f1);
  }

  .setting-description {
    font-size: var(--text-base, 13px);
    color: rgba(255, 255, 255, 0.5);
    padding-left: 4px;
    margin: 4px 0 0 0;
  }

  .version-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: var(--radius-sm, 4px);
    color: rgba(255, 255, 255, 0.7);
    font-size: var(--text-base, 13px);
    font-family: 'JetBrains Mono', monospace;
    cursor: pointer;
    transition: all 0.15s;
  }

  .version-btn:hover {
    border-color: rgba(255, 255, 255, 0.3);
    color: white;
  }

  .version-btn :global(svg) {
    opacity: 0.5;
  }

  .dev-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-left: 4px;
  }

  .dev-icon {
    width: 32px;
    height: 32px;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    transition: transform 0.15s;
  }

  .dev-icon:hover {
    transform: scale(1.1);
  }

  .dev-icon img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .dev-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .dev-name {
    font-size: 14px;
    font-weight: 500;
    color: var(--accent, #6366f1);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-align: left;
  }

  .dev-name:hover {
    text-decoration: underline;
  }

  .dev-role {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .dev-links {
    display: flex;
    gap: 4px;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm, 6px);
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    transition: all 0.15s;
  }

  .icon-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: white;
  }
</style>
