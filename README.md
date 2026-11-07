<div align="center">

<img src="resources/AppIcon.svg" width="112" alt="">

# That's it

**Drag & drop file converter for macOS**

*Drop it. Pick it. That's it.*

[![CI](https://github.com/federicosimoni/thats-it/actions/workflows/ci.yml/badge.svg)](https://github.com/federicosimoni/thats-it/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
![macOS 13+](https://img.shields.io/badge/macOS-13%2B-black)

English · [Italiano](README.it.md)

<!-- Demo GIF, enabled once docs/assets/demo.gif is added:
<img src="docs/assets/demo.gif" width="560" alt="Dragging a HEIC photo onto the widget, the ring opens and the photo is converted to JPG">
-->

</div>

A small glass disc lives on your desktop. Drag files onto it and it opens into a ring of the
conversions that make sense for those files. Move onto one, let go, and the result appears
next to the original. No windows, no dialogs, no uploads: everything runs on your Mac.

## Features

- **Only what fits.** The ring shows just the conversions valid for the files you drag.
  Several files at once? You get the conversions valid for *all* of them, run as a batch.
- **Native.** Written in Rust on top of AppKit and Core Animation, with real macOS glass,
  SF Symbols, light and dark mode. No Electron, no web view, no Dock icon.
- **Safe with your files.** Results are written next to the original as `photo.jpg`,
  `photo (1).jpg`… The original is never moved, overwritten or deleted.
- **Private.** Nothing leaves your Mac. Conversions use built-in macOS tools (`sips`,
  `afconvert`, `textutil`, PDFKit…) and Rust libraries.
- **Extensible.** Every conversion is a small TOML file. Add your own without writing
  any Rust: see [docs/conversions.md](docs/conversions.md).
- **Localized.** English and Italian, more welcome.

## What it converts

| Category | Conversions |
|---|---|
| **Images** | PNG, JPG, TIFF, BMP, GIF, WebP (lossless) in any direction · HEIC → JPG/PNG · SVG → PNG · PNG → app icon (ICNS) · resize 50% · resize to max 1920 px · images → PDF |
| **Audio & video** | WAV, AIFF, M4A (AAC), CAF · → MP3¹ · MOV → MP4¹ · compress video¹ · video → GIF¹ · extract audio¹ |
| **Documents** | DOCX, DOC, RTF, ODT, HTML, TXT in any direction · those → PDF · Markdown → HTML/PDF/DOCX · PDF pages → PNG |
| **Utilities** | compress to ZIP · remove photo metadata (EXIF/GPS) without re-encoding · optimize PNG losslessly |

¹ Needs [ffmpeg](https://ffmpeg.org): `brew install ffmpeg`. Without it these conversions
are simply hidden.

## Install

**Homebrew**

```sh
brew install --cask federicosimoni/tap/thats-it
```

**Manually:** download the `.dmg` from [Releases](https://github.com/federicosimoni/thats-it/releases),
open it and drag *That's it* to Applications.

Requires macOS 13 Ventura or later, Apple silicon or Intel.

> [!NOTE]
> That's it is not notarized by Apple yet, so macOS blocks the first launch. Open
> **System Settings → Privacy & Security** and click **Open Anyway**, or run
> `xattr -dr com.apple.quarantine "/Applications/That's it.app"`.

## How to use it

1. Drag one or more files from the Finder onto the disc on your desktop.
2. The ring opens. Move towards a conversion; it lights up.
3. Let go. A progress ring runs around the disc and a notification tells you when it is done.

To cancel, let go in the middle of the ring or drag away. When there are more than eight
conversions, the ring shows categories first: rest on one to open it, rest on the centre to
go back.

**Moving the widget:** hold **Option** and drag the disc. It snaps to one of five places
(top left, top centre, top right, bottom right, bottom left), always far enough from the
edges for the open ring to fit. **Option-click** jumps to the next place.

**Menu bar:** the tray icon in the menu bar opens Preferences (screen, widget size, open
at login, accent color, which conversions appear, detected tools), hides the widget,
cancels a running conversion and opens the folder for your own conversions.

The widget sits on the desktop, under your windows, so it is hidden behind full-screen
apps.

## Add your own conversions

Drop a `.toml` file into
`~/Library/Application Support/io.github.federicosimoni.thatsit/conversions/`
(**menu bar → Open Conversions Folder**):

```toml
schema     = 1
id         = "image.to-avif"
category   = "images"
icon       = "photo"
accepts    = ["public.jpeg", "public.png"]
output_ext = "avif"
requires   = ["avifenc"]

[label]
en = "Convert to AVIF"
it = "Converti in AVIF"

[[steps]]
cmd = ["avifenc", "{input}", "{output}"]
```

Commands run directly, never through a shell. The full reference is in
[docs/conversions.md](docs/conversions.md).

## Contributing

New conversions, translations, bug reports and fixes are all welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md). Translations are done in [Weblate](docs/translating.md)
or directly in `locales/`.

## Building from source

```sh
cargo run -p thatsit-app            # run the widget from the terminal
cargo xtask bundle --native         # build target/bundle/That's it.app
```

See [docs/releasing.md](docs/releasing.md) and [docs/architecture.md](docs/architecture.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Unless you explicitly state otherwise, any
contribution intentionally submitted for inclusion in this project, as defined in the
Apache-2.0 license, shall be dual licensed as above, without any additional terms or
conditions.
