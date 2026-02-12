export interface FormatOption {
  format: string;
  label: string;
  audioOnly: boolean;
}

const VIDEO_EXTENSIONS = ['mp4', 'webm', 'mkv', 'avi', 'mov', 'flv', 'wmv', 'm4v'];
const AUDIO_EXTENSIONS = ['mp3', 'm4a', 'aac', 'opus', 'ogg', 'flac', 'wav', 'wma'];

const VIDEO_CONVERSION_FORMATS: FormatOption[] = [
  { format: 'mp4', label: 'MP4', audioOnly: false },
  { format: 'webm', label: 'WebM', audioOnly: false },
  { format: 'mkv', label: 'MKV', audioOnly: false },
  { format: 'gif', label: 'GIF', audioOnly: false },
  { format: 'mp3', label: 'MP3 (Audio)', audioOnly: true },
  { format: 'flac', label: 'FLAC (Audio)', audioOnly: true },
];

const AUDIO_CONVERSION_FORMATS: FormatOption[] = [
  { format: 'mp3', label: 'MP3', audioOnly: true },
  { format: 'm4a', label: 'M4A', audioOnly: true },
  { format: 'flac', label: 'FLAC', audioOnly: true },
  { format: 'wav', label: 'WAV', audioOnly: true },
];

export function getConversionFormats(extension: string): FormatOption[] {
  const sourceExt = (extension || '').toLowerCase();

  const isVideo = VIDEO_EXTENSIONS.includes(sourceExt);
  const isAudio = AUDIO_EXTENSIONS.includes(sourceExt);

  if (isVideo) {
    return VIDEO_CONVERSION_FORMATS.filter((f) => f.format !== sourceExt);
  } else if (isAudio) {
    return AUDIO_CONVERSION_FORMATS.filter((f) => f.format !== sourceExt);
  }

  return [];
}
