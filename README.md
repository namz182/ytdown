# YT_DOWN

<p align="center">
  <img src="public/icons/ytdown.ico" alt="YT_DOWN logo" width="128" />
</p>

YT_DOWN is a desktop YouTube downloader built with Tauri 2, Rust, TypeScript, and Vite. It supports video and playlist downloads, audio extraction, a persistent queue and history, and a searchable media library.

## Requirements

- Node.js and npm
- Rust and the [Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/)
- FFmpeg for MP4 conversion and merging separate video and audio streams
- `libnotify-bin` on Debian or Kali for Linux desktop notifications

yt-dlp installs automatically when you start your first download or preview metadata. You can also manage it in Settings or use an existing executable on your PATH. Install FFmpeg separately for conversion and merging.

## Run in development

```sh
npm install
npm run tauri dev
```

## Build

```sh
npm run tauri build
```

## Features

- Download YouTube videos and playlists at 480p, 720p, 1080p, or best available quality, or extract audio as MP3.
- Review video metadata and playlist options before downloading.
- Track concurrent downloads with progress, speed, ETA, cancellation, retry, and resume controls.
- Browse download history and manage files in the media library.
- Configure download location, quality, concurrency, appearance, and notifications.
- Search YouTube videos from the app and queue a result directly.

Settings, history, and queue state are stored in Tauri's application data directory. Downloads default to `~/Downloads/YT_DOWN`; playlist files are grouped in subfolders.

## Development checks

```sh
npm run build
cargo test -p ytdown --lib
node --test tests/ui-model.test.mjs
```

For the browser-only UI preview, run the Vite development server and open `http://localhost:1420/tests/desktop-preview.html`. Add `?download=running` to preview the compact transfer window.
