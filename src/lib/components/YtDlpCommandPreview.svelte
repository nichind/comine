<script lang="ts">
  import { t } from '$lib/i18n';
  import Icon from './Icon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toast } from './Toast.svelte';
  import { defaultYtDlpAdvanced, type YtDlpAdvancedSettings } from '$lib/stores/settings';

  interface Props {
    mode: 'extraction' | 'download';
    url?: string;
    playerClient?: string;
    cookiesFromBrowser?: string;
    useAria2?: boolean;
    advancedSettings?: YtDlpAdvancedSettings;
    videoQuality?: string;
    downloadMode?: string;
    audioQuality?: string;
    sponsorBlock?: boolean;
    sponsorBlockSkipSponsors?: boolean;
    sponsorBlockSkipIntros?: boolean;
    sponsorBlockSkipSelfPromo?: boolean;
    sponsorBlockSkipInteraction?: boolean;
    chapters?: boolean;
    embedSubtitles?: boolean;
    subtitleLanguages?: string;
    embedThumbnail?: boolean;
    remux?: boolean;
    convertToMp4?: boolean;
    clearMetadata?: boolean;
    downloadSpeedLimit?: number;
  }

  let {
    mode,
    url = 'https://youtube.com/watch?v=...',
    playerClient = 'android_sdkless',
    cookiesFromBrowser = '',
    useAria2 = false,
    advancedSettings,
    videoQuality = 'max',
    downloadMode = 'auto',
    audioQuality = 'best',
    sponsorBlock = false,
    sponsorBlockSkipSponsors = true,
    sponsorBlockSkipIntros = false,
    sponsorBlockSkipSelfPromo = false,
    sponsorBlockSkipInteraction = false,
    chapters = true,
    embedSubtitles = false,
    subtitleLanguages = 'en,ru',
    embedThumbnail = true,
    remux = true,
    convertToMp4 = false,
    clearMetadata = false,
    downloadSpeedLimit = 0,
  }: Props = $props();

  // Use defaults if advancedSettings is not provided
  let settings = $derived(advancedSettings ?? defaultYtDlpAdvanced);

  let copied = $state(false);

  function buildExtractionCommand(): string[] {
    const args: string[] = ['yt-dlp'];

    // Encoding
    args.push('--encoding', 'utf-8');

    // Extraction type args
    if (mode === 'extraction') {
      args.push('--dump-json');
      args.push('--no-download');
      
      if (settings.extractionFlatPlaylist) {
        args.push('--flat-playlist');
      }
      if (settings.extractionNoPlaylist) {
        args.push('--no-playlist');
      }
    }

    // Cookies
    if (cookiesFromBrowser && cookiesFromBrowser !== 'custom') {
      args.push('--cookies-from-browser', cookiesFromBrowser);
    } else if (cookiesFromBrowser === 'custom') {
      args.push('--cookies', '<custom_cookies.txt>');
    }

    // YouTube extractor args
    const isYouTube = url.includes('youtube.com') || url.includes('youtu.be');
    if (isYouTube && playerClient) {
      const skipParts: string[] = [];
      if (settings.extractionPlayerSkipWebpage) skipParts.push('webpage');
      if (settings.extractionPlayerSkipConfigs) skipParts.push('configs');
      
      let extractorArg = `youtube:player_client=${playerClient}`;
      if (skipParts.length > 0) {
        extractorArg += `;player_skip=${skipParts.join(',')}`;
      }
      args.push('--extractor-args', extractorArg);
    }

    // Custom extraction args
    if ((settings.extractionCustomArgs ?? '').trim()) {
      args.push(...(settings.extractionCustomArgs ?? '').trim().split(/\s+/));
    }

    args.push(url);
    return args;
  }

  function buildDownloadCommand(): string[] {
    const args: string[] = ['yt-dlp'];

    // Encoding
    args.push('--encoding', 'utf-8');

    // Output template
    const template = settings.outputTemplate || '%(title)s.%(ext)s';
    args.push('-o', `<download_path>/${template}`);

    // Progress
    args.push('--newline', '--progress');

    // Format selection
    let formatStr = '';
    if (downloadMode === 'audio') {
      formatStr = audioQuality === 'best' ? 'bestaudio/best' : `bestaudio[abr<=${audioQuality}]/bestaudio/best`;
    } else if (downloadMode === 'mute') {
      formatStr = videoQuality === 'max' ? 'bestvideo/best' : `bestvideo[height<=${videoQuality.replace('p', '')}]/bestvideo/best`;
    } else {
      formatStr = videoQuality === 'max' ? 'bestvideo+bestaudio/best' : `bestvideo[height<=${videoQuality.replace('p', '')}]+bestaudio/best`;
    }
    args.push('-f', formatStr);

    // Audio extraction
    if (downloadMode === 'audio') {
      args.push('-x', '--audio-format', 'm4a');
      if (embedThumbnail) {
        args.push('--embed-thumbnail');
      }
    }

    // Video post-processing
    if (downloadMode !== 'audio') {
      if (convertToMp4) {
        args.push('--format-sort', 'vcodec:h264,acodec:aac');
        args.push('--recode-video', 'mp4');
      } else if (remux) {
        args.push('--remux-video', 'mp4');
      }
    }

    // Metadata
    if (clearMetadata) {
      args.push('--no-embed-metadata');
    }

    // Cookies
    if (cookiesFromBrowser && cookiesFromBrowser !== 'custom') {
      args.push('--cookies-from-browser', cookiesFromBrowser);
    } else if (cookiesFromBrowser === 'custom') {
      args.push('--cookies', '<custom_cookies.txt>');
    }

    // No playlist
    if (settings.downloadNoPlaylist) {
      args.push('--no-playlist');
    }

    // YouTube extractor args
    const isYouTube = url.includes('youtube.com') || url.includes('youtu.be');
    if (isYouTube && playerClient) {
      const skipParts: string[] = [];
      if (settings.extractionPlayerSkipWebpage) skipParts.push('webpage');
      if (settings.extractionPlayerSkipConfigs) skipParts.push('configs');
      
      let extractorArg = `youtube:player_client=${playerClient}`;
      if (skipParts.length > 0) {
        extractorArg += `;player_skip=${skipParts.join(',')}`;
      }
      args.push('--extractor-args', extractorArg);
    }

    // SponsorBlock
    if (sponsorBlock) {
      const categories: string[] = [];
      if (sponsorBlockSkipSponsors) categories.push('sponsor');
      if (sponsorBlockSkipIntros) categories.push('intro', 'outro');
      if (sponsorBlockSkipSelfPromo) categories.push('selfpromo');
      if (sponsorBlockSkipInteraction) categories.push('interaction');
      
      if (categories.length > 0) {
        args.push('--sponsorblock-remove', categories.join(','));
      }
    }

    // Chapters
    if (chapters) {
      args.push('--embed-chapters');
    }

    // Subtitles
    if (embedSubtitles) {
      args.push('--embed-subs', '--sub-langs', subtitleLanguages);
    }

    // Speed limit
    if (downloadSpeedLimit > 0) {
      args.push('--limit-rate', `${downloadSpeedLimit}M`);
    }

    // Concurrent fragments
    if ((settings.downloadConcurrentFragments ?? 1) > 1) {
      args.push('--concurrent-fragments', String(settings.downloadConcurrentFragments ?? 1));
    }

    // Retries
    if ((settings.downloadRetries ?? 10) !== 10) {
      args.push('--retries', String(settings.downloadRetries ?? 10));
    }
    if ((settings.downloadFragmentRetries ?? 10) !== 10) {
      args.push('--fragment-retries', String(settings.downloadFragmentRetries ?? 10));
    }

    // aria2
    if (useAria2) {
      args.push('--downloader', '<aria2c_path>');
      
      const connections = settings.aria2OverrideGlobal 
        ? (settings.aria2YtdlpConnections ?? 8)
        : 8;
      const splits = settings.aria2OverrideGlobal 
        ? (settings.aria2YtdlpSplits ?? 8)
        : 8;
      const minSplit = settings.aria2OverrideGlobal 
        ? (settings.aria2YtdlpMinSplitSize ?? '1M')
        : '1M';
      const disableIPv6 = settings.aria2OverrideGlobal 
        ? (settings.aria2YtdlpDisableIPv6 ?? true)
        : true;
      
      let aria2Args = `aria2c:-x ${connections} -s ${splits} -k ${minSplit} --file-allocation=none`;
      if (disableIPv6) {
        aria2Args += ' --disable-ipv6=true';
      }
      
      const customAria2Args = settings.aria2OverrideGlobal 
        ? (settings.aria2YtdlpCustomArgs ?? '')
        : '';
      if (customAria2Args.trim()) {
        aria2Args += ' ' + customAria2Args.trim();
      }
      
      args.push('--downloader-args', aria2Args);
    }

    // Output filename settings
    if (settings.outputRestrictFilenames) {
      args.push('--restrict-filenames');
    }
    if (settings.outputWindowsFilenames) {
      args.push('--windows-filenames');
    }

    // Post-process keep original
    if (settings.postProcessKeepOriginal) {
      args.push('--keep-video');
    }

    // Embed info.json
    if (settings.postProcessEmbedInfoJson) {
      args.push('--embed-info-json');
    }

    // Custom download args
    if ((settings.downloadCustomArgs ?? '').trim()) {
      args.push(...(settings.downloadCustomArgs ?? '').trim().split(/\s+/));
    }

    // Custom post-process args
    if ((settings.postProcessCustomArgs ?? '').trim()) {
      args.push(...(settings.postProcessCustomArgs ?? '').trim().split(/\s+/));
    }

    args.push(url);
    return args;
  }

  let commandArgs = $derived(mode === 'extraction' ? buildExtractionCommand() : buildDownloadCommand());
  let commandString = $derived(commandArgs.map(arg => arg.includes(' ') ? `"${arg}"` : arg).join(' '));

  async function copyCommand() {
    try {
      try {
        await navigator.clipboard.writeText(commandString);
      } catch {
        const { writeText } = await import('@tauri-apps/plugin-clipboard-manager');
        await writeText(commandString);
      }

      copied = true;
      toast.success($t('common.copied'));
      setTimeout(() => (copied = false), 2000);
    } catch {
      toast.error($t('common.error') || 'Failed to copy');
    }
  }
</script>

<div class="command-preview">
  <Icon name="code" size={14} />
  <code class="command-line">{commandString}</code>
  <button class="copy-btn" onclick={copyCommand} use:tooltip={$t('common.copy')}>
    <Icon name={copied ? 'check' : 'clipboard'} size={12} />
  </button>
</div>

<style>
  .command-preview {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: rgba(0, 0, 0, 0.2);
    border-radius: 6px;
    margin-top: 12px;
    color: rgba(255, 255, 255, 0.5);
  }

  .command-line {
    flex: 1;
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.6);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .copy-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: 4px;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    flex-shrink: 0;
  }

  .copy-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.9);
  }
</style>
