# That's it — English UI strings (reference locale).
# The app name "That's it" is never translated.

## Tagline

tagline = Drop it. Pick it. That's it.

## Ring categories

category-images = Images
category-audio-video = Audio & Video
category-documents = Documents
category-utilities = Utilities
# Category sector label, e.g. "Images (6)".
category-with-count = { $name } ({ $count })

## Result notification

summary-saved = { $conversion }: saved { $file }
summary-converted = { $conversion }: { $count ->
    [one] 1 file converted
   *[other] { $count } files converted
}
summary-fallback = The original folder is not writable, so the result was saved to { $folder }.
summary-failed-one = Failed: { $error }
summary-failed-many = { $count ->
    [one] 1 file failed (see the log)
   *[other] { $count } files failed (see the log)
}
summary-cancelled = Cancelled.

## Menu bar

menu-preferences = Preferences…
menu-show-widget = Show Widget
menu-hide-widget = Hide Widget
menu-cancel = Cancel Conversion
menu-open-conversions = Open Conversions Folder
menu-open-log = Open Log Folder
menu-about = About That's it
menu-quit = Quit That's it

## Preferences window

prefs-title = That's it Preferences
prefs-tab-general = General
prefs-tab-conversions = Conversions
prefs-tab-tools = Tools

prefs-screen = Show the widget on:
prefs-size = Widget size:
prefs-size-small = Small
prefs-size-medium = Medium
prefs-size-large = Large
prefs-login = Open at login
prefs-login-unavailable = Available when running the installed app.
prefs-accent = Use system accent color
prefs-summon = Show the widget near the pointer when the desktop is covered
prefs-fallback = If the original folder is not writable, save to:
prefs-choose = Choose…

prefs-conversions-hint = Unchecked conversions never appear in the ring.
prefs-needs = needs { $tools }
prefs-from = from { $formats }

prefs-tools-hint = Command-line tools used by conversions. Apps opened from the Finder don't see your shell PATH, so That's it also looks in /opt/homebrew/bin and /usr/local/bin.
prefs-tool-found = { $tool }: { $path }
prefs-tool-missing = { $tool }: not found
prefs-ffmpeg-hint = Install ffmpeg to unlock video and MP3 conversions: brew install ffmpeg
prefs-invalid-title = Invalid conversion files
prefs-invalid-none = All conversion files are valid.
prefs-rescan = Rescan

## Finder service (right-click → Services)
service-no-match = No conversion for these files
