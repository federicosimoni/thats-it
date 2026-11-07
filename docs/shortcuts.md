# Finder menu, Shortcuts and the command line

The desktop widget is not the only way in. These work even when the desktop is covered
by windows or a full-screen app.

## Finder: "Convert with That's it…"

1. In the Finder, select one or more files and **Control-click** (or right-click) them.
2. Choose **Quick Actions** or **Services** (depending on the macOS version and how many
   items there are, it may also be listed directly at the bottom of the menu) →
   **Convert with That's it…**.
3. A menu with the conversions for those files appears at the pointer. Pick one: it runs
   exactly like a drop on the widget (progress ring, notification, result next to the
   original).

That's it must be running (it starts at login if you turned on **Open at login**).

**Not in the menu?** Open **System Settings → Keyboard → Keyboard Shortcuts… →
Services → Files and Folders** and turn on **Convert with That's it…**. You can give it a
keyboard shortcut there too. If it still doesn't show up, quit That's it, run
`/System/Library/CoreServices/pbs -update` in Terminal and open the app again.

## Shortcuts

The app has a command line that the Shortcuts action **Run Shell Script** can call:

```sh
"/Applications/That's it.app/Contents/MacOS/thatsit" --convert <conversion-id> <file>...
```

### Example: a "Convert to JPG" Quick Action

1. Open **Shortcuts** and, once only, allow scripts: **Shortcuts → Settings → Advanced →
   Allow Running Scripts**.
2. Create a new shortcut (**+**) and name it *Convert to JPG*.
3. Open the details panel (ⓘ) and turn on **Use as Quick Action**, with **Finder** and
   **Services Menu** checked. Set it to receive **Files**.
4. Add the action **Run Shell Script** and set **Shell** to `zsh`, **Input** to
   *Shortcut Input* and **Pass Input** to *as arguments*. Script:

   ```sh
   "/Applications/That's it.app/Contents/MacOS/thatsit" --convert image.to-jpg "$@"
   ```

5. Optionally add **Show Notification** with the *Shell Script Result*.

Now Control-click images in the Finder → **Quick Actions → Convert to JPG**. The same
shortcut can be run from the menu bar, Spotlight, a keyboard shortcut or Siri.

Conversions with variants (JPG quality, resize steps…) take `<id>:<variant>`, e.g.
`--convert image.to-jpg:high photo.png`; without a variant the default one is used.

### Which id?

List every conversion available on your Mac, or only those that apply to some files:

```sh
"/Applications/That's it.app/Contents/MacOS/thatsit" --list
"/Applications/That's it.app/Contents/MacOS/thatsit" --list ~/Desktop/photo.heic
```

Each line is `id<TAB>label`, grouped by category.

### What the command prints

One line per file — `OK <result path>` or `FAILED <file>: <reason>` — and exit code `0`
when every file was converted, `1` otherwise (so a shortcut can stop on errors). Results
follow the same rules as the widget: next to the original, never overwriting anything.

Conversions started from the command line run in their own process: they don't show the
progress ring or a notification (add **Show Notification** in your shortcut if you want
one). Conversions you switched off in Preferences can still be run by id.

## Command-line reference

```text
thatsit --convert <id>[:<variant>] <file>...  convert files, print OK/FAILED per file
thatsit --list [<file>...]                    list conversions (only those for <file>s)
thatsit --help | --version
```

Tip: `alias thatsit="'/Applications/That'\''s it.app/Contents/MacOS/thatsit'"` in your
`~/.zshrc` makes it `thatsit --list` from any Terminal.
