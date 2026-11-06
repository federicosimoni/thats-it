# Building and releasing

## Build the app locally (macOS)

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin   # once
cargo xtask bundle            # universal "target/bundle/That's it.app"
cargo xtask bundle --native   # faster: only your Mac's architecture
cargo xtask dmg               # app + target/bundle/Thats-it-<version>.dmg
open "target/bundle/That's it.app"
```

The bundle contains the universal binary, `Info.plist` (from `resources/Info.plist`), the
icon rendered from `resources/AppIcon.svg`, the bundled conversions, the locales and the
`*.lproj` folders. It is signed **ad hoc** (`codesign -s -`).

## Signing status

There is no Developer ID signature or notarization yet, so Gatekeeper blocks the first
launch of a downloaded copy: **System Settings → Privacy & Security → Open Anyway**, or
`xattr -dr com.apple.quarantine "/Applications/That's it.app"`. Adding Developer ID signing
and `notarytool` later only changes the `codesign` step in `xtask` and needs the
certificate and an app-specific password as GitHub Actions secrets.

## Cutting a release

1. Update `version` in the root `Cargo.toml` (`[workspace.package]`) and `CHANGELOG.md`.
2. Commit, then tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
3. The **Release** workflow tests, builds the universal `.dmg`, creates the GitHub Release
   with install notes and a SHA-256, and updates the Homebrew tap.

## Homebrew tap (one-time setup)

1. Create the public repository `federicosimoni/homebrew-tap`.
2. Create a fine-grained token with *Contents: read and write* on that repository only and
   save it as the `HOMEBREW_TAP_TOKEN` secret of `federicosimoni/thats-it`.
3. From the next release, users install with
   `brew install --cask federicosimoni/tap/thats-it`.

The cask template is `packaging/homebrew/thats-it.rb`. Without the secret the workflow just
skips the tap update.
