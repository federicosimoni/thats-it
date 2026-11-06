# Homebrew cask for That's it — template.
# The release workflow fills in `version` and `sha256` and pushes the result to
# federicosimoni/homebrew-tap (Casks/thats-it.rb) when HOMEBREW_TAP_TOKEN is set.
cask "thats-it" do
  version "@VERSION@"
  sha256 "@SHA256@"

  url "https://github.com/federicosimoni/thats-it/releases/download/v#{version}/Thats-it-#{version}.dmg"
  name "That's it"
  desc "Drag & drop file converter for macOS"
  homepage "https://github.com/federicosimoni/thats-it"

  depends_on macos: ">= :ventura"

  app "That's it.app"

  zap trash: [
    "~/Library/Application Support/io.github.federicosimoni.thatsit",
    "~/Library/Logs/io.github.federicosimoni.thatsit",
    "~/Library/Preferences/io.github.federicosimoni.thatsit.plist",
  ]

  caveats <<~EOS
    That's it is not notarized by Apple yet. On first launch macOS will block it:
    open System Settings → Privacy & Security and click "Open Anyway", or run
      xattr -dr com.apple.quarantine "#{appdir}/That's it.app"
    Video and MP3 conversions need ffmpeg:  brew install ffmpeg
  EOS
end
