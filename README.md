# comine <img src="static\icon.png" align="right" height="90" alt="Comine logo (icon)" />

comine is a cross-platform media, file, torrent, gallery, & more downloader/archiver. Available on Windows, Linux, MacOS and Android.

<img width="750" align="center" alt="App preview" src="https://github.com/user-attachments/assets/d0c95796-99f3-4f98-9ab8-d53c6fde2019" />

## Tech Stack

- **Frontend:** Svelte 5 + SvelteKit
- **Backend:** Tauri v2 + Rust
- **Download Engine:** yt-dlp, aria2c, ffmpeg

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) (v20+)
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/)
- [Tauri Prerequisites](https://tauri.app/v1/guides/getting-started/prerequisites)

### Setup

```bash
pnpm install
pnpm tauri dev
```

### Build

```bash
# Desktop
pnpm tauri build

# Android
pnpm tauri android build
```

## License

GPL-3.0

#### Consider leaving a star ⭐

[![Star History Chart](https://api.star-history.com/svg?repos=nichind/comine&type=Date)](https://github.com/nichind/comine)
