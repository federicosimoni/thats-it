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

## Signing and notarization

Without an Apple Developer account the app is signed **ad hoc**, so Gatekeeper blocks the
first launch of a downloaded copy: **System Settings → Privacy & Security → Open Anyway**,
or `xattr -dr com.apple.quarantine "/Applications/That's it.app"`.

Developer ID signing (hardened runtime) and notarization are already wired into
`cargo xtask dmg` and the Release workflow; they switch on when the secrets exist.

### One-time setup (needs the paid Apple Developer Program)

1. In Xcode → Settings → Accounts → Manage Certificates, create a
   **Developer ID Application** certificate. In Keychain Access, export it with its private
   key as `certificate.p12` with a password.
2. In [App Store Connect → Users and Access → Integrations → App Store Connect API](https://appstoreconnect.apple.com/access/integrations/api),
   create a key with the *Developer* role. Download the `.p8` (only possible once) and note
   the **Key ID** and the **Issuer ID**.
3. Add these secrets to `federicosimoni/thats-it` (Settings → Secrets and variables →
   Actions):

   | Secret | Value |
   |---|---|
   | `MACOS_CERTIFICATE` | `base64 -i certificate.p12 \| pbcopy` |
   | `MACOS_CERTIFICATE_PASSWORD` | the export password |
   | `MACOS_SIGN_IDENTITY` | e.g. `Developer ID Application: Federico Simoni (TEAMID)` (`security find-identity -v -p codesigning`) |
   | `APPLE_API_KEY` | `base64 -i AuthKey_XXXX.p8 \| pbcopy` |
   | `APPLE_API_KEY_ID` | the Key ID |
   | `APPLE_API_ISSUER` | the Issuer ID |

The next release is then signed, notarized and stapled; the release notes and the
Homebrew cask drop the "Open Anyway" instructions automatically. Afterwards, remove the
Gatekeeper notes from the READMEs.

### Locally

```sh
export THATSIT_SIGN_IDENTITY="Developer ID Application: Federico Simoni (TEAMID)"
xcrun notarytool store-credentials thatsit   # once: stores the credentials in the keychain
export THATSIT_NOTARY_PROFILE=thatsit
cargo xtask dmg
```

## Cutting a release

1. Update `version` in the root `Cargo.toml` (`[workspace.package]`) and `CHANGELOG.md`.
2. Commit and push to `main`, then either push a tag (`git tag v0.1.0 && git push origin
   v0.1.0`) or run **Actions → Release → Run workflow** on `main`, which creates the tag
   `v<version>` itself.
3. The **Release** workflow tests, builds the universal `.dmg`, creates the GitHub Release
   with install notes and a SHA-256, and updates the Homebrew tap.

## Homebrew tap (one-time setup)

1. Create the public repository `federicosimoni/homebrew-tap` (done; it holds
   `Casks/thats-it.rb` and a README).
2. Create a fine-grained token with *Contents: read and write* on that repository only and
   save it as the `HOMEBREW_TAP_TOKEN` secret of `federicosimoni/thats-it`.
3. From the next release, users install with
   `brew install --cask federicosimoni/tap/thats-it`.

The cask template is `packaging/homebrew/thats-it.rb`. Without the secret the workflow just
skips the tap update.
