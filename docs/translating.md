# Translating That's it

*That's it* uses [Project Fluent](https://projectfluent.org/). English is the reference
language and the fallback for anything not translated yet. The app name **That's it** is
never translated.

## What to translate

| What | Where |
|---|---|
| UI strings (menu bar, Preferences, notifications, ring categories) | `locales/<lang>/main.ftl` |
| Conversion names shown in the ring | the `[label]` table of each file in `conversions/` (e.g. `it = "Converti in JPG"`) |
| App name/copyright shown by macOS | `resources/<lang>.lproj/InfoPlist.strings` |

## Adding a language

1. Copy `locales/en/main.ftl` to `locales/<lang>/main.ftl` (e.g. `fr`, `pt-BR`) and translate
   the values; keep every message id (the part before `=`) unchanged.
2. Register it in `crates/thatsit-core/src/i18n.rs` (`LOCALES`) and in
   `resources/Info.plist` (`CFBundleLocalizations`), and add
   `resources/<lang>.lproj/InfoPlist.strings`.
3. Add `<lang> = "…"` labels to the conversion files.
4. Run `cargo test`.

Plurals use Fluent selectors, e.g.

```ftl
summary-converted = { $conversion }: { $count ->
    [one] 1 file converted
   *[other] { $count } files converted
}
```

Keep the variables (`{ $count }`, `{ $file }`…) as they are; you may move them inside the
sentence. Use the plural categories of your language (`one`, `few`, `many`, `other`…).

## Checks

CI runs `cargo test -p thatsit-core --test locales`:

- a message that exists in a translation but **not in English** fails the build (usually a
  typo in the id);
- a message missing from a translation only produces a warning, and English is shown
  until it is translated.

## Which language the app uses

The app follows the language macOS picks for it, including the per-app setting in
**System Settings → General → Language & Region → Applications**. It negotiates exact
match (`it-CH`), then the base language (`it`), then English.

Community translations will be hosted on Weblate (set up at the open-source launch).
