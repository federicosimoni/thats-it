# Contributing to That's it

Thanks for helping! There are three common ways to contribute, from easiest to hardest:

1. **Add a conversion** — one TOML file, no Rust needed.
2. **Translate** — on Weblate or by editing one `.ftl` file.
3. **Fix a bug or build a feature** — in Rust.

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md). Please report
security problems privately, as described in [SECURITY.md](SECURITY.md).

## Add a conversion

Every conversion in the ring is a file in [`conversions/`](conversions). Issues labelled
[`good first issue`](https://github.com/federicosimoni/thats-it/labels/good%20first%20issue)
and [`conversion`](https://github.com/federicosimoni/thats-it/labels/conversion) are a good
place to start.

1. Copy a similar file, e.g. `conversions/image.to-png.toml`, and give it a new `id`
   (`<category>.<what-it-does>`, the file name is the id plus `.toml`).
2. Set `accepts` to the [UTType identifiers](https://developer.apple.com/documentation/uniformtypeidentifiers/system-declared_uniform_type_identifiers)
   it works on, `output_ext`, an [SF Symbol](https://developer.apple.com/sf-symbols/) as
   `icon`, and an English label (an Italian one too if you can).
3. Write the steps: a command as an argv array, or a builtin engine. List every external
   program in `requires`.
4. Try it: put the file in the user conversions folder (menu bar → Open Conversions
   Folder) and drag a file onto the widget, or run
   `cargo run -p thatsit-app -- --convert <id> <file>`.
5. Run `cargo test`, which validates every bundled conversion, and open a pull request.

The whole schema is in [docs/conversions.md](docs/conversions.md).

**Rules for bundled conversions**

- Prefer tools that ship with macOS (`sips`, `afconvert`, `textutil`, `ditto`…) or a
  builtin engine. A conversion that needs something from Homebrew must say so in
  `requires`; it is hidden when the tool is missing. New hard dependencies on external
  tools are discussed in an issue first.
- Never overwrite or delete the input. Write only to `{output}` and `{tmpdir}`.
- Keep it to one obvious result. Options such as quality sliders are not supported yet.

## Translate

See [docs/translating.md](docs/translating.md). In short: use Weblate, or copy
`locales/en/main.ftl` to `locales/<lang>/main.ftl`, translate the values and open a pull
request. The app name *That's it* is never translated.

## Development setup

You need macOS 13 or later, Xcode Command Line Tools (`xcode-select --install`) and stable
Rust from [rustup](https://rustup.rs).

```sh
git clone https://github.com/federicosimoni/thats-it.git
cd thats-it
cargo run -p thatsit-app           # the widget, logging to the terminal
cargo test                         # all tests
cargo xtask bundle --native        # target/bundle/That's it.app for your Mac
```

The core crate (`crates/thatsit-core`) is platform-independent, so `cargo test -p
thatsit-core` also works on Linux. The app crate only builds on macOS. Notifications work
only from the bundled app; under `cargo run` they are written to the log. Logs are in
`~/Library/Logs/io.github.federicosimoni.thatsit/`.

The code layout is described in [docs/architecture.md](docs/architecture.md).

## Pull requests

Before pushing, run:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

- Keep pull requests small and focused, one topic each.
- Use [conventional commit](https://www.conventionalcommits.org) messages: `feat:`, `fix:`,
  `docs:`, `chore:`, `ci:`, `test:`…
- Code, comments and docs are in English.
- A new dependency needs a one-line reason in the commit message.
- `unsafe` is only used at the objc2/AppKit boundary, and every `unsafe` block has a
  `// SAFETY:` comment explaining why it is sound.
- Anything that can be tested without AppKit belongs in `thatsit-core`, with unit tests.
- User-visible changes get a line in [CHANGELOG.md](CHANGELOG.md) under *Unreleased*.

CI runs formatting, clippy and tests on Linux and macOS, builds the app, runs every bundled
conversion with the real macOS tools and takes screenshots of the ring.

## License

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this project, as defined in the Apache-2.0 license, shall be dual licensed
under MIT OR Apache-2.0, without any additional terms or conditions.
