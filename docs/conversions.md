# Writing conversions

> **Status: first draft (Phase 1).** The schema is versioned (`schema = 1`); fields may still
> be added before v0.1.0, but existing ones will keep their meaning.

A *conversion* is one TOML file that tells *That's it* which files it accepts, which
tools it needs, and how to produce the result. You can add a conversion **without touching
any Rust code**.

## Where conversion files live

Files are loaded in this order; a later file with the same `id` **replaces** an earlier one:

1. **Bundled:** `That's it.app/Contents/Resources/conversions/*.toml`. These are reviewed in pull requests
   (source: [`conversions/`](../conversions) in this repository).
2. **User:** `~/Library/Application Support/io.github.federicosimoni.thatsit/conversions/*.toml`

Only `*.toml` files directly inside these folders are read; hidden files (starting with
`.`) are ignored. Invalid files are skipped and reported in the log and in Preferences.

> ⚠️ **Trust:** a user conversion file runs local commands **with your privileges**, just
> like a script you run yourself. Only install conversion files you have read and trust.

## A complete example

```toml
schema     = 1
id         = "image.heic-to-jpg"
category   = "images"
icon       = "photo"
accepts    = ["public.heic", "public.heif"]
accepts_ext = ["heic", "heif"]
output_ext = "jpg"
requires   = ["sips"]

[label]
en = "Convert to JPG"
it = "Converti in JPG"

[[steps]]
cmd = ["sips", "-s", "format", "jpeg", "{input}", "--out", "{output}"]
```

## Fields

| Field | Required | Description |
|---|---|---|
| `schema` | yes | Must be `1`. |
| `id` | yes | Unique identifier: lowercase letters and digits separated by `.`, `-` or `_`, e.g. `image.heic-to-jpg`. Bundled files must be named `<id>.toml`. |
| `category` | yes | One of `images`, `audio-video`, `documents`, `utilities`. Used to group the ring when more than 8 conversions match. |
| `icon` | no (yes for bundled) | An [SF Symbol](https://developer.apple.com/sf-symbols/) name, e.g. `photo`, `waveform`, `doc.zipper`. |
| `accepts` | one of these two | [UTType](https://developer.apple.com/documentation/uniformtypeidentifiers/system-declared_uniform_type_identifiers) identifiers. **Conformance is honoured:** `public.image` matches every image, `public.item` matches every file and folder. |
| `accepts_ext` | one of these two | File extensions, as a fallback (case-insensitive, with or without the leading dot). |
| `output_kind` | no | `"file"` (default) or `"directory"`. A directory output is named after the input (`report.pdf` → `report/`) and is meant for multi-file results such as one image per PDF page. |
| `output_ext` | for `output_kind = "file"` | Extension of the result, e.g. `jpg`. Must not be set for directory outputs. |
| `requires` | no | Bare names of the command-line tools the steps use. If any is missing, the conversion is **hidden** (Preferences explains what to install). |
| `[label]` | yes | Localized menu label per language tag. `en` is required and is the fallback. |
| `[[steps]]` | yes | One or more steps, run in order. |

### Steps

Each step is **either** a command **or** a builtin engine.

```toml
[[steps]]
cmd = ["afconvert", "-f", "m4af", "-d", "aac", "{input}", "{output}"]
timeout_secs = 600            # optional, default 300, max 7200
```

```toml
[[steps]]
engine = "builtin:pdf-to-images"
args   = { format = "png", dpi = 144 }
```

| Field | Description |
|---|---|
| `cmd` | The argv array. `cmd[0]` is either a bare tool name that **must be listed in `requires`**, or an absolute path. It cannot contain placeholders. |
| `engine` | A builtin engine, `builtin:<name>` (the list of engines is documented in Phase 3). |
| `args` | Engine options (only with `engine`). String values may use placeholders. |
| `timeout_secs` | Per-step timeout in seconds (1–7200, default 300). |

At least one step must write the result: either a `cmd` argument contains `{output}`, or
the step is an engine (engines always receive the output path).

### Placeholders

| Placeholder | Value |
|---|---|
| `{input}` | Absolute path of the input file or folder. |
| `{output}` | Absolute path the result must be written to (a hidden temporary path; *That's it* moves it into place afterwards). It keeps the real extension, so tools that pick the format from the extension work. For directory outputs it is an existing, empty folder. |
| `{stem}` | Input name without its extension (`photo` for `photo.heic`). |
| `{dir}` | Folder containing the input. |
| `{tmpdir}` | A private, empty temporary folder for this job. It is deleted afterwards and is also the working directory of every command. Use it to pass data between steps. |

Placeholders can appear anywhere inside an argument (`"{tmpdir}/icon.iconset"`). Use `{{`
and `}}` for literal braces.

## Security model

- Commands run as **argv arrays, never through a shell**. Placeholders are substituted
  inside individual arguments only: there is no word splitting, no globbing, no `$(…)`.
  A file called `$(rm -rf ~).png` is just a strange file name.
- The program (`cmd[0]`) is fixed in the file and cannot come from a placeholder.
- Every step has a timeout. On timeout or cancel, the whole process group of the step is
  killed (including helper processes it started).
- Results are written to a temporary path and then moved next to the original without ever
  overwriting an existing file (`photo.jpg` → `photo (1).jpg`). The source file is never
  modified or deleted. If the source folder is not writable, the result goes to
  `~/Downloads` and the notification says so.

## Finding tools

Apps started from the Finder do not see your shell's `PATH`. *That's it* searches `PATH`
plus `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `/usr/sbin` and `/bin`, and passes
the same list as `PATH` to the commands it runs.

## Testing your conversion

1. Copy your file into the user folder (see above).
2. Open Preferences: invalid files are listed with the reason.
3. Drag a matching file onto the widget.

To contribute a bundled conversion, add the file to [`conversions/`](../conversions) and
run `cargo test`: the test suite validates every bundled file.
