# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.4.0] — 2026-11-07

### Added

- Conversion variants: resting on "Convert to JPG …", "Resize by % …", "Resize to max
  size …", "Compress video …" or "Pages to PNG …" opens a second ring with options
  (JPG quality, 25/50/75%, 1280/1920/3840 px, compression strength, 72/144/300 dpi).
  A quick drop uses the default. The Finder service shows them as a submenu; the command
  line takes `<id>:<variant>`.
- `[[variants]]` in conversion files, with `{name}` variables (see `docs/conversions.md`).

## [0.3.0] — 2026-11-07

### Added

- Summon mode: when the desktop widget is covered by windows or a full-screen app, a file
  drag held for half a second shows a widget next to the pointer, above every window.
  Preference: "Show the widget near the pointer when the desktop is covered" (on).

## [0.2.0] — 2026-11-07

### Added

- Finder service **Convert with That's it…** (Control-click files → Services / Quick
  Actions): a menu of the compatible conversions at the pointer, run like a drop.
- Command line for Shortcuts and scripts: `--list [files]` lists the conversion ids,
  `--help` and `--version`; guide in `docs/shortcuts.md`.

## [0.1.2] — 2026-11-07

### Fixed

- Notifications are shown even while That's it is the active app, and permission or
  delivery errors are written to the log.
- The WebP label no longer gets cut off in the ring: it is now "Convert to WebP lossless".

## [0.1.1] — 2026-11-07

### Fixed

- With a PNG, "Convert to WebP" was silently left out of the ring (nine image conversions,
  eight fit). Images → PDF now lives in the Documents category.
- Markdown files no longer show "Convert to Word / HTML / PDF" twice: the Markdown-aware
  conversion wins over the plain-text one.

### Changed

- Preferences show the source formats of conversions that share a label, e.g.
  "Convert to PNG — from HEIC".
- Step-by-step installation guide for the unsigned app in both READMEs and the release
  notes.

## [0.1.0] — 2026-11-07

First public release.

### Added

- Desktop widget: a glass disc above the desktop icons and below all windows, on every
  Space, with no Dock icon.
- Radial menu that opens while files are dragged over the widget, showing only the
  conversions valid for all dragged files; categories and a second ring when more than
  eight match; central dead zone to cancel; light and dark mode.
- Wide catch area around the open ring, with a short grace period when the drag leaves it.
- Option-drag to snap the widget to one of five places on the screen; Option-click to jump
  to the next one. The place is remembered per screen.
- 36 bundled conversions for images, audio and video, documents and utilities, using
  built-in macOS tools, pure-Rust engines and, when installed, ffmpeg.
- Conversions defined in TOML files; user conversions in
  `~/Library/Application Support/io.github.federicosimoni.thatsit/conversions/`.
- Results saved next to the original without ever overwriting it, with a fallback folder
  when the original folder is not writable.
- Progress ring, success/failure feedback and system notifications.
- Menu bar item and Preferences: screen, widget size, open at login, system accent color,
  enabled conversions, fallback folder, detected tools and invalid conversion files.
- English and Italian localization (Project Fluent).
- Universal (Apple silicon + Intel) app bundle and `.dmg`, Homebrew cask.

[Unreleased]: https://github.com/federicosimoni/thats-it/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/federicosimoni/thats-it/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/federicosimoni/thats-it/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/federicosimoni/thats-it/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/federicosimoni/thats-it/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/federicosimoni/thats-it/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/federicosimoni/thats-it/releases/tag/v0.1.0
