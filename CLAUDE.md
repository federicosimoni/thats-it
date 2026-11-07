# That's it — project brief for Claude Code

> **That's it — drag & drop file converter for macOS**
> *Drop it. Pick it. That's it.* — IT: *Trascina. Scegli. Ecco fatto.*

This file is the single source of truth for the project. Read it fully before doing anything.
Decisions marked **DECIDED** are settled with the owner; do not change them without asking.

---

## 0. Working rules

- **Talk to the owner (Federico) in Italian.** Everything that lives in the repo — code, comments,
  commit messages, docs, issue templates — is in **English**.
- Work phase by phase (see §9). At the end of each phase **stop**, tell the owner how to test it
  on his Mac, and wait for his feedback before starting the next phase.
- Before every commit: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Small, focused commits with conventional messages (`feat:`, `fix:`, `docs:`, `chore:`…).
- Do not add a dependency without a one-line justification in the commit message.
- Keep `unsafe` to the objc2 boundary; every `unsafe` block gets a `// SAFETY:` comment.
- Ask before any decision that changes architecture, adds a new runtime dependency on an external
  tool, or affects distribution.
- Prefer native macOS capabilities over bundling third-party binaries.

---

## 1. Identity — DECIDED

| Item | Value |
|---|---|
| Display name (app, README, website) | **That's it** (with apostrophe) |
| Subtitle / GitHub description | *That's it — drag & drop file converter for macOS* |
| Tagline EN / IT | *Drop it. Pick it. That's it.* / *Trascina. Scegli. Ecco fatto.* |
| GitHub repository | `federicosimoni/thats-it` |
| Homebrew cask | `thats-it` (first in own tap `federicosimoni/homebrew-tap`) |
| Bundle identifier | `io.github.federicosimoni.thatsit` |
| Rust crate prefix | `thatsit-` |
| License | Dual **MIT OR Apache-2.0** |
| Default / fallback language | **English** |
| Launch languages | English + Italian |

The name is never translated; only the tagline and UI strings are.

---

## 2. What the product does

A small round widget lives **on the macOS desktop** (above the desktop icons, below every app window).
When the user drags one or more files over it, the widget **expands into a radial (pie) menu** showing
only the conversions compatible with the dragged file type. The user moves the cursor onto a sector
and releases: the conversion runs and the result is saved next to the original file.

UX details:
- Collapsed state: a small circular icon (≈ 64–80 pt).
- **Repositioning — DECIDED:** only with **Option + drag** (a plain drag does nothing), snapping to one of
  **5 anchors**: top-left, top-centre, top-right, bottom-right, bottom-left (no bottom-centre: Dock).
  Anchors are inset from the screen edges so the fully expanded ring always fits. Option + click
  (no drag) jumps to the next anchor clockwise. The anchor is persisted per screen; default bottom-right.
- **Catch area — DECIDED:** as soon as a compatible drag enters the icon, the window grows to the ring
  plus a wide invisible margin, so overshooting the ring does not close it; leaving the window closes
  the ring after a short grace period (≈ 350 ms) unless the drag comes back.
- Drag enters the collapsed circle → short hover delay (≈ 150 ms) → ring expands with animation.
- Cursor angle selects the highlighted sector; a **central dead zone** cancels (drop there = nothing).
- Max **8 sectors** per ring. If more conversions match, group them by category in the first ring
  and open a second ring for the chosen category.
- Drag leaves the expanded area → ring collapses. Drop on a sector → ring collapses, progress ring
  appears on the collapsed icon, success/failure feedback + system notification at the end.
- **Multiple files:** show only conversions valid for *all* dragged files (intersection); run as a batch.
- Menu bar icon (status item) with: Preferences, Pause/Hide widget, Open conversions folder, About, Quit.
- No Dock icon (`LSUIElement = true`, activation policy `Accessory`).

---

## 3. Tech stack — DECIDED

- **Language:** Rust (stable), edition 2024.
- **UI:** native **AppKit + Core Animation via objc2** — no webview, no Tauri, no cross-platform GUI toolkit.
  - Crates: `objc2` (0.6.x), `objc2-foundation`, `objc2-app-kit` (0.3.x), `objc2-quartz-core`,
    `objc2-core-graphics`, `dispatch2`; plus `objc2-user-notifications`, `objc2-service-management`,
    `objc2-pdf-kit`, `objc2-uniform-type-identifiers` where needed. **Verify the latest versions
    on crates.io before pinning.**
- **Platform:** macOS only for now. Minimum **macOS 13 Ventura** (needed for `SMAppService` login item).
- **Architectures:** universal binary (arm64 + x86_64).
- **Conversion engine:** hybrid (see §6) — Rust crates + built-in macOS tools + optional `ffmpeg`.
- **Conversions are defined in external TOML files** ("conversions"), not hard-coded (see §5).
- **i18n:** Project Fluent (see §7).

---

## 4. Architecture

```
thats-it/
├── Cargo.toml                    # workspace
├── crates/
│   ├── thatsit-core/             # platform-independent, fully unit-tested
│   │   ├── conversion/           # TOML schema, loading, validation, registry, matching
│   │   ├── geometry.rs           # radial layout + point → sector hit-testing
│   │   ├── pipeline.rs           # step execution, placeholder substitution, timeouts, cancel
│   │   ├── output.rs             # output naming, collision handling, atomic write
│   │   ├── tools.rs              # external binary discovery
│   │   ├── i18n.rs               # Fluent bundles + language negotiation
│   │   └── engines/              # builtin engines that are pure Rust
│   └── thatsit-app/              # macOS app (objc2): window, view, drag&drop, menu bar,
│                                 # preferences, notifications, AppKit/PDFKit-based engines
├── conversions/                  # bundled conversion definitions (*.toml)
├── locales/{en,it}/main.ftl      # UI strings
├── resources/                    # Info.plist template, icons, *.lproj (InfoPlist.strings)
├── xtask/                        # cargo xtask: bundle .app, universal binary, sign, dmg
├── docs/                         # conversions.md (schema reference), translating.md, architecture.md
└── .github/                      # CI, release workflow, issue templates
```

Rule: everything that can be tested without AppKit lives in `thatsit-core`. The app crate is a thin
native shell.

### 4.1 The desktop window (thatsit-app)

- `NSWindow` borderless, `opaque = false`, `backgroundColor = clearColor`, `hasShadow = false`.
- Window level: `CGWindowLevelForKey(kCGDesktopIconWindowLevelKey) + 1` → above desktop icons,
  below all normal windows.
- `collectionBehavior = CanJoinAllSpaces | Stationary | IgnoresCycle` → visible on every Space,
  not in Cmd+Tab, not moved by Mission Control.
- **Frame sizing:** while collapsed the window frame is only the icon size (so it never blocks clicks
  on desktop icons underneath). On drag enter it grows (centered) to the expanded ring size and shrinks
  back on exit/drop.
- The snap anchors keep the fully expanded ring inside the screen's `visibleFrame` (still clamped as a safety net).
- Multi-monitor: user chooses the screen in Preferences; anchor persisted per screen.
- Rendering: custom `NSView` subclass (objc2 `define_class!`) with layer-backed drawing
  (`CAShapeLayer` per sector) for smooth expand/highlight animations. Support light/dark mode.

### 4.2 Drag & drop

- The view calls `registerForDraggedTypes:` with `NSPasteboardTypeFileURL`.
- Implement `NSDraggingDestination`:
  - `draggingEntered:` → read file URLs from the dragging pasteboard
    (`readObjectsForClasses:` with `NSURL` + `NSPasteboardURLReadingFileURLsOnlyKey`), resolve
    their UTTypes, ask the core registry for matching conversions. If none match → show a
    "not supported" state and return `NSDragOperationNone`.
  - `draggingUpdated:` → convert cursor to view coordinates → `geometry::hit_test` → highlight sector.
  - `draggingExited:` → collapse.
  - `performDragOperation:` → start the selected conversion (or cancel if in dead zone).
- Return `NSDragOperationCopy` so the Finder never moves the original file.

### 4.3 Jobs

- Conversions run off the main thread (thread pool or tokio — choose the simpler that fits);
  UI updates are dispatched back to the main queue (`dispatch2`).
- Each step has a timeout; jobs are cancellable from the menu bar.
- Notifications via `UNUserNotificationCenter` (works only from a bundled .app — in `cargo run`
  dev mode, fall back to logging).
- Logs: `~/Library/Logs/io.github.federicosimoni.thatsit/`.

---

## 5. Conversion definitions — the "conversions" (TOML)

Users and contributors add conversions **without touching Rust**: one TOML file per conversion.
This is the main entry point for contributors ("add conversion X" = typical *good first issue*).

Load order (later overrides earlier by `id`):
1. Bundled: `That's it.app/Contents/Resources/conversions/*.toml`
2. User: `~/Library/Application Support/io.github.federicosimoni.thatsit/conversions/*.toml`

Draft schema (finalise in Phase 1, document in `docs/conversions.md`, version it with `schema = 1`):

```toml
schema   = 1
id       = "image.heic-to-jpg"
category = "images"                 # images | audio-video | documents | utilities
icon     = "photo"                  # SF Symbol name
accepts  = ["public.heic", "public.png", "public.tiff"]   # UTType identifiers (preferred)
# accepts_ext = ["heic"]            # optional extension fallback
output_ext = "jpg"
requires = ["sips"]                 # binaries that must exist, else conversion is hidden

[label]                             # localized; falls back to "en"
en = "Convert to JPG"
it = "Converti in JPG"

[[steps]]                           # either a command…
cmd = ["sips", "-s", "format", "jpeg", "{input}", "--out", "{output}"]

# …or a builtin engine:
# [[steps]]
# engine = "builtin:image"
# args   = { format = "jpeg", quality = 85 }
```

Placeholders: `{input}`, `{output}`, `{stem}`, `{dir}`, `{tmpdir}`. Multi-step pipelines pass data
through `{tmpdir}`.

**Security rules (non-negotiable):**
- Commands are executed as **argv arrays — never through a shell**. Placeholders are substituted
  inside individual arguments only; no word splitting, no globbing.
- Validate every TOML on load; invalid files are skipped and reported in the log and Preferences.
- Document in `docs/conversions.md` that user conversion files run local commands with the user's
  privileges (same trust level as their own scripts). Bundled conversions are reviewed in PRs.

**Tool discovery:** GUI apps launched from Finder do **not** inherit the shell `PATH`.
Search `PATH` plus `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `/usr/sbin`, `/bin`.
Cache results; re-scan when Preferences opens.

**Output rules:**
- Save next to the original: `photo.heic` → `photo.jpg`; on collision → `photo (1).jpg`, `photo (2).jpg`…
- Write to a temp file in the same directory, then atomic rename. Never overwrite, never delete the source.
- If the source directory is not writable → fall back to `~/Downloads` and say so in the notification.

---

## 6. Conversion catalogue for v1 — DECIDED categories, engines to be verified

All four categories ship in v1. Engines in order of preference; **verify each on a real Mac in the
relevant phase** and adjust.

**Images**
- PNG ⇄ JPG ⇄ TIFF ⇄ BMP ⇄ GIF → `builtin:image` (`image` crate)
- → WebP → `image` crate, **lossless only — DECIDED** (no extra encoder; lossy WebP maybe in v2)
- HEIC → JPG/PNG → `sips`
- SVG → PNG → `resvg`
- PNG → ICNS → `iconutil` (build an `.iconset` with `sips` resizes first)
- Resize 50% / to max 1920 px → `sips -Z` or `image` crate
- Image(s) → PDF → `sips -s format pdf`

**Audio / Video**
- WAV / AIFF / M4A (AAC) / CAF → `afconvert` (built-in). **afconvert cannot encode MP3.**
- → MP3 → `ffmpeg` (`requires = ["ffmpeg"]`)
- MOV → MP4, compress video, video → GIF (palettegen/paletteuse), extract audio → `ffmpeg`
- If ffmpeg is missing, those conversions are hidden; Preferences shows "Install ffmpeg to unlock
  video conversions: `brew install ffmpeg`".

**Documents**
- DOCX / DOC / RTF / ODT / HTML / TXT ⇄ each other → `textutil` (built-in)
- Those → PDF → native: load into `NSAttributedString`, render to PDF via AppKit print-to-file
  (`builtin:doc-to-pdf`, lives in the app crate). Evaluate `cupsfilter` only as fallback.
- Markdown → HTML → `pulldown-cmark`; Markdown → PDF/DOCX = MD → HTML → (doc-to-pdf | textutil)
- PDF → PNG (all pages) → PDFKit (`builtin:pdf-to-images`)

**Utilities**
- Compress to ZIP → `ditto -c -k --sequesterRsrc --keepParent` (native, keeps macOS metadata)
- Strip metadata (EXIF/GPS) without re-encoding → `img-parts` crate (`builtin:strip-metadata`)
- Optimize PNG losslessly → `oxipng` crate (`builtin:optimize-png`)

---

## 7. Localization — DECIDED

- **UI strings:** Project Fluent (`fluent-bundle`, `fluent-langneg`, `unic-langid`).
  Files: `locales/<lang>/main.ftl`. English is the reference and the final fallback.
  Use Fluent plurals (e.g. "1 file converted" / "3 files converted").
- **Language detection:** use the macOS preferred languages *as resolved for this app bundle*
  (respect the per-app language setting in System Settings). Negotiation: exact (`it-CH`) → base
  (`it`) → `en`.
- **Bundle:** add `resources/<lang>.lproj/InfoPlist.strings` and `CFBundleLocalizations` in Info.plist
  so macOS knows the supported languages.
- **Conversion labels:** the `[label]` table in each TOML (fallback `en`).
- **Formatting:** file sizes and numbers through native formatters (`NSByteCountFormatter`,
  `NSNumberFormatter`) so they follow the locale.
- **RTL languages:** the ring is symmetric; just make text alignment direction-aware.
- **Community translations:** Weblate (free hosting for open source). CI warns (does not fail) on keys
  present in `en` but missing in other locales; fails on keys present in a locale but not in `en`.

---

## 8. Distribution & open-source hygiene

- `cargo xtask bundle` → builds both architectures, `lipo` into a universal binary, assembles
  `That's it.app` (Info.plist, icons, resources, conversions, locales, lproj).
- Signing & notarization: Developer ID Application cert, hardened runtime, `notarytool`, `stapler`.
  Requires a paid Apple Developer account (owner gets it last — see §10); already wired in xtask and the Release workflow, switched on by secrets (docs/releasing.md). Secrets only in GitHub
  Actions secrets, never in the repo.
- Release artifact: `.dmg` attached to GitHub Releases, built by a GitHub Actions workflow on a
  macOS runner when a `v*` tag is pushed.
- Homebrew cask in `federicosimoni/homebrew-tap` first; submit to the official cask repo later.
- Login item ("Open at login") via `SMAppService.mainApp`.
- Repo files: `README.md` (EN, with demo GIF at the top), `README.it.md`, `LICENSE-MIT`,
  `LICENSE-APACHE`, `CONTRIBUTING.md` (how to write a conversion, how to translate, dev setup),
  `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CHANGELOG.md`, issue templates
  ("Bug report", "Conversion request", "Translation"), labels incl. `good first issue`.
- CI on every PR: fmt, clippy, tests (core on Linux + macOS), build the app on macOS,
  validate all bundled conversion TOMLs.

---

## 9. Roadmap — phases (stop after each one)

**Phase 0 — Spike (validate the riskiest part)**
Minimal app: transparent borderless window at desktop-icon level + 1, visible on all Spaces,
accepting file drags. On drag: log file paths, UTTypes, cursor angle and computed sector index.
*Acceptance:* owner can drag a file from Finder onto the widget on the desktop and sees correct
sector numbers in the log; desktop icons under the collapsed widget remain clickable.

**Phase 1 — Core**
Workspace layout, `thatsit-core`: TOML schema + validation + registry + UTType matching,
geometry, pipeline executor (argv, placeholders, timeouts, cancel), output naming, tool discovery.
Unit tests for everything. `docs/conversions.md` first draft.

**Phase 2 — Radial UI**
Layer-based ring, expand/collapse animations, hover delay, dead zone, highlight, categories +
second ring, multi-file intersection, light/dark mode, Option-drag to snap anchors, persisted anchor.

**Phase 3 — Engines & conversions**
Builtin engines (§6) and the full v1 set of bundled conversion TOMLs. Progress ring, notifications,
error reporting.

**Phase 4 — Menu bar, Preferences, i18n**
Status item, Preferences window (screen, size, open at login, enabled conversions, output fallback,
detected tools), Fluent integration, EN + IT complete.

**Phase 5 — Packaging & release**
xtask bundle, universal binary, signing/notarization workflow, dmg, Homebrew tap.

**Phase 6 — Open-source launch**
README with GIF, all community files, issue templates/labels, Weblate setup, first `v0.1.0` release.

**v2 — DECIDED with the owner (in this order, one phase each, release + owner test after each):**
1. **Finder service + Shortcuts, without Swift.** NSServices "Convert with That's it…" (menu of
   compatible conversions at the pointer) and a documented command line (`--convert`, `--list`) for
   the Shortcuts action "Run Shell Script" (`docs/shortcuts.md`). Real App Intents need Swift + full
   Xcode: an architecture change, to be evaluated separately only if needed.
2. **Summon mode.** Watch the drag pasteboard's `changeCount` while a file drag is in progress; show
   the widget **only when the desktop is covered** (full-screen app / no desktop visible), **near the
   cursor, offset to the side**, after a short delay (≈ 0.5 s), in a window above full-screen apps.
3. **Conversion options as variants in the ring.** Resting on a conversion opens a second ring of
   variants (e.g. JPG quality High/Medium/Low) declared in the TOML; dropping immediately uses the
   default. Candidates: JPG quality, video compression, resize steps, PDF→PNG DPI.

**Later:** Windows/Linux ports.

---

## 10. Known constraints & open questions

- The widget is on the desktop layer: it is hidden behind full-screen apps by design (v2 summon mode
  addresses this).
- Pure-Rust WebP encoding is lossless only.
- `afconvert` does not encode MP3 → ffmpeg needed for MP3.
- Notifications need a bundled, signed app.
- **Apple Developer account — DECIDED:** the owner gets it only at the very end, once every phase is
  closed. Until then releases are ad-hoc signed (Gatekeeper "Open Anyway"); Developer ID signing and
  notarization are the last step of the project.

**Visual style — DECIDED (Phase 2): direction A "Native glass".**
- Ring and collapsed icon use real macOS vibrancy (`NSVisualEffectView`, behind-window blending),
  thin separators, SF Symbols, system font; adapts to light/dark automatically.
- Highlighted sector: **denser glass with a border** by default. Preference
  "Use system accent color" (**off by default**) switches the highlight and the progress ring to
  `NSColor.controlAccentColor`.
- Collapsed icon: glass disc with a "tray with down arrow" glyph; progress ring around it while
  converting. App icon follows the same motif.
- Mockups: https://claude.ai/artifact/ERwoMcB9cLcUcGsqf9wpw9 (private to the owner).
