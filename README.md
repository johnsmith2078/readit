# Readit MVP

Windows-first Tauri 2 MVP for hovering over accessible text, drawing a translucent bordered overlay directly over the text paragraph, and reading the text aloud with `edge-tts`.

## Features

- Tauri 2 + React + TypeScript desktop app.
- Windows UI Automation text capture under the current mouse cursor after a short hover.
- Translucent always-on-top bordered overlay positioned directly over the detected text paragraph.
- Clicking the paragraph overlay reads the hovered text; playback shows a semi-transparent playing state inside the same border.
- Fallback global shortcut: `Ctrl+Alt+R`; if it is already registered by another app, Readit still starts and hover overlay remains available.
- Manual capture/read buttons in the app window.
- `edge-tts` speech synthesis through the local Python environment.
- MP3 playback through Rust `rodio`.

## Prerequisites

- Windows 10/11.
- Node.js and npm.
- Rust stable toolchain.
- Python 3.10+.
- `edge-tts` installed in the Python environment used by `python`:

```powershell
python -m pip install edge-tts
```

## Development

```powershell
npm install
npm run tauri -- dev
```

After the app starts, hover over readable text in another Windows app, such as a browser page or Notepad. When the translucent border appears over the paragraph itself, click that covered paragraph area to read the text. Readit ignores its own windows so the settings UI remains usable.

## Build

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri -- build --debug
```

Debug app output:

- `src-tauri/target/debug/readit.exe`

Debug installers:

- `src-tauri/target/debug/bundle/msi/Readit_0.1.0_x64_en-US.msi`
- `src-tauri/target/debug/bundle/nsis/Readit_0.1.0_x64-setup.exe`

## MVP Limitations

- Text capture only works when the target app exposes text through Windows UI Automation `TextPattern`.
- Browser text, some PDF text layers, and native text controls may work; image-only PDFs, Canvas, games, remote desktops, and custom-rendered controls may not.
- Current paragraph detection is a simple best-effort heuristic using accessible document text.
- Hover detection is best-effort and uses UI Automation text exposure; unsupported targets are ignored silently.
- `edge-tts` uses an online speech service; avoid reading sensitive content.
- This MVP calls `python -c ...` directly instead of shipping a bundled sidecar.

## Important Files

- Frontend: `src/main.tsx`
- Styles: `src/styles.css`
- Tauri config: `src-tauri/tauri.conf.json`
- Rust backend: `src-tauri/src/lib.rs`
- Feasibility plan: `TAURI2_READIT_FEASIBILITY_PLAN.md`
