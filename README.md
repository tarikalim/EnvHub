# envhub

A small desktop app that finds every `KEY=VALUE` in the `.env` files scattered across your
machine, so you can search them, copy one to the clipboard, and edit it in place — without
opening ten repos to find the one secret someone just asked you for.

Built for local development convenience, not for security: values are stored in your `.env`
files exactly as they already are, and the app only reads and writes those files.

![key icon](tools/icon.png)

## Install

Grab a build from [Releases](../../releases): `envhub-macos-arm64.zip` (unzip, drop
`envhub.app` in `/Applications`) or `envhub-linux-x86_64.tar.gz` (untar, put `envhub`
somewhere on your `PATH`). macOS will warn that the app is unsigned — right-click → Open.

Or build it yourself. Requires Rust, and macOS for the app bundle.

```sh
git clone <this repo> && cd envhub
cargo run --release          # just run it
./tools/install.sh           # or install it as ~/Applications/envhub.app
```

`install.sh` builds the release binary, copies it into the bundle with its icon, and leaves
you an app you can launch from Spotlight.

## Use

- Type in the search box to filter by key or repo; terms are ANDed (`forge oauth`).
- Click a key or a value to copy the value. `copy` does the same; `open` opens the file in `$EDITOR`.
- Above the results there is a second box that narrows whatever is currently listed — handy
  when a single service has a hundred keys. It clears itself when you switch repos.
- The left panel lists every directory that has env files, with key counts. Click one to
  scope the list to it, or `all repos` to search everything.
- `reveal values` unmasks values; `search values` also searches inside them.
- `commented` also lists the keys that are commented out in the files (`# KEY=VALUE`), shown
  dimmed and prefixed with `#`. Editing one keeps it commented.
- `edit mode` turns on inline editing: click a value, type, and click away — only that one
  line of the file is rewritten. Off by default so nothing is changed by accident.
- `+ new` adds a key that belongs to no repo; it goes to `~/.config/envhub/scratch.env`.

## Config

`~/.config/envhub/config` (or `$XDG_CONFIG_HOME/envhub/config`), one directive per line:

```
~/code          # a directory to scan; defaults to $HOME when none are listed
~/work
!dist           # a directory name to skip, on top of the built-in list
```

See `config.example`. `↻ rescan` reloads the config and rescans.

## Shell alternative

If you only want the search-and-copy half, a shell function with ripgrep and fzf covers it:

```zsh
envs() {
  rg --hidden --no-ignore --no-messages -n --no-heading \
     -g '.env*' -g '!*.example' -g '!**/node_modules/**' -g '!**/.git/**' -g '!Library/**' \
     '^[A-Za-z_][A-Za-z0-9_]*=' "$HOME" \
  | sed "s|^$HOME/||" \
  | fzf --delimiter='=' --nth=1 --preview 'echo {2..}' --preview-window=down:3:wrap \
  | cut -d= -f2- | tr -d '\n' | pbcopy
}
```

## License

MIT
