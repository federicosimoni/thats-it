# Architecture

That's it is a Cargo workspace with two crates and a build helper:

```
crates/thatsit-core   platform-independent logic, unit-tested on Linux and macOS
crates/thatsit-app    the macOS app: a thin AppKit shell over the core (objc2)
xtask                 cargo xtask bundle | dmg | icon
conversions/          bundled conversion definitions (*.toml)
locales/<lang>/       UI strings (Project Fluent)
resources/            Info.plist, app icon, *.lproj
```

The rule: anything that can be tested without AppKit lives in `thatsit-core`.

## thatsit-core

| Module | What it does |
|---|---|
| `conversion` | TOML schema, validation, loading (bundled, then user, later wins by `id`), the registry and matching dragged files (UTType + extension) to conversions, intersected for several files |
| `geometry` | Radial layout, point → sector hit-testing, clamping the ring into the screen, the five snap anchors |
| `menu` | The ring's state machine: hover delay, categories and second ring, dwell, drop outcome |
| `placeholder` | `{input}`, `{output}`, `{stem}`, `{dir}`, `{tmpdir}` substitution inside single arguments |
| `pipeline` | Runs the steps of a conversion: argv commands (never a shell) with timeouts and cancellation of the whole process group, or builtin engines |
| `output` | Output naming next to the original, `name (1).ext` on collision, no-clobber commit, fallback folder |
| `tools` | Finds external programs in `PATH` plus the Homebrew and system folders |
| `engines` | Pure-Rust builtin engines: `image`, `svg`, `markdown`, `strip-metadata`, `optimize-png` |
| `i18n` | Fluent bundles and language negotiation (exact → base language → English) |

## thatsit-app

| Module | What it does |
|---|---|
| `main` | Accessory app (no Dock icon), wiring, `--convert` and demo modes |
| `window` | The borderless, transparent window at desktop-icon level + 1, on every Space |
| `ring_view` | The `NSView` subclass: glass disc, sector layers, drag destination, Option-drag snapping, progress ring |
| `files` | Reading file URLs and UTTypes from the dragging pasteboard |
| `catalog` | Loads the registry, checks tools, holds the UI language |
| `jobs` | Runs batches on a worker thread and reports back on the main queue |
| `app_engines` | Engines that need AppKit/PDFKit: `doc-to-pdf`, `pdf-to-images` |
| `notify` | User notifications (logged when not running as a bundled app) |
| `status`, `prefs_window`, `prefs` | Menu bar item, Preferences window, `NSUserDefaults` |
| `log` | Log file in `~/Library/Logs/io.github.federicosimoni.thatsit/` |

## A drop, end to end

1. The Finder drag enters the collapsed disc. `ring_view` reads the files, asks the
   registry for matching conversions and, if there are any, grows the window to the
   catch area.
2. After the hover delay `menu::Interaction` opens the ring; every `draggingUpdated:`
   hit-tests the cursor with `geometry` and the view renders the reported state.
3. On drop, the chosen conversion and the files go to `jobs` as a batch. The worker runs
   the `pipeline` for each file into a temporary folder and `output` moves the result next
   to the original.
4. Progress and the result come back on the main queue: progress ring, result glyph,
   notification.
