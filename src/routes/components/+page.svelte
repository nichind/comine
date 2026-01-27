<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import Button from '$lib/components/Button.svelte';
  import Input from '$lib/components/Input.svelte';
  import Checkbox from '$lib/components/Checkbox.svelte';
  import Toggle from '$lib/components/Toggle.svelte';
  import Select from '$lib/components/Select.svelte';
  import Icon from '$lib/components/Icon.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import { toast } from '$lib/components/Toast.svelte';

  let inputValue = $state('');
  let checkboxValue = $state(false);
  let toggleValue = $state(false);
  let selectValue = $state('');
  let showModal = $state(false);
  let showConfirmModal = $state(false);

  const selectOptions = [
    { value: 'en', label: 'English' },
    { value: 'ru', label: 'Русский' },
    { value: 'es', label: 'Español' },
  ];

  async function testNotification() {
    try {
      await invoke('show_notification_window', {
        data: {
          title: 'Test Notification',
          body: 'This is a test notification with a longer description.',
          thumbnail: null,
          url: 'https://example.com',
        },
        offset: 48,
      });
    } catch (e) {
      console.error('Failed to show notification:', e);
      toast.error('Failed to show notification');
    }
  }

  async function testNotificationWithThumbnail() {
    try {
      await invoke('show_notification_window', {
        data: {
          title: 'Rick Astley - Never Gonna Give You Up',
          body: 'Rick Astley • 3:32',
          thumbnail: 'https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg',
          url: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
        },
        offset: 48,
      });
    } catch (e) {
      console.error('Failed to show notification:', e);
      toast.error('Failed to show notification');
    }
  }
</script>

<div class="page">
  <div class="page-header">
    <h1>Component Showcase</h1>
    <p class="subtitle">All UI components in one place for easy styling</p>
  </div>

  <div class="components-grid">
    <section class="component-section">
      <h2>Buttons</h2>

      <div class="component-group">
        <h3>Variants</h3>
        <div class="showcase">
          <Button variant="primary">Primary Button</Button>
          <Button variant="secondary">Secondary Button</Button>
          <Button variant="ghost">Ghost Button</Button>
        </div>
      </div>

      <div class="component-group">
        <h3>Sizes</h3>
        <div class="showcase">
          <Button size="sm">Small</Button>
          <Button size="md">Medium</Button>
          <Button size="lg">Large</Button>
        </div>
      </div>

      <div class="component-group">
        <h3>With Icons</h3>
        <div class="showcase">
          <Button>
            {#snippet iconLeft()}
              <Icon name="download" size={16} />
            {/snippet}
            Download
          </Button>

          <Button variant="secondary">
            Star on GitHub
            {#snippet iconRight()}
              <Icon name="github" size={16} />
            {/snippet}
          </Button>
        </div>
      </div>

      <div class="component-group">
        <h3>States</h3>
        <div class="showcase">
          <Button>Normal</Button>
          <Button disabled>Disabled</Button>
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>Inputs</h2>

      <div class="component-group">
        <h3>Basic</h3>
        <div class="showcase">
          <Input bind:value={inputValue} placeholder="Enter text..." />
          <Input type="password" placeholder="Password..." />
          <Input type="email" placeholder="Email..." />
        </div>
      </div>

      <div class="component-group">
        <h3>With Icons</h3>
        <div class="showcase">
          <Input placeholder="Search...">
            {#snippet iconLeft()}
              <Icon name="search" size={16} />
            {/snippet}
          </Input>

          <Input placeholder="Download path...">
            {#snippet iconRight()}
              <Icon name="folder" size={16} />
            {/snippet}
          </Input>
        </div>
      </div>

      <div class="component-group">
        <h3>States</h3>
        <div class="showcase">
          <Input placeholder="Normal" />
          <Input placeholder="Disabled" disabled />
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>OS Notifications</h2>

      <div class="component-group">
        <h3>External Notification Windows</h3>
        <p style="color: rgba(255,255,255,0.6); margin-bottom: 12px; font-size: 13px;">
          These notifications appear outside the app window, like Telegram/Steam notifications.
        </p>
        <div class="showcase">
          <Button onclick={testNotification}>
            {#snippet iconLeft()}
              <Icon name="info" size={16} />
            {/snippet}
            Test Simple Notification
          </Button>
          <Button onclick={testNotificationWithThumbnail}>
            {#snippet iconLeft()}
              <Icon name="play" size={16} />
            {/snippet}
            Test with Thumbnail
          </Button>
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>Checkboxes</h2>

      <div class="component-group">
        <div class="showcase vertical">
          <Checkbox bind:checked={checkboxValue} label="Checkbox with label" />
          <Checkbox label="Another checkbox" />
          <Checkbox label="Disabled checkbox" disabled />
          <Checkbox checked label="Checked & disabled" disabled />
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>Toggles</h2>

      <div class="component-group">
        <div class="showcase vertical">
          <Toggle bind:checked={toggleValue} label="Enable feature" />
          <Toggle label="Another toggle" />
          <Toggle label="Disabled toggle" disabled />
          <Toggle checked label="Checked & disabled" disabled />
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>Select</h2>

      <div class="component-group">
        <div class="showcase">
          <Select
            bind:value={selectValue}
            options={selectOptions}
            placeholder="Choose language..."
          />
          <Select options={selectOptions} placeholder="Disabled" disabled />
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>Modals</h2>

      <div class="component-group">
        <h3>Basic Modal</h3>
        <div class="showcase">
          <Button onclick={() => (showModal = true)}>Open Modal</Button>
          <Button variant="secondary" onclick={() => (showConfirmModal = true)}
            >Confirm Dialog</Button
          >
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>Toasts</h2>

      <div class="component-group">
        <h3>Toast Types</h3>
        <div class="showcase">
          <Button variant="ghost" onclick={() => toast.success('Download completed successfully!')}
            >Success</Button
          >
          <Button variant="ghost" onclick={() => toast.error('Failed to connect to server')}
            >Error</Button
          >
          <Button variant="ghost" onclick={() => toast.warning('Storage space is running low')}
            >Warning</Button
          >
          <Button variant="ghost" onclick={() => toast.info('New version available')}>Info</Button>
        </div>
      </div>

      <div class="component-group">
        <h3>Custom Duration</h3>
        <div class="showcase">
          <Button
            variant="secondary"
            onclick={() => toast('This will disappear in 2 seconds', 'info', 2000)}
            >Short (2s)</Button
          >
          <Button
            variant="secondary"
            onclick={() => toast('This will stay for 8 seconds', 'info', 8000)}>Long (8s)</Button
          >
        </div>
      </div>
    </section>

    <section class="component-section">
      <h2>Icons</h2>

      <div class="component-group">
        <h3>Sizes</h3>
        <div class="showcase">
          <Icon name="download" size={16} />
          <Icon name="download" size={24} />
          <Icon name="download" size={32} />
        </div>
      </div>

      <div class="component-group">
        <h3>Common Icons</h3>
        <div class="icon-grid">
          <div class="icon-item">
            <Icon name="download" size={24} />
            <span>download</span>
          </div>
          <div class="icon-item">
            <Icon name="history" size={24} />
            <span>history</span>
          </div>
          <div class="icon-item">
            <Icon name="queue" size={24} />
            <span>queue</span>
          </div>
          <div class="icon-item">
            <Icon name="settings" size={24} />
            <span>settings</span>
          </div>
          <div class="icon-item">
            <Icon name="info" size={24} />
            <span>info</span>
          </div>
          <div class="icon-item">
            <Icon name="logs" size={24} />
            <span>logs</span>
          </div>
          <div class="icon-item">
            <Icon name="github" size={24} />
            <span>github</span>
          </div>
          <div class="icon-item">
            <Icon name="discord" size={24} />
            <span>discord</span>
          </div>
          <div class="icon-item">
            <Icon name="search" size={24} />
            <span>search</span>
          </div>
          <div class="icon-item">
            <Icon name="folder" size={24} />
            <span>folder</span>
          </div>
        </div>
      </div>
    </section>
  </div>
</div>

<Modal bind:open={showModal} title="Basic Modal">
  <p style="color: rgba(255,255,255,0.8); line-height: 1.6;">
    This is a basic modal dialog. You can put any content here - forms, information, images, etc.
  </p>
  <p style="color: rgba(255,255,255,0.6); margin-top: 12px; font-size: 13px;">
    Press Escape or click the backdrop to close.
  </p>
</Modal>

<Modal bind:open={showConfirmModal} title="Confirm Action">
  <p style="color: rgba(255,255,255,0.8); line-height: 1.6;">
    Are you sure you want to delete this item? This action cannot be undone.
  </p>
  {#snippet actions()}
    <Button variant="ghost" onclick={() => (showConfirmModal = false)}>Cancel</Button>
    <Button
      variant="primary"
      onclick={() => {
        showConfirmModal = false;
        toast.success('Item deleted!');
      }}>Delete</Button
    >
  {/snippet}
</Modal>

<style>
  .page {
    padding: 24px;
  }

  .page-header {
    margin-bottom: 40px;
  }

  h1 {
    font-size: 28px;
    font-weight: 700;
    margin-bottom: 8px;
  }

  .subtitle {
    color: rgba(255, 255, 255, 0.6);
    font-size: 16px;
  }

  .components-grid {
    display: flex;
    flex-direction: column;
    gap: 48px;
  }

  .component-section {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--radius-lg, 12px);
    padding: 24px;
  }

  .component-section > h2 {
    font-size: 20px;
    font-weight: 600;
    margin-bottom: 24px;
    color: rgba(255, 255, 255, 0.9);
  }

  .component-group {
    margin-bottom: 24px;
  }

  .component-group:last-child {
    margin-bottom: 0;
  }

  .component-group h3 {
    font-size: 14px;
    font-weight: 500;
    margin-bottom: 12px;
    color: rgba(255, 255, 255, 0.7);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .showcase {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    align-items: center;
  }

  .showcase.vertical {
    flex-direction: column;
    align-items: flex-start;
  }

  .icon-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: 16px;
  }

  .icon-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 16px;
    background: rgba(255, 255, 255, 0.05);
    border-radius: var(--radius, 8px);
    transition: background 0.2s;
  }

  .icon-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .icon-item span {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.6);
  }
</style>
