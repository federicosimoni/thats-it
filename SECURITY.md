# Security policy

## Supported versions

Only the latest release receives security fixes.

## Reporting a vulnerability

Please **do not open a public issue**. Report it privately through GitHub:
[Report a vulnerability](https://github.com/federicosimoni/thats-it/security/advisories/new)
(repository **Security** tab → **Report a vulnerability**).

Include what you found, how to reproduce it and the version you used. You will get an
answer within a week. Once a fix is released, the advisory is published and you are
credited unless you prefer otherwise.

## Scope

In scope, for example:

- a bundled conversion, a file name or file contents that make That's it run a command or
  write a file it should not (commands are run as argv arrays and never through a shell);
- a way to make That's it overwrite, move or delete a user's original file;
- problems in the release artifacts (the `.dmg`, the Homebrew cask).

Out of scope: **user conversion files**. A `.toml` file in
`~/Library/Application Support/io.github.federicosimoni.thatsit/conversions/` runs local
commands with your privileges by design, exactly like a script you install yourself. Only
add conversion files you trust. See
[docs/conversions.md](docs/conversions.md#security-model).
